//! Deep zoom (plan §5.15, D33, ADR-0028): every image of the projects as
//! Deep Zoom tiles, made on demand from its pyramid and kept on the scratch
//! disk.
//!
//! - `GET /api/v1/zoom/{project}/{kind}/{id}/image.dzi`: the descriptor,
//!   `kind` being `acquisitions` or `results`.
//! - `GET /api/v1/zoom/{project}/{kind}/{id}/image_files/{level}/{col}_{row}.jpg`:
//!   a tile, as OpenSeadragon asks for them.
//! - `GET /api/v1/zoom/{project}/{kind}/{id}/status`: its size and levels,
//!   its look by default, a raw image's colour overview and coarse levels,
//!   the threads tiles are made on now, and the caches.
//! - **The look,** in each one's query: `plane` (0), and `black`, `white`,
//!   from 0 to 1 of the samples' range, and `gamma`. Unset, the image shows
//!   as its thumbnail does.
//! - **`Server-Timing`** on each tile: `tile;desc="hit"` or `"miss"`, and
//!   the server's time in ms.
//! - **The tile cache,** in `<scratch>/zoom`, its layout in
//!   [`oscope_store::zoom`]. A raw image's coarse tiles are kept only once
//!   its colour overview is built.
//! - **Tiles are made on a pool of their own,** of 8 threads at most, and
//!   1 while a hardware job holds the instrument, so that viewing never
//!   starves acquisition.
//! - **When a stitch ends,** its mosaic's colour overview, if raw, and its
//!   coarse levels are made at once, so that it opens at once.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use oscope_core::{BayerPattern, ProjectId};
use oscope_imaging::color::{Curve, Display, DisplayTable};
use oscope_imaging::zoom::{Dzi, Pyramid, ZoomError};
use oscope_jobs::{HardwareLock, JobService, JobState};
use oscope_store::RecordKind;
use oscope_store::zoom::{
    ChunkCache, StoredImage, StoredPyramid, TileCache, TileKey, build_overview, default_display,
    image_key, look_key, record_image,
};
use oscope_store::{Store, StoreError};
use oscope_workflows::stitch::STITCH;
use serde::Deserialize;
use serde_json::json;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::extract::{Path as UrlPath, Query, State};
use teta_wot::http::axum::http::{HeaderName, StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;

use crate::API_PREFIX;
use crate::data::problem;

/// The decompressed chunks kept in memory, in bytes.
const CHUNKS: usize = 512 << 20;
/// The tiles kept on the scratch disk, in bytes.
const TILES: u64 = 4 << 30;
/// The tile pool's threads, at most, and while a hardware job runs.
const THREADS: usize = 8;
const CAPPED: usize = 1;
/// The images kept open.
const OPEN: usize = 64;
/// Levels at most this wide and high are made when a stitch ends.
const COARSE: u64 = 1024;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An image being served.
struct Served {
    key: String,
    image: Arc<StoredImage>,
    pattern: Option<BayerPattern>,
    /// A raw image's colour overview, once built, and where it goes.
    overview: Mutex<Option<Arc<StoredImage>>>,
    overview_path: Option<PathBuf>,
    building: AtomicBool,
    /// Whether its coarse levels were made, in its default look.
    coarse: AtomicBool,
    display: Display,
}

impl Served {
    fn dzi(&self) -> Dzi {
        Dzi {
            width: self.image.info().width,
            height: self.image.info().height,
        }
    }

    fn pyramid(&self, chunks: &Arc<ChunkCache>) -> Result<StoredPyramid, StoreError> {
        let raw = self.pattern.map(|p| (p, lock(&self.overview).clone()));
        StoredPyramid::new(Arc::clone(&self.image), Arc::clone(chunks), raw)
    }

    /// Whether `level`'s tiles show as they will: not a raw image's halved
    /// ones before its colour overview.
    fn settled(&self, level: u32) -> bool {
        self.pattern.is_none() || self.dzi().halvings(level) == 0 || lock(&self.overview).is_some()
    }
}

/// Deep zoom's state: its caches, its pool and the images open.
pub(crate) struct Zoom {
    store: Arc<Store>,
    chunks: Arc<ChunkCache>,
    tiles: Option<TileCache>,
    images: Mutex<HashMap<String, Arc<Served>>>,
    /// The looks' tables, by their keys: made once each.
    tables: Mutex<HashMap<String, DisplayTable>>,
    full: rayon::ThreadPool,
    capped: rayon::ThreadPool,
    hardware: Option<HardwareLock>,
}

impl Zoom {
    /// Deep zoom of `store`'s images, its tiles kept in `cache` if it can
    /// be made there, and made with a thread while `jobs` runs a hardware
    /// job. With a runtime, it watches `jobs` for stitches that end.
    pub(crate) fn start(store: Arc<Store>, cache: PathBuf, jobs: Option<&JobService>) -> Arc<Self> {
        let pool = |threads: usize, name: &'static str| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(move |i| format!("{name}-{i}"))
                .build()
                .expect("a tile pool")
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let tiles = TileCache::open(&cache, TILES)
            .inspect_err(|e| tracing::warn!("deep zoom keeps no tiles on disk: {e}"))
            .ok();
        let zoom = Arc::new(Zoom {
            store,
            chunks: Arc::new(ChunkCache::new(CHUNKS)),
            tiles,
            images: Mutex::new(HashMap::new()),
            tables: Mutex::new(HashMap::new()),
            full: pool(THREADS.min(threads), "zoom"),
            capped: pool(CAPPED, "zoom-capped"),
            hardware: jobs.map(JobService::hardware),
        });
        if let (Some(jobs), Ok(runtime)) = (jobs, tokio::runtime::Handle::try_current()) {
            let mut events = jobs.subscribe();
            let watcher = Arc::downgrade(&zoom);
            runtime.spawn(async move {
                use tokio::sync::broadcast::error::RecvError;
                loop {
                    let info = match events.recv().await {
                        Ok(info) => info,
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                    };
                    if info.spec.kind != STITCH || info.state != JobState::Completed {
                        continue;
                    }
                    let Some(zoom) = watcher.upgrade() else {
                        break;
                    };
                    let made = info.results.first().and_then(|r| {
                        let mut parts = r.split('/').skip(1);
                        let project = parts.next()?.parse().ok()?;
                        Some((project, parts.nth(1)?.to_owned()))
                    });
                    if let Some((project, id)) = made {
                        let warmed = zoom.warm(project, RecordKind::Result, id).await;
                        if let Err(error) = warmed {
                            tracing::warn!("deep zoom couldn't warm a stitch: {error}");
                        }
                    }
                }
            });
        }
        zoom
    }

    /// `display`'s table, under its key `look`: made once.
    fn table(&self, look: &str, display: &Display) -> Result<DisplayTable, ZoomError> {
        if let Some(table) = lock(&self.tables).get(look) {
            return Ok(table.clone());
        }
        let table = DisplayTable::new(display)?;
        let mut tables = lock(&self.tables);
        if tables.len() >= OPEN {
            tables.clear();
        }
        tables.insert(look.to_owned(), table.clone());
        Ok(table)
    }

    /// The pool to make tiles on: one thread while a hardware job runs.
    fn pool(&self) -> &rayon::ThreadPool {
        match self.hardware.as_ref().is_some_and(|h| h.holder().is_some()) {
            true => &self.capped,
            false => &self.full,
        }
    }

    /// `kind` record `id`'s image of `project`, plane `plane`, opened once.
    async fn served(
        self: &Arc<Self>,
        project: ProjectId,
        kind: RecordKind,
        id: String,
        plane: u32,
    ) -> Result<Arc<Served>, StoreError> {
        let key = image_key(project, kind, &id, plane);
        if let Some(served) = lock(&self.images).get(&key) {
            return Ok(Arc::clone(served));
        }
        let zoom = Arc::clone(self);
        let opened = tokio::task::spawn_blocking(move || {
            let (path, pattern) = record_image(&zoom.store, project, kind, &id)?;
            let image = Arc::new(StoredImage::open(&path, plane)?);
            let display = default_display(&image)?;
            let overview_path = pattern
                .and(zoom.tiles.as_ref())
                .map(|t| t.root().join(&key).join("overview.ome.zarr"));
            let overview = overview_path
                .as_ref()
                .filter(|p| p.exists())
                .and_then(|p| StoredImage::open(p, 0).ok())
                .map(Arc::new);
            Ok(Arc::new(Served {
                key,
                image,
                pattern,
                overview: Mutex::new(overview),
                overview_path,
                building: AtomicBool::new(false),
                coarse: AtomicBool::new(false),
                display,
            }))
        });
        let served: Arc<Served> = opened
            .await
            .map_err(|e| StoreError::Zarr(e.to_string()))??;
        let mut images = lock(&self.images);
        if images.len() >= OPEN {
            images.clear();
        }
        Ok(Arc::clone(
            images.entry(served.key.clone()).or_insert(served),
        ))
    }

    /// Builds `served`'s colour overview, if it is raw and has none, on the
    /// capped pool: once at a time.
    fn build(self: &Arc<Self>, served: &Arc<Served>) {
        let (Some(pattern), Some(path)) = (served.pattern, served.overview_path.clone()) else {
            return;
        };
        if lock(&served.overview).is_some() || served.building.swap(true, Ordering::AcqRel) {
            return;
        }
        let (zoom, served) = (Arc::clone(self), Arc::clone(served));
        tokio::task::spawn_blocking(move || {
            let built = zoom
                .capped
                .install(|| build_overview(&served.image, pattern, &path))
                .and_then(|()| StoredImage::open(&path, 0));
            match built {
                Ok(overview) => *lock(&served.overview) = Some(Arc::new(overview)),
                Err(error) => tracing::warn!("deep zoom's colour overview: {error}"),
            }
            served.building.store(false, Ordering::Release);
        });
    }

    /// Makes `kind` record `id`'s colour overview, if it is raw, and its
    /// coarse levels' tiles in its default look, on the capped pool.
    async fn warm(
        self: &Arc<Self>,
        project: ProjectId,
        kind: RecordKind,
        id: String,
    ) -> Result<(), StoreError> {
        let served = self.served(project, kind, id, 0).await?;
        let zoom = Arc::clone(self);
        let work = tokio::task::spawn_blocking(move || {
            if let (Some(pattern), Some(path)) = (served.pattern, &served.overview_path)
                && lock(&served.overview).is_none()
            {
                zoom.capped
                    .install(|| build_overview(&served.image, pattern, path))?;
                *lock(&served.overview) = Some(Arc::new(StoredImage::open(path, 0)?));
            }
            let Some(tiles) = &zoom.tiles else {
                return Ok(());
            };
            let pyramid = served.pyramid(&zoom.chunks)?;
            let (dzi, look) = (served.dzi(), look_key(&served.display));
            let table = zoom
                .table(&look, &served.display)
                .map_err(|e| StoreError::Zarr(e.to_string()))?;
            for level in 0..dzi.levels() {
                let (w, h) = dzi.size(level);
                if w.max(h) > COARSE {
                    break;
                }
                let (cols, rows) = dzi.tiles(level);
                for tile in (0..rows).flat_map(|r| (0..cols).map(move |c| (c, r))) {
                    let key = TileKey {
                        image: served.key.clone(),
                        look: look.clone(),
                        level,
                        tile,
                    };
                    let made = zoom
                        .capped
                        .install(|| oscope_imaging::zoom::tile(&pyramid, level, tile, &table));
                    if let Some(jpeg) = made.map_err(|e| StoreError::Zarr(e.to_string()))? {
                        tiles.put(&key, &jpeg)?;
                    }
                }
            }
            served.coarse.store(true, Ordering::Release);
            Ok(())
        });
        work.await.map_err(|e| StoreError::Zarr(e.to_string()))?
    }
}

/// Deep zoom's routes, over `zoom`.
pub(crate) fn router(zoom: Arc<Zoom>) -> Router {
    let at = |rest: &str| format!("{API_PREFIX}/zoom/{{project}}/{{kind}}/{{id}}/{rest}");
    Router::new()
        .route(&at("image.dzi"), get(descriptor))
        .route(&at("status"), get(status))
        .route(&at("image_files/{level}/{tile}"), get(tile))
        .with_state(zoom)
}

/// A request's look, from its query.
#[derive(Debug, Default, Deserialize)]
struct Look {
    #[serde(default)]
    plane: u32,
    /// One of its channels alone, as grey: a stain's amount (W7.6).
    channel: Option<u32>,
    black: Option<f32>,
    white: Option<f32>,
    gamma: Option<f32>,
}

impl Look {
    /// The display it asks for, from `display` where it asks for nothing.
    fn display(&self, display: Display) -> Result<Display, String> {
        let black = self.black.unwrap_or(display.black);
        let white = self.white.unwrap_or(display.white);
        if !(black.is_finite() && white.is_finite() && black < white) {
            return Err(format!(
                "black {black} and white {white}: black below white"
            ));
        }
        let curve = match self.gamma {
            None => display.curve,
            Some(gamma) if gamma.is_finite() && gamma > 0.0 => Curve::Gamma { gamma },
            Some(gamma) => return Err(format!("a gamma of {gamma}: above 0")),
        };
        Ok(Display {
            black,
            white,
            curve,
        })
    }
}

/// One channel of a pyramid's, as grey (W7.6): a stain's amount, alone.
struct OneChannel<'a> {
    pyramid: &'a dyn Pyramid,
    channel: u32,
}

impl Pyramid for OneChannel<'_> {
    fn size(&self) -> (u64, u64) {
        self.pyramid.size()
    }

    fn channels(&self) -> u32 {
        1
    }

    fn coarsest(&self) -> u32 {
        self.pyramid.coarsest()
    }

    fn read(&self, halvings: u32, at: (u64, u64), size: (u64, u64)) -> Result<Vec<f32>, ZoomError> {
        let all = self.pyramid.read(halvings, at, size)?;
        let step = self.pyramid.channels().max(1) as usize;
        Ok(all
            .into_iter()
            .skip(self.channel as usize)
            .step_by(step)
            .collect())
    }
}

/// The record a route names: its project, kind and ID; or why there is
/// none.
fn record(project: &str, kind: &str, id: &str) -> Result<(ProjectId, RecordKind, String), String> {
    let project = project
        .parse()
        .map_err(|_| format!("no project {project}"))?;
    let kind = RecordKind::ALL
        .into_iter()
        .find(|k| k.folder() == kind && *k != RecordKind::Report)
        .ok_or_else(|| format!("no records are {kind}"))?;
    Ok((project, kind, id.to_owned()))
}

fn failed(error: &StoreError) -> Response {
    let status = match error {
        StoreError::NotFound { .. } => StatusCode::NOT_FOUND,
        StoreError::Refused(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    problem(status, error)
}

async fn descriptor(
    State(zoom): State<Arc<Zoom>>,
    UrlPath((project, kind, id)): UrlPath<(String, String, String)>,
    Query(look): Query<Look>,
) -> Response {
    let (project, kind, id) = match record(&project, &kind, &id) {
        Ok(record) => record,
        Err(why) => return problem(StatusCode::NOT_FOUND, why),
    };
    match zoom.served(project, kind, id, look.plane).await {
        Ok(served) => (
            [(header::CONTENT_TYPE, "application/xml")],
            served.dzi().descriptor(),
        )
            .into_response(),
        Err(error) => failed(&error),
    }
}

async fn status(
    State(zoom): State<Arc<Zoom>>,
    UrlPath((project, kind, id)): UrlPath<(String, String, String)>,
    Query(look): Query<Look>,
) -> Response {
    let (project, kind, id) = match record(&project, &kind, &id) {
        Ok(record) => record,
        Err(why) => return problem(StatusCode::NOT_FOUND, why),
    };
    let served = match zoom.served(project, kind, id, look.plane).await {
        Ok(served) => served,
        Err(error) => return failed(&error),
    };
    let dzi = served.dzi();
    let overview = match (served.pattern, lock(&served.overview).is_some()) {
        (None, _) => "none",
        (Some(_), true) => "ready",
        (Some(_), false) if served.building.load(Ordering::Acquire) => "building",
        (Some(_), false) => "not built",
    };
    let gamma = match served.display.curve {
        Curve::Gamma { gamma } => json!(gamma),
        Curve::Linear => json!(1.0),
        Curve::Srgb => json!(null),
    };
    let (hits, misses) = zoom.chunks.counts();
    let tiles = zoom
        .tiles
        .as_ref()
        .map(TileCache::counts)
        .unwrap_or_default();
    teta_wot::http::axum::Json(json!({
        "size": [dzi.width, dzi.height],
        "levels": dzi.levels(),
        "planes": served.image.info().planes,
        "raw": served.pattern.is_some(),
        "overview": overview,
        "coarse": served.coarse.load(Ordering::Acquire),
        "threads": zoom.pool().current_num_threads(),
        "look": { "black": served.display.black, "white": served.display.white, "gamma": gamma },
        "chunks": { "hits": hits, "misses": misses },
        "tiles": { "hits": tiles.hits, "misses": tiles.misses, "kept": tiles.tiles, "bytes": tiles.bytes },
    }))
    .into_response()
}

async fn tile(
    State(zoom): State<Arc<Zoom>>,
    UrlPath((project, kind, id, level, name)): UrlPath<(String, String, String, u32, String)>,
    Query(look): Query<Look>,
) -> Response {
    let started = Instant::now();
    let (project, kind, id) = match record(&project, &kind, &id) {
        Ok(record) => record,
        Err(why) => return problem(StatusCode::NOT_FOUND, why),
    };
    let tile = name
        .strip_suffix(".jpg")
        .and_then(|n| n.split_once('_'))
        .and_then(|(c, r)| Some((c.parse::<u64>().ok()?, r.parse::<u64>().ok()?)));
    let Some(tile) = tile else {
        return problem(StatusCode::NOT_FOUND, format!("no tile {name}"));
    };
    let served = match zoom.served(project, kind, id, look.plane).await {
        Ok(served) => served,
        Err(error) => return failed(&error),
    };
    if served.dzi().tile(level, tile).is_none() {
        return problem(
            StatusCode::NOT_FOUND,
            format!("no tile {name} at level {level}"),
        );
    }
    let display = match look.display(served.display) {
        Ok(display) => display,
        Err(why) => return problem(StatusCode::BAD_REQUEST, why),
    };
    let channels = match served.pattern {
        Some(_) => 3,
        None => served.image.info().channels,
    };
    let channel = look.channel;
    if let Some(c) = channel.filter(|&c| c >= channels) {
        let why = format!("no channel {c}: the image has {channels}");
        return problem(StatusCode::BAD_REQUEST, why);
    }
    let key = TileKey {
        image: served.key.clone(),
        look: match channel {
            Some(c) => format!("{};channel={c}", look_key(&display)),
            None => look_key(&display),
        },
        level,
        tile,
    };
    let settled = served.settled(level);
    if !settled {
        zoom.build(&served);
    }
    let made = tokio::task::spawn_blocking(move || {
        if let Some(jpeg) = zoom.tiles.as_ref().and_then(|t| t.get(&key)) {
            return Ok((jpeg, true));
        }
        let table = zoom.table(&key.look, &display)?;
        let rendered = zoom.pool().install(|| {
            let pyramid = served
                .pyramid(&zoom.chunks)
                .map_err(|e| ZoomError::Source(e.to_string()))?;
            match channel {
                Some(channel) => {
                    let one = OneChannel {
                        pyramid: &pyramid,
                        channel,
                    };
                    oscope_imaging::zoom::tile(&one, level, tile, &table)
                }
                None => oscope_imaging::zoom::tile(&pyramid, level, tile, &table),
            }
        })?;
        let jpeg = rendered.unwrap_or_default();
        if let (Some(tiles), true) = (&zoom.tiles, settled)
            && let Err(error) = tiles.put(&key, &jpeg)
        {
            tracing::warn!("deep zoom couldn't keep a tile: {error}");
        }
        Ok::<_, ZoomError>((jpeg, false))
    });
    match made.await {
        Ok(Ok((jpeg, hit))) => {
            let ms = started.elapsed().as_secs_f64() * 1e3;
            let timing = format!(
                "tile;desc=\"{}\";dur={ms:.2}",
                if hit { "hit" } else { "miss" }
            );
            (
                [
                    (header::CONTENT_TYPE, "image/jpeg".to_owned()),
                    (header::CACHE_CONTROL, "max-age=3600".to_owned()),
                    (HeaderName::from_static("server-timing"), timing),
                ],
                jpeg,
            )
                .into_response()
        }
        Ok(Err(error)) => problem(StatusCode::INTERNAL_SERVER_ERROR, error),
        Err(error) => problem(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}
