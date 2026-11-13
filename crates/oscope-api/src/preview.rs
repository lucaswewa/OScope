//! The `preview` Thing (plan §5.4, §5.5; ADR-0014): live previews of the
//! camera's frames, each the size of its client's viewport, as a binary
//! WebSocket at `/api/v1/preview/stream`; their defaults; and how each
//! session is doing.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use oscope_acquire::preview::wire::{FRAME, Header, JPEG, MASK, STATISTICS};
use oscope_acquire::preview::{Adaptation, Look, Preview, PreviewError, PreviewRequest, Rendered};
use oscope_acquire::stats::StatsRequest;
use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{Frame, PixelRect};
use oscope_imaging::color::{ColourCorrection, Curve, Display};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use teta_wot::http::Subscriber;
use teta_wot::prelude::*;

use crate::devices::Devices;

/// How many messages a client that acknowledges may have unshown.
const AHEAD: u32 = 2;
/// The least time between one client's statistics: 10 a second at most.
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
    service: OnceLock<Service>,
    sessions: Mutex<BTreeMap<u64, SessionStats>>,
    opened: AtomicU64,
}

impl PreviewThing {
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
        let _ = self.service.set(Service { devices, preview });
        Ok(())
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
            // Behind on acknowledging: wait, then take the newest frame,
            // counting those it replaced.
            let mut missed = false;
            while subscriber.params().unshown(sequence) >= AHEAD {
                if !subscriber.changed().await {
                    break 'session;
                }
                if let Some(newer) = frames.try_recv() {
                    skipped += newer.meta().id.0.saturating_sub(frame.meta().id.0);
                    (frame, missed) = (newer, true);
                }
            }
            let (request, max_fps) = self.request(&subscriber.params(), &adaptation);
            next = Instant::now() + Duration::from_secs_f64(1.0 / max_fps);
            let look = Look::from(&self.look.get());
            let (preview, gpu) = (service.preview.clone(), !self.cpu_only.get());
            let stats = subscriber
                .params()
                .stats
                .filter(|_| measured.is_none_or(|at| at.elapsed() >= STATS_EVERY));
            if stats.is_some() {
                measured = Some(Instant::now());
            }
            let made = tokio::task::spawn_blocking(move || {
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
            let (rendered, frame, quality, blocks) = match made {
                Ok(Ok(made)) => made,
                Ok(Err(error)) => {
                    if !std::mem::replace(&mut warned, true) {
                        tracing::warn!("preview session {session}: {error}");
                    }
                    continue;
                }
                Err(_) => break,
            };
            let header = header(&frame, &rendered, quality, sequence, &service.devices);
            let latency = header.ready.saturating_since(header.exposure_end);
            let blocks: Vec<(u8, &[u8])> = blocks.iter().map(|(k, b)| (*k, b.as_slice())).collect();
            let message = header.message(&rendered.jpeg, &blocks);
            let bytes = message.len();
            if !subscriber.send(message) {
                break;
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

/// The header of `frame`'s preview, ready now.
fn header(
    frame: &Frame,
    rendered: &Rendered,
    quality: u8,
    sequence: u32,
    devices: &Devices,
) -> Header {
    let meta = frame.meta();
    Header {
        kind: FRAME,
        codec: JPEG,
        quality,
        sequence,
        frame: meta.id,
        exposure_start: meta.exposure_start,
        exposure_end: meta.row_exposure(meta.height.saturating_sub(1)).1,
        ready: devices.clock.now(),
        roi: rendered.roi,
        width: rendered.width,
        height: rendered.height,
    }
}
