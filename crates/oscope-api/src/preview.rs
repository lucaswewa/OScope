//! The `preview` Thing (plan §5.4, §5.5; ADR-0014): live previews of the
//! camera's frames, each the size of its client's viewport, as a binary
//! WebSocket at `/api/v1/preview/stream`; their defaults; how each
//! session is doing; and the live recipe run on each (P22), with the
//! references recipes read.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use oscope_acquire::mosaic::LookSource;
use oscope_acquire::preview::wire::{Header, MASK, STATISTICS, TILES};
use oscope_acquire::preview::{
    Adaptation, LiveRecipe, Look, Preview, PreviewError, PreviewRequest, Rendered, RingSettings,
    publish,
};
use oscope_acquire::stats::StatsRequest;
use oscope_compute::{Compute, ComputeConfig, Image, Shape, cpu};
use oscope_core::{Frame, PixelRect, timing};
use oscope_ext::Extensions;
use oscope_imaging::color::{ColourCorrection, Curve, Display, encode_tiff16};
use oscope_jobs::JobService;
use oscope_pipeline::{Descriptor, Engine, Recipe, stock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use teta_wot::http::Subscriber;
use teta_wot::prelude::*;

use crate::batch;
use crate::devices::Devices;
use crate::mosaic::Mosaics;
use crate::server::{RingSlot, Roots};

/// How many messages a client that acknowledges may have unshown.
const AHEAD: u32 = 2;
/// The least time between one client's messages with statistics, from one
/// being sent to the next being measured: 10 a second at most.
const STATS_EVERY: Duration = Duration::from_millis(100);

/// What a client gets unless it asks otherwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewDefaults {
    /// The viewport's width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The JPEG quality, from 1 to 100.
    pub quality: u8,
    /// The most frames a second. The camera's rate is the most it gets.
    pub max_fps: f64,
}

impl Default for PreviewDefaults {
    /// 1920 × 1080, at quality 80 and 30 frames a second at most.
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            quality: 80,
            max_fps: 30.0,
        }
    }
}

/// How previews look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewLook {
    /// The level shown as black, from 0 to 1, once the camera's black level
    /// is taken off.
    pub black: f32,
    /// The level shown as white: above black.
    pub white: f32,
    /// The display's gamma, such as 2.2; none for sRGB's curve.
    pub gamma: Option<f32>,
    /// Red, green and blue's white-balance gains.
    pub gains: [f32; 3],
}

impl Default for PreviewLook {
    /// The whole range, through sRGB's curve, unbalanced.
    fn default() -> Self {
        Self {
            black: 0.0,
            white: 1.0,
            gamma: None,
            gains: [1.0; 3],
        }
    }
}

impl From<&PreviewLook> for Look {
    fn from(look: &PreviewLook) -> Self {
        let curve = look
            .gamma
            .map_or(Curve::Srgb, |gamma| Curve::Gamma { gamma });
        Look {
            display: Display {
                black: look.black,
                white: look.white,
                curve,
            },
            correction: ColourCorrection {
                gains: look.gains,
                ..ColourCorrection::default()
            },
        }
    }
}

/// What a client asks for, as a JSON text message. Each message replaces
/// the last, and what it leaves out takes the `defaults` setting's value.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewParams {
    /// The viewport's width, in pixels.
    pub width: Option<u32>,
    /// Its height.
    pub height: Option<u32>,
    /// The sensor's pixels to show, in full-sensor coordinates: the whole
    /// frame when not given, or when it misses the frame.
    pub roi: Option<PixelRect>,
    /// The JPEG quality, from 1 to 100.
    pub quality: Option<u8>,
    /// The most frames a second.
    pub max_fps: Option<f64>,
    /// The number of the next message the client is ready for: one more
    /// than the last it has shown, and 0 before the first. Once a client
    /// sends it, it has at most 2 messages it hasn't shown, so that no socket
    /// buffer holds stale frames for it (ADR-0014).
    pub ack: Option<u32>,
    /// Live statistics to send with the previews, at most 10 times a
    /// second (ADR-0035): none when not given.
    pub stats: Option<StatsRequest>,
    /// Whether to send the live mosaic's tiles changed since the last
    /// message, as a block with each message that has some (ADR-0026).
    pub mosaic: Option<bool>,
}

impl PreviewParams {
    /// How many of the `sent` messages the client hasn't shown: none for a
    /// client that doesn't acknowledge.
    fn unshown(&self, sent: u32) -> u32 {
        self.ack.map_or(0, |ack| sent.saturating_sub(ack))
    }
}

/// How a client's session is doing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionStats {
    /// The session's number, from 1 since the server started.
    pub session: u64,
    /// Messages sent in the last second.
    pub fps: f64,
    /// The last second's mean time from the end of a frame's exposure to its
    /// message being ready, in ms.
    pub latency_ms: f64,
    /// Bytes sent in the last second.
    pub bytes_per_s: f64,
    /// Messages sent.
    pub sent: u64,
    /// Frames it missed: messages replaced before the client took them, and
    /// frames skipped while it was behind on acknowledging.
    pub dropped: u64,
    /// The quality it gets now, after adapting to its link.
    pub quality: u8,
    /// The width of its latest preview.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Whether the GPU made it.
    pub gpu: bool,
    /// How long the live recipe took on it, in ms, if one is set.
    pub recipe_ms: Option<f64>,
}

/// What a live recipe was checked to take.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecipeCheck {
    /// Its steps, in order.
    pub steps: Vec<String>,
    /// Its time on an RGB image the default viewport's size, in ms.
    pub time_ms: f64,
    /// That image's width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The budget it was checked against, in ms.
    pub budget_ms: f64,
}

/// A reference, captured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Reference {
    /// Its name, which `io.reference` takes.
    pub name: String,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The frames it is the mean of.
    pub frames: u32,
}

/// The widest a reference is kept: references are smooth, and the steps
/// that read them sample them at the size they need.
const REFERENCE_WIDTH: u32 = 1024;

/// A refusal, as ADR-0034 shapes them.
fn refused(why: impl std::fmt::Display) -> ActionError {
    ActionError::handled(format!("refused (400): {why}"))
}

/// Plans `recipe` as a live recipe with `engine`, checks it, and times it
/// on an RGB image of `size`.
fn check(
    engine: &Arc<Engine>,
    recipe: &Recipe,
    size: (u32, u32),
) -> Result<(LiveRecipe, Duration), String> {
    let plan = engine.plan(recipe).map_err(|error| error.to_string())?;
    let live = LiveRecipe::new(Arc::clone(engine), Arc::new(plan))?;
    let time = live
        .time(size.0, size.1)
        .map_err(|error| error.to_string())?;
    Ok((live, time))
}

/// The names of the references in `folder`.
fn references(folder: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(folder).into_iter().flatten().flatten();
    let mut names: Vec<String> = entries
        .map(|entry| entry.path())
        .filter(|path| {
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            matches!(extension, "tiff" | "tif" | "png")
        })
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The last second's messages: when each was sent, its bytes and latency.
#[derive(Default)]
struct Meter(VecDeque<(Instant, usize, f64)>);

impl Meter {
    /// Counts a message: the last second's messages, mean latency and bytes.
    fn add(&mut self, bytes: usize, latency_ms: f64) -> (f64, f64, f64) {
        let now = Instant::now();
        self.0.push_back((now, bytes, latency_ms));
        while self
            .0
            .front()
            .is_some_and(|(at, ..)| now - *at > Duration::from_secs(1))
        {
            self.0.pop_front();
        }
        let n = self.0.len() as f64;
        let latency = self.0.iter().map(|(.., l)| l).sum::<f64>() / n;
        (n, latency, self.0.iter().map(|(_, b, _)| *b as f64).sum())
    }
}

/// What the stream needs, found when the Thing starts.
struct Service {
    devices: Arc<Devices>,
    preview: Preview,
    /// The stock nodes' engine, for live recipes (P22).
    engine: Arc<Engine>,
    /// Where references are kept.
    references: PathBuf,
    /// The live mosaics, whose changed tiles go with the previews.
    mosaics: Option<Arc<Mosaics>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Live previews: the camera's frames as JPEGs, each client's the size of
/// its viewport, through a binary WebSocket (ADR-0014).
#[derive(Thing)]
#[thing(
    title = "Preview",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct PreviewThing {
    /// What a client gets unless it asks otherwise.
    ///
    /// Saved. Clients that didn't ask take a change at once.
    #[setting]
    defaults: Prop<PreviewDefaults>,
    /// How previews look: the levels, the tone curve and white balance.
    ///
    /// Saved.
    #[setting]
    look: Prop<PreviewLook>,
    /// Makes previews on the CPU even when there is a GPU.
    ///
    /// Saved. For comparing them: both give the same pixels.
    #[setting]
    cpu_only: Prop<bool>,
    /// The live recipe, run on each preview in place of the look, or none
    /// (P22, ADR-0017).
    ///
    /// Saved. `set_live_recipe` sets it, once it has checked it.
    #[setting(read_only)]
    live_recipe: Prop<Option<Recipe>>,
    /// The most a live recipe may take, in ms, on an image the default
    /// viewport's size.
    ///
    /// Saved.
    #[setting(default = 20.0, gt = 0, unit = "ms")]
    recipe_budget_ms: Prop<f64>,
    service: OnceLock<Service>,
    sessions: Mutex<BTreeMap<u64, SessionStats>>,
    opened: AtomicU64,
}

impl PreviewThing {
    fn started(&self) -> Result<&Service, ActionError> {
        self.service
            .get()
            .ok_or_else(|| ActionError::handled("the Thing hasn't started"))
    }

    /// Checks `recipe` and times it, then has the previews run it, or none.
    async fn install(&self, recipe: Option<Recipe>) -> Result<Option<RecipeCheck>, ActionError> {
        let service = self.started()?;
        let Some(recipe) = recipe else {
            service.preview.set_recipe(None);
            self.live_recipe
                .set(None)
                .map_err(|e| ActionError::handled(e.to_string()))?;
            return Ok(None);
        };
        let defaults = self.defaults.get();
        let size = (defaults.width.max(1), defaults.height.max(1));
        let (engine, planned) = (Arc::clone(&service.engine), recipe.clone());
        let checked = tokio::task::spawn_blocking(move || check(&engine, &planned, size)).await;
        let checked = checked.map_err(|error| ActionError::handled(error.to_string()))?;
        let (live, time) = checked.map_err(refused)?;
        let (time_ms, budget_ms) = (time.as_secs_f64() * 1e3, self.recipe_budget_ms.get());
        if time_ms > budget_ms {
            let (w, h) = size;
            return Err(refused(format!(
                "it takes {time_ms:.1} ms on a {w} × {h} image, over its budget of {budget_ms} ms"
            )));
        }
        let steps = recipe.nodes.iter().map(|node| node.id.clone()).collect();
        service.preview.set_recipe(Some(live));
        self.live_recipe
            .set(Some(recipe))
            .map_err(|e| ActionError::handled(e.to_string()))?;
        Ok(Some(RecipeCheck {
            steps,
            time_ms,
            width: size.0,
            height: size.1,
            budget_ms,
        }))
    }

    /// How the preview ring's previews are made (ADR-0015): the defaults,
    /// the look and the GPU setting, as they are when each is made.
    fn ring_settings(&self) -> impl Fn() -> RingSettings + Send + 'static {
        let defaults = self.defaults.subscribe();
        let (look, cpu_only) = (self.look.subscribe(), self.cpu_only.subscribe());
        move || {
            let defaults = defaults.borrow().clone();
            RingSettings {
                request: PreviewRequest {
                    viewport: (defaults.width.max(1), defaults.height.max(1)),
                    roi: None,
                    quality: defaults.quality.clamp(1, 100),
                },
                look: Look::from(&*look.borrow()),
                gpu: !*cpu_only.borrow(),
                max_fps: defaults.max_fps,
            }
        }
    }

    /// What `params` asks for, from the defaults where it doesn't, as
    /// `adaptation` lets it have.
    fn request(&self, params: &PreviewParams, adaptation: &Adaptation) -> (PreviewRequest, f64) {
        let defaults = self.defaults.get();
        let scale = adaptation.scale();
        let size = |asked: Option<u32>, default: u32| {
            ((asked.unwrap_or(default) as f32 * scale).round() as u32).max(1)
        };
        let quality = params.quality.unwrap_or(defaults.quality).clamp(1, 100);
        let request = PreviewRequest {
            viewport: (
                size(params.width, defaults.width),
                size(params.height, defaults.height),
            ),
            roi: params.roi,
            quality: adaptation.quality(quality),
        };
        (request, params.max_fps.unwrap_or(defaults.max_fps).max(0.1))
    }
}

#[thing_impl]
impl PreviewThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let devices = ctx
            .server()
            .service::<Devices>()
            .ok_or_else(|| anyhow::anyhow!("the server has no devices"))?;
        let compute =
            tokio::task::spawn_blocking(|| Compute::new(&ComputeConfig::default())).await??;
        let preview = Preview::new(Arc::new(compute));
        let roots = ctx.server().service::<Roots>();
        let references = roots.as_ref().map_or_else(
            || PathBuf::from("references"),
            |r| r.data.join("references"),
        );
        let mut registry = stock::registry(&references);
        // The nodes of the extensions this start loaded (ADR-0019), checked
        // already: no stock node has their IDs.
        let extensions = ctx.server().service::<Extensions>();
        for node in extensions.iter().flat_map(|extensions| extensions.nodes()) {
            if let Err(error) = registry.register(Arc::clone(node)) {
                tracing::warn!("a live recipe can't use an extension's node: {error}");
            }
        }
        let engine = Arc::new(Engine::new(registry, Arc::clone(preview.compute()), 0));
        // Recipes over the camera's frames as batch jobs (Checkpoint B), on
        // the same nodes as live recipes.
        if let Some(jobs) = ctx.server().service::<JobService>() {
            let data = roots
                .as_ref()
                .map_or_else(|| PathBuf::from("."), |r| r.data.clone());
            let made = batch::factory(
                Arc::clone(&engine),
                preview.clone(),
                Arc::clone(&devices),
                data.join("batch"),
            );
            jobs.register("pipeline.batch", made);
        }
        if let Some(slot) = ctx.server().service::<RingSlot>()
            && let Some(ring) = lock(&slot.0).take()
        {
            let frames = devices.acquisition.bus().subscribe_latest("preview ring");
            let clock = Arc::clone(&devices.clock);
            let settings = self.ring_settings();
            tokio::spawn(publish(preview.clone(), frames, clock, ring, settings));
        }
        // The live mosaic draws as the previews do (ADR-0026).
        let mosaics = ctx.server().service::<Mosaics>();
        if let Some(mosaics) = &mosaics {
            let (look, cpu_only) = (self.look.subscribe(), self.cpu_only.subscribe());
            let source: LookSource =
                Arc::new(move || (Look::from(&*look.borrow()), !*cpu_only.borrow()));
            mosaics.set_drawing(preview.clone(), source);
        }
        let _ = self.service.set(Service {
            devices,
            preview,
            engine,
            references,
            mosaics,
        });
        // The saved live recipe, checked again: a reference it reads may
        // have gone.
        if let Some(recipe) = self.live_recipe.get()
            && let Err(error) = self.install(Some(recipe)).await
        {
            tracing::warn!("the saved live recipe was dropped: {error:?}");
            self.live_recipe
                .set(None)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        Ok(())
    }

    /// The nodes live recipes may use.
    ///
    /// The stock nodes (P22), and the nodes of the extensions this start
    /// loaded (P25).
    #[property]
    async fn nodes(&self) -> Result<Vec<Descriptor>, PropertyError> {
        let service = self.service.get();
        let registry = service.map(|service| service.engine.registry());
        Ok(registry
            .map(|r| r.descriptors().cloned().collect())
            .unwrap_or_default())
    }

    /// The references recipes may read with `io.reference`, by name.
    #[property]
    async fn references(&self) -> Result<Vec<String>, PropertyError> {
        Ok(self
            .service
            .get()
            .map(|s| references(&s.references))
            .unwrap_or_default())
    }

    /// Sets the live recipe, run on each preview after demosaicing, binning
    /// and the black level, in place of the look; or none.
    ///
    /// It is checked first: every step runs live; it takes `image`, and
    /// perhaps `region`, and gives `image`, the same size, in display
    /// values; and it takes at most `recipe_budget_ms` on an image the
    /// default viewport's size. Refused (400), saying why, otherwise.
    #[action]
    async fn set_live_recipe(
        &self,
        #[param(default)] recipe: Option<Recipe>,
    ) -> Result<Option<RecipeCheck>, ActionError> {
        self.install(recipe).await
    }

    /// Captures a reference, such as a flat field or a dark frame: the
    /// mean of the camera's next `frames` frames (4 unless given, 64 at
    /// most), whole, in linear light, at most 1024 pixels wide.
    ///
    /// It is kept as `{data}/references/{name}.tiff`, for `io.reference`.
    /// The live recipe, if one is set, is made again, to read it.
    #[action]
    async fn capture_reference(
        &self,
        name: String,
        #[param(default)] frames: Option<u32>,
    ) -> Result<Reference, ActionError> {
        let service = self.started()?;
        let named = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if name.is_empty() || !name.chars().all(named) {
            return Err(refused(format!(
                "`{name}` isn't a name: letters, digits, `-` and `_`"
            )));
        }
        let count = frames.unwrap_or(4).clamp(1, 64);
        let mut feed = service
            .devices
            .acquisition
            .bus()
            .subscribe_latest("reference");
        let mut sum: Option<Image> = None;
        for _ in 0..count {
            let frame = tokio::time::timeout(Duration::from_secs(10), feed.recv()).await;
            let Ok(Some(frame)) = frame else {
                return Err(refused(
                    "the camera gave no frame for 10 s: is it streaming?",
                ));
            };
            let preview = service.preview.clone();
            let linear = tokio::task::spawn_blocking(move || preview.linear(&frame)).await;
            let linear = linear.map_err(|e| ActionError::handled(e.to_string()))?;
            let image = linear.map_err(refused)?;
            sum = Some(match sum {
                None => image,
                Some(mut sum) if sum.shape() == image.shape() => {
                    sum.data_mut()
                        .iter_mut()
                        .zip(image.data())
                        .for_each(|(s, v)| *s += v);
                    sum
                }
                Some(_) => return Err(refused("the frames changed size")),
            });
        }
        let mut mean = sum.ok_or_else(|| refused("no frames"))?;
        mean.data_mut().iter_mut().for_each(|v| *v /= count as f32);
        let folder = service.references.clone();
        let file = folder.join(format!("{name}.tiff"));
        let written = tokio::task::spawn_blocking(move || -> Result<Image, String> {
            let factor = mean.width().div_ceil(REFERENCE_WIDTH).max(1);
            let small = cpu::downsample(&mean, factor).map_err(|e| e.to_string())?;
            let rgb = match small.channels() {
                3 => small,
                _ => {
                    let rgb = small.data().iter().flat_map(|&v| [v; 3]).collect();
                    Image::from_vec(Shape::new(small.width(), small.height(), 3), rgb)
                        .map_err(|e| e.to_string())?
                }
            };
            let mut bytes = Vec::new();
            encode_tiff16(&mut bytes, &rgb).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
            Ok(rgb)
        });
        let written = written
            .await
            .map_err(|e| ActionError::handled(e.to_string()))?;
        let image =
            written.map_err(|e| ActionError::handled(format!("writing the reference: {e}")))?;
        if let Some(recipe) = self.live_recipe.get() {
            self.install(Some(recipe)).await?;
        }
        Ok(Reference {
            name,
            width: image.width(),
            height: image.height(),
            frames: count,
        })
    }

    /// The sessions open now, and how each is doing.
    #[property]
    async fn sessions(&self) -> Result<Vec<SessionStats>, PropertyError> {
        Ok(lock(&self.sessions).values().cloned().collect())
    }

    /// Whether there is a GPU to make previews on.
    #[property]
    async fn gpu(&self) -> Result<bool, PropertyError> {
        Ok(self.service.get().is_some_and(|s| s.preview.has_gpu()))
    }

    /// The camera's newest frames, as binary messages: a header, then a
    /// JPEG (ADR-0014). JSON text messages set the viewport, ROI, quality
    /// and rate, and acknowledge. A client that falls behind gets the
    /// newest frame, and while it keeps missing them, lower quality, then
    /// smaller frames.
    #[binary_stream]
    async fn stream(&self, mut subscriber: Subscriber<PreviewParams>) {
        let Some(service) = self.service.get() else {
            return;
        };
        let session = self.opened.fetch_add(1, Ordering::Relaxed) + 1;
        let bus = service.devices.acquisition.bus();
        let mut frames = bus.subscribe_latest(&format!("preview {session}"));
        let (mut adaptation, mut meter) = (Adaptation::default(), Meter::default());
        let (mut sequence, mut replaced, mut skipped, mut warned) = (0_u32, 0, 0, false);
        let (mut next, mut measured) = (Instant::now(), None::<Instant>);
        let mut seen = None;
        'session: loop {
            tokio::select! {
                () = tokio::time::sleep_until(next.into()) => {}
                () = subscriber.closed() => break,
            }
            let mut frame = tokio::select! {
                frame = frames.recv() => match frame {
                    Some(frame) => frame,
                    None => break,
                },
                changed = subscriber.changed() => match changed {
                    true => continue,
                    false => break,
                },
            };
            // How long since the camera finished the frame: acquisition, and
            // its wait on the bus.
            let age = service.devices.clock.now();
            timing::record(
                "stream.frame_age",
                age.saturating_since(frame.meta().host_time),
            );
            // Behind on acknowledging: wait, then take the newest frame,
            // counting those it replaced.
            let mut missed = false;
            let acknowledged = timing::start("stream.ack_wait");
            while subscriber.params().unshown(sequence) >= AHEAD {
                if !subscriber.changed().await {
                    break 'session;
                }
                if let Some(newer) = frames.try_recv() {
                    skipped += newer.meta().id.0.saturating_sub(frame.meta().id.0);
                    (frame, missed) = (newer, true);
                }
            }
            acknowledged.stop();
            let (request, max_fps) = self.request(&subscriber.params(), &adaptation);
            next = Instant::now() + Duration::from_secs_f64(1.0 / max_fps);
            let look = Look::from(&self.look.get());
            let (preview, gpu) = (service.preview.clone(), !self.cpu_only.get());
            let stats = subscriber
                .params()
                .stats
                .filter(|_| measured.is_none_or(|at| at.elapsed() >= STATS_EVERY));
            let queued = Instant::now();
            let made = tokio::task::spawn_blocking(move || {
                timing::record("stream.spawn", queued.elapsed());
                let mut made = preview.render(&frame, &request, &look, gpu);
                if let Err(PreviewError::Roi { .. }) = made {
                    let whole = PreviewRequest {
                        roi: None,
                        ..request
                    };
                    made = preview.render(&frame, &whole, &look, gpu);
                }
                let rendered = made?;
                let blocks = match stats {
                    Some(asked) => blocks(&preview, &frame, &asked, &rendered)?,
                    None => Vec::new(),
                };
                Ok::<_, PreviewError>((rendered, frame, request.quality, blocks))
            })
            .await;
            let (rendered, frame, quality, mut blocks) = match made {
                Ok(Ok(made)) => made,
                Ok(Err(error)) => {
                    if !std::mem::replace(&mut warned, true) {
                        tracing::warn!("preview session {session}: {error}");
                    }
                    continue;
                }
                Err(_) => break,
            };
            let measuring = !blocks.is_empty();
            if subscriber.params().mosaic == Some(true)
                && let Some(mosaics) = &service.mosaics
                && let Some(changed) = mosaics.changes(&mut seen)
            {
                blocks.push((TILES, changed));
            }
            let ready = service.devices.clock.now();
            let header = Header::of(&frame, &rendered, quality, sequence, ready);
            let latency = header.ready.saturating_since(header.exposure_end);
            timing::record("stream.latency", latency);
            let blocks: Vec<(u8, &[u8])> = blocks.iter().map(|(k, b)| (*k, b.as_slice())).collect();
            let message =
                timing::time("stream.message", || header.message(&rendered.jpeg, &blocks));
            let bytes = message.len();
            if !timing::time("stream.send", || subscriber.send(message)) {
                break;
            }
            // Timed from the sending, so that messages with statistics go at
            // least 100 ms apart, however long each took to make.
            if measuring {
                measured = Some(Instant::now());
            }
            sequence = sequence.wrapping_add(1);
            adaptation.record(missed || subscriber.dropped() > replaced);
            replaced = subscriber.dropped();
            let (fps, latency_ms, bytes_per_s) = meter.add(bytes, latency.as_secs_f64() * 1e3);
            let stats = SessionStats {
                session,
                fps,
                latency_ms,
                bytes_per_s,
                sent: subscriber.sent(),
                dropped: replaced + skipped,
                quality,
                width: rendered.width,
                height: rendered.height,
                gpu: rendered.gpu,
                recipe_ms: rendered.recipe.map(|took| took.as_secs_f64() * 1e3),
            };
            lock(&self.sessions).insert(session, stats);
        }
        lock(&self.sessions).remove(&session);
    }
}

/// The statistics `asked` for, of `frame` as `rendered` shows it, as blocks
/// (ADR-0035). An ROI that misses the frame is left out.
fn blocks(
    preview: &Preview,
    frame: &Frame,
    asked: &StatsRequest,
    rendered: &Rendered,
) -> Result<Vec<(u8, Vec<u8>)>, PreviewError> {
    let mut measured = preview.measure(frame, asked, rendered);
    if let Err(PreviewError::Roi { .. }) = measured {
        let whole = StatsRequest {
            roi: None,
            ..asked.clone()
        };
        measured = preview.measure(frame, &whole, rendered);
    }
    let (stats, mask) = measured?;
    let json = serde_json::to_vec(&stats).expect("statistics serialise to JSON");
    let mut blocks = vec![(STATISTICS, json)];
    blocks.extend(mask.map(|mask| (MASK, mask.to_bytes())));
    Ok(blocks)
}
