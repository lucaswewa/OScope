//! The `live_mosaic` Thing (ADR-0026; plan §5.15): a mosaic that builds as
//! frames arrive, from the live stream as the stage roams, or from scans.
//!
//! - **Its tiles** are JPEGs at `GET /api/v1/mosaic/tiles/{level}/{col}/{row}`,
//!   level 0 the finest.
//! - **The tiles changed** come with the previews of a client that asks:
//!   `mosaic` in its parameters (ADR-0026).
//! - **Its actions** are `begin`, `halt` and `clear`: a Thing's own `start`
//!   and `stop` are teta-wot's (ADR-0034).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use oscope_acquire::mosaic::{Gates, LiveMosaic, LookSource, MosaicCounts, MosaicSetup};
use oscope_acquire::preview::Preview;
use oscope_core::{Frame, Rect, Xyz};
use oscope_stitch::{Blend, CanvasGeometry, TILE, TileKey};
use oscope_workflows::calibration::{Values, geometry_key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::extract::{Path as UrlPath, State as Shared};
use teta_wot::http::axum::http::{StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;
use teta_wot::prelude::*;

use crate::API_PREFIX;
use crate::calibration::Calibrations;
use crate::data::problem;
use crate::devices::Devices;
use crate::server::Roots;

/// A canvas's memory unless asked otherwise, in MB.
const MEMORY_MB: u32 = 256;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The live mosaic being made, or last made, and how previews are drawn,
/// shared by the `live_mosaic` and `preview` Things, the scan engine and
/// the tile route.
#[derive(Default)]
pub(crate) struct Mosaics {
    current: Mutex<Option<(u64, Arc<LiveMosaic>)>>,
    made: AtomicU64,
    drawing: OnceLock<(Preview, LookSource)>,
}

impl std::fmt::Debug for Mosaics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let made = self.made.load(Ordering::Relaxed);
        f.debug_struct("Mosaics")
            .field("made", &made)
            .finish_non_exhaustive()
    }
}

impl Mosaics {
    /// The mosaic being made, or last made, with its number.
    pub(crate) fn current(&self) -> Option<(u64, Arc<LiveMosaic>)> {
        lock(&self.current).clone()
    }

    /// How the `preview` Thing draws, as it starts.
    pub(crate) fn set_drawing(&self, preview: Preview, look: LookSource) {
        let _ = self.drawing.set((preview, look));
    }

    /// Hands a scan's frame, taken at `at`, to the mosaic being made.
    pub(crate) fn scan_frame(&self, frame: &Frame, at: Xyz) {
        if let Some((_, mosaic)) = self.current() {
            mosaic.scan_frame(frame.clone(), at);
        }
    }

    /// The tiles changed since a viewer last saw `seen` (the mosaic's
    /// number and generation), as the preview stream's block: none if
    /// nothing changed.
    pub(crate) fn changes(&self, seen: &mut Option<(u64, u64)>) -> Option<Vec<u8>> {
        let (id, mosaic) = self.current()?;
        let changes = match *seen {
            Some((was, generation)) if was == id => mosaic.changes_since(generation),
            _ => mosaic.changes_since(u64::MAX),
        };
        *seen = Some((id, changes.generation));
        if !changes.all && changes.tiles.is_empty() {
            return None;
        }
        let tiles: Vec<[u32; 3]> = changes
            .tiles
            .iter()
            .map(|k| [k.level, k.col, k.row])
            .collect();
        let block = json!({
            "mosaic": id,
            "generation": changes.generation,
            "all": changes.all,
            "tiles": tiles,
        });
        Some(block.to_string().into_bytes())
    }
}

/// What a live mosaic is asked to be, each part with a default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MosaicRequest {
    /// The part of the stage it covers, in µm: the stage's travel if not
    /// given.
    pub region: Option<Rect>,
    /// Its pixel at level 0, in µm: twice the frames' if not given.
    pub pixel_um: Option<f64>,
    /// How frames are drawn over what is there: the newest wins.
    pub blend: Blend,
    /// Whether it draws the live stream's frames as the stage roams, or
    /// only scans': it roams, unless `false`.
    pub roam: Option<bool>,
    /// Free roaming's gates.
    pub gates: Gates,
    /// The most memory its tiles take, in MB: 256. The rest go to the
    /// scratch disk.
    pub memory_mb: Option<u32>,
}

/// How the live mosaic is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MosaicState {
    /// Its number, from 1 since the server started: 0 before any.
    pub mosaic: u64,
    /// Whether it is drawing.
    pub drawing: bool,
    /// Whether it draws the live stream's frames.
    pub roaming: bool,
    /// Its canvas, once one is made: its tiles are `tile` pixels square,
    /// in `levels` levels.
    pub canvas: Option<CanvasGeometry>,
    /// How many levels its canvas has.
    pub levels: u32,
    /// A tile's side, in pixels.
    pub tile: u32,
    /// How frames are drawn over what is there.
    pub blend: Blend,
    /// How many frames became what.
    pub counts: MosaicCounts,
    /// Its tiles, at every level.
    pub tiles: usize,
    /// The memory its tiles take, in MB.
    pub memory_mb: f64,
    /// Its generation: the preview stream tells a client of the tiles
    /// changed since the one it last saw.
    pub generation: u64,
}

fn state_of(current: Option<(u64, Arc<LiveMosaic>)>) -> MosaicState {
    let Some((id, mosaic)) = current else {
        return MosaicState {
            tile: TILE,
            ..MosaicState::default()
        };
    };
    let geometry = mosaic.geometry();
    let (resident, tiles) = mosaic.tiles();
    MosaicState {
        mosaic: id,
        drawing: !mosaic.stopped(),
        roaming: mosaic.roaming() && !mosaic.stopped(),
        canvas: Some(geometry),
        levels: geometry.levels(),
        tile: TILE,
        blend: mosaic.blend(),
        counts: mosaic.counts(),
        tiles,
        memory_mb: (resident * TILE as usize * TILE as usize * 4) as f64 / 1e6,
        generation: mosaic.changes_since(u64::MAX).generation,
    }
}

struct State {
    devices: Arc<Devices>,
    roots: Arc<Roots>,
    calibrations: Arc<Calibrations>,
    mosaics: Arc<Mosaics>,
}

/// The live mosaic: a mosaic that builds as frames arrive (ADR-0026).
///
/// From the live stream as the stage roams, gated by motion blur, place
/// and sharpness, and from the scans' tiles as they are taken. Its tiles
/// are at `/api/v1/mosaic/tiles/{level}/{col}/{row}`.
#[derive(Thing)]
#[thing(
    title = "Live mosaic",
    semantic_type = "saref:Function",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct LiveMosaicThing {
    state: OnceLock<State>,
}

/// A request that can't be (ADR-0034).
fn refused(why: impl std::fmt::Display) -> ActionError {
    ActionError::handled(format!("refused (400): {why}"))
}

impl LiveMosaicThing {
    fn ready(&self) -> Result<&State, ActionError> {
        self.state
            .get()
            .ok_or_else(|| ActionError::handled("unavailable (503): the server has no devices"))
    }

    /// The setup `request` asks for, from the camera, the objective fitted
    /// and its calibration, and the stage.
    async fn setup(&self, request: &MosaicRequest) -> Result<MosaicSetup, ActionError> {
        let state = self.ready()?;
        let devices = &state.devices;
        let device = crate::devices::action_error;
        let settings = devices.camera.settings().await.map_err(device)?;
        let capabilities = devices.camera.capabilities();
        let binning = capabilities
            .mode(&settings.mode)
            .map_err(device)?
            .binning
            .max(1);
        let objective = devices.objectives.fitted().await.map_err(device)?;
        let objective =
            objective.ok_or_else(|| refused("no objective is fitted: fit one first"))?;
        let calibrations = state.calibrations.open(&state.roots).map_err(refused)?;
        let active = calibrations.active(&geometry_key(&objective.name, binning));
        let matrix = match active.map(|set| set.values) {
            Some(Values::Geometry(g)) => g.matrix,
            _ => {
                let catalogue = devices.objectives.catalogue();
                let pixel = catalogue.pixel_size(&objective, capabilities.pixel_size).0;
                let pixel = pixel * f64::from(binning);
                [[pixel, 0.0], [0.0, pixel]]
            }
        };
        let [[a, b], [c, d]] = matrix;
        let pixel_um = request
            .pixel_um
            .unwrap_or(2.0 * (a * d - b * c).abs().sqrt());
        let travel = devices.stage.capabilities();
        let region = request.region.unwrap_or(Rect::new(
            travel.x.min.0,
            travel.y.min.0,
            travel.x.max.0 - travel.x.min.0,
            travel.y.max.0 - travel.y.min.0,
        ));
        let corner = (region.origin.x, region.origin.y);
        let size = (region.size.width, region.size.height);
        let geometry = CanvasGeometry::covering(corner, size, pixel_um).map_err(refused)?;
        let memory = request.memory_mb.unwrap_or(MEMORY_MB) as usize;
        Ok(MosaicSetup {
            geometry,
            blend: request.blend,
            objective: Some(objective.name),
            binning,
            matrix,
            sensor: (capabilities.sensor_width, capabilities.sensor_height),
            gates: request.gates,
            budget: memory << 20,
            folder: state.roots.scratch.join("mosaic"),
        })
    }
}

#[thing_impl]
impl LiveMosaicThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        let server = ctx.server();
        let found = (
            server.service::<Devices>(),
            server.service::<Roots>(),
            server.service::<Calibrations>(),
            server.service::<Mosaics>(),
        );
        if let (Some(devices), Some(roots), Some(calibrations), Some(mosaics)) = found {
            let state = State {
                devices,
                roots,
                calibrations,
                mosaics,
            };
            let _ = self.state.set(state);
        }
        Ok(())
    }

    /// How the live mosaic is.
    ///
    /// Its canvas, whether it is drawing, how many frames became what, and
    /// its tiles.
    #[property]
    async fn state(&self) -> Result<MosaicState, PropertyError> {
        let current = self.state.get().and_then(|s| s.mosaics.current());
        Ok(state_of(current))
    }

    /// Begins a new live mosaic.
    ///
    /// On a new canvas, as `request` asks, through the objective fitted and
    /// the camera's binning, by their calibration or the nominal pixel. The
    /// mosaic before stops, and its tiles go.
    #[action]
    async fn begin(
        &self,
        #[param(default)] request: Option<MosaicRequest>,
    ) -> Result<MosaicState, ActionError> {
        let request = request.unwrap_or_default();
        let setup = self.setup(&request).await?;
        let state = self.ready()?;
        let drawing = state.mosaics.drawing.get().cloned();
        let unavailable = || ActionError::handled("unavailable (503): the preview hasn't started");
        let (preview, look) = drawing.ok_or_else(unavailable)?;
        let devices = &state.devices;
        let roam = request.roam.unwrap_or(true).then(|| {
            let frames = devices.acquisition.bus().subscribe_latest("live mosaic");
            (frames, devices.stage.track())
        });
        let mut current = lock(&state.mosaics.current);
        if let Some((_, before)) = current.take() {
            before.stop();
        }
        let mosaic = LiveMosaic::start(setup, preview, look, roam).map_err(refused)?;
        let id = state.mosaics.made.fetch_add(1, Ordering::Relaxed) + 1;
        *current = Some((id, mosaic));
        Ok(state_of(current.clone()))
    }

    /// Halts the live mosaic.
    ///
    /// It draws no more frames; its tiles stay to be seen.
    #[action]
    async fn halt(&self) -> Result<MosaicState, ActionError> {
        let current = self.ready()?.mosaics.current();
        if let Some((_, mosaic)) = &current {
            mosaic.stop();
        }
        Ok(state_of(current))
    }

    /// Clears the live mosaic.
    ///
    /// Its canvas is emptied; it goes on drawing if it was.
    #[action]
    async fn clear(&self) -> Result<MosaicState, ActionError> {
        let current = self.ready()?.mosaics.current();
        if let Some((_, mosaic)) = &current {
            mosaic
                .clear()
                .map_err(|e| ActionError::handled(format!("error (500): {e}")))?;
        }
        Ok(state_of(current))
    }
}

/// The tile route, over `mosaics`.
pub(crate) fn router(mosaics: Arc<Mosaics>) -> Router {
    Router::new()
        .route(
            &format!("{API_PREFIX}/mosaic/tiles/{{level}}/{{col}}/{{row}}"),
            get(tile),
        )
        .with_state(mosaics)
}

/// A tile of the live mosaic as a JPEG, or 404 where nothing is drawn.
async fn tile(
    Shared(mosaics): Shared<Arc<Mosaics>>,
    UrlPath((level, col, row)): UrlPath<(u32, u32, u32)>,
) -> Response {
    let Some((_, mosaic)) = mosaics.current() else {
        return problem(StatusCode::NOT_FOUND, "there is no live mosaic yet");
    };
    let key = TileKey { level, col, row };
    match tokio::task::spawn_blocking(move || mosaic.tile_jpeg(key)).await {
        Ok(Ok(Some(jpeg))) => {
            let headers = [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store"),
            ];
            (headers, jpeg).into_response()
        }
        Ok(Ok(None)) => problem(
            StatusCode::NOT_FOUND,
            format!("nothing is drawn on {key:?}"),
        ),
        Ok(Err(error)) => problem(StatusCode::INTERNAL_SERVER_ERROR, error),
        Err(error) => problem(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}
