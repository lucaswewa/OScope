//! Volumes on the media plane (plan §5.16, D8, ADR-0029): a stored image's
//! planes as a volume, for VTK.js.
//!
//! - `GET /api/v1/volume/{project}/{kind}/{id}/meta`: its size, voxel,
//!   samples and levels, `kind` being `acquisitions` or `results`.
//! - `GET …/slice?axis=z&index=N&level=L&format=u8`: an orthogonal slice,
//!   across z, y or x, at a level, its index in that level's voxels.
//! - `GET …/brick?x=&y=&z=&w=&h=&d=&level=&max=&format=`: a box, given in
//!   full-size voxels (the whole volume by default), at `level`, or at the
//!   finest whose part holds `max` voxels or fewer (512³).
//! - **Slices and bricks** are a header of [`HEADER`] bytes, then the
//!   voxels: x fastest, then y, then z, a voxel's channels together, as
//!   `uint8` or `uint16`, little-endian.
//! - **A raw image,** a camera's Bayer mosaic, starts at level 1, each
//!   voxel its cell's mean: level 0 holds the mosaic's pattern.
//!
//! The header, little-endian:
//!
//! | Bytes | What |
//! |---|---|
//! | 0–3 | `OSV1` |
//! | 4–5 | the header's length, 64 |
//! | 6 | the samples: 1 for `uint8`, 2 for `uint16` |
//! | 7 | the channels |
//! | 8–11 | the level |
//! | 12–23 | the size in voxels: x, y, z, as `u32` |
//! | 24–35 | the origin in the level's voxels: x, y, z, as `u32` |
//! | 36–47 | the voxel's size in µm: x, y, z, as `f32`, 0 if not known |
//! | 48–63 | 0 |

use std::sync::Arc;

use oscope_core::ProjectId;
use oscope_store::zoom::record_image;
use oscope_store::{ImageInfo, ImageReader, RecordKind, SampleType, Samples, Store, StoreError};
use serde::Deserialize;
use serde_json::json;
use teta_wot::http::axum::Router;
use teta_wot::http::axum::extract::{Path as UrlPath, Query, State};
use teta_wot::http::axum::http::{StatusCode, header};
use teta_wot::http::axum::response::{IntoResponse, Response};
use teta_wot::http::axum::routing::get;

use crate::API_PREFIX;
use crate::data::problem;

/// A slice's or a brick's header, in bytes.
pub const HEADER: usize = 64;
/// A brick's voxels at most, by default: 512³.
const MOST: u64 = 512 * 512 * 512;

/// The volume routes, over `store`.
pub(crate) fn router(store: Arc<Store>) -> Router {
    let at = |rest: &str| format!("{API_PREFIX}/volume/{{project}}/{{kind}}/{{id}}/{rest}");
    Router::new()
        .route(&at("meta"), get(meta))
        .route(&at("slice"), get(slice))
        .route(&at("brick"), get(brick))
        .with_state(store)
}

/// A volume opened: its reader, and whether it is a raw mosaic.
struct Opened {
    reader: ImageReader,
    raw: bool,
}

impl Opened {
    fn info(&self) -> &ImageInfo {
        self.reader.info()
    }

    /// Its finest level served: 1 for a raw mosaic, if it has one.
    fn finest(&self) -> u32 {
        u32::from(self.raw).min(self.info().levels - 1)
    }

    /// Level `level`'s size in voxels.
    fn size(&self, level: u32) -> (u64, u64, u64) {
        let (w, h) = self.info().level_size(level);
        (w, h, u64::from(self.info().level_planes(level)))
    }

    /// Level `level`'s voxel in µm, if known.
    fn voxel(&self, level: u32) -> Option<[f64; 3]> {
        let size = self.info().pixel_size?;
        let f = f64::from(1_u32 << level);
        let fz = f64::from(1_u32 << self.info().z_halvings(level));
        Some([size.x * f, size.y * f, size.z * fz])
    }

    /// The box at `origin`, `size` voxels of `level`, as `format`'s samples.
    fn read(
        &self,
        level: u32,
        origin: (u64, u64, u64),
        size: (u64, u64, u64),
        format: Format,
    ) -> Result<Vec<u8>, StoreError> {
        let (x, y, z) = origin;
        let (w, h, d) = size;
        let c = self.info().channels as usize;
        let mut out = Vec::with_capacity((w * h * d) as usize * c * format.bytes());
        for plane in z..z + d {
            let samples = self.reader.read(level, plane as u32, (x, y), (w, h))?;
            format.push(&samples, &mut out);
        }
        Ok(out)
    }
}

/// What a slice's or brick's samples are.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Format {
    /// 8 bits, the samples scaled.
    #[default]
    U8,
    /// 16 bits, the samples scaled.
    U16,
}

impl Format {
    fn bytes(self) -> usize {
        match self {
            Format::U8 => 1,
            Format::U16 => 2,
        }
    }

    /// `samples` in this format, appended to `out`, little-endian.
    fn push(self, samples: &Samples, out: &mut Vec<u8>) {
        match (self, samples) {
            (Format::U8, Samples::U8(v)) => out.extend_from_slice(v),
            (Format::U8, Samples::U16(v)) => {
                out.extend(
                    v.iter()
                        .map(|&s| ((u32::from(s) * 255 + 32_767) / 65_535) as u8),
                );
            }
            (Format::U16, Samples::U16(v)) => out.extend(v.iter().flat_map(|s| s.to_le_bytes())),
            (Format::U16, Samples::U8(v)) => {
                out.extend(v.iter().flat_map(|&s| (u16::from(s) * 257).to_le_bytes()));
            }
            (Format::U8, Samples::F32(v)) => {
                out.extend(v.iter().map(|s| (s.clamp(0.0, 1.0) * 255.0).round() as u8));
            }
            (Format::U16, Samples::F32(v)) => out.extend(
                v.iter()
                    .flat_map(|s| ((s.clamp(0.0, 1.0) * 65_535.0).round() as u16).to_le_bytes()),
            ),
        }
    }
}

/// The header of `size` voxels at `origin` of `level` of `opened`, in
/// `format`.
fn header(
    opened: &Opened,
    level: u32,
    origin: (u64, u64, u64),
    size: (u64, u64, u64),
    format: Format,
) -> Vec<u8> {
    let mut head = Vec::with_capacity(HEADER);
    head.extend_from_slice(b"OSV1");
    head.extend_from_slice(&(HEADER as u16).to_le_bytes());
    head.push(format.bytes() as u8);
    head.push(opened.info().channels as u8);
    head.extend_from_slice(&level.to_le_bytes());
    for v in [size.0, size.1, size.2, origin.0, origin.1, origin.2] {
        head.extend_from_slice(&(v as u32).to_le_bytes());
    }
    let voxel = opened.voxel(level).unwrap_or([0.0; 3]);
    for v in voxel {
        head.extend_from_slice(&(v as f32).to_le_bytes());
    }
    head.resize(HEADER, 0);
    head
}

fn failed(error: &StoreError) -> Response {
    let status = match error {
        StoreError::NotFound { .. } => StatusCode::NOT_FOUND,
        StoreError::Refused(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    problem(status, error)
}

/// The record a route names, opened, on a blocking thread.
async fn open(
    store: Arc<Store>,
    (project, kind, id): (String, String, String),
) -> Result<Opened, Box<Response>> {
    let project: ProjectId = project.parse().map_err(|_| {
        Box::new(problem(
            StatusCode::NOT_FOUND,
            format!("no project {project}"),
        ))
    })?;
    let kind = RecordKind::ALL
        .into_iter()
        .find(|k| k.folder() == kind && *k != RecordKind::Report)
        .ok_or_else(|| {
            Box::new(problem(
                StatusCode::NOT_FOUND,
                format!("no records are {kind}"),
            ))
        })?;
    let opened = tokio::task::spawn_blocking(move || {
        let (path, _) = record_image(&store, project, kind, &id)?;
        let reader = ImageReader::open(&path)?;
        let raw = reader.info().channel_names == ["raw"] && reader.info().channels == 1;
        Ok::<_, StoreError>(Opened { reader, raw })
    });
    match opened.await {
        Ok(Ok(opened)) => Ok(opened),
        Ok(Err(error)) => Err(Box::new(failed(&error))),
        Err(error) => Err(Box::new(problem(StatusCode::INTERNAL_SERVER_ERROR, error))),
    }
}

async fn meta(
    State(store): State<Arc<Store>>,
    UrlPath(record): UrlPath<(String, String, String)>,
) -> Response {
    let opened = match open(store, record).await {
        Ok(opened) => opened,
        Err(response) => return *response,
    };
    let info = opened.info();
    let levels: Vec<_> = (opened.finest()..info.levels)
        .map(|level| {
            let (w, h, d) = opened.size(level);
            json!({"level": level, "size": [w, h, d], "voxel_um": opened.voxel(level)})
        })
        .collect();
    let sample = match info.sample {
        SampleType::U8 => "u8",
        SampleType::U16 => "u16",
        SampleType::F32 => "f32",
    };
    teta_wot::http::axum::Json(json!({
        "size": [info.width, info.height, info.planes],
        "channels": info.channels,
        "sample": sample,
        "voxel_um": opened.voxel(0),
        "volume": info.volume,
        "raw": opened.raw,
        "levels": levels,
        "header": HEADER,
    }))
    .into_response()
}

/// A slice's query.
#[derive(Debug, Deserialize)]
struct SliceQuery {
    axis: char,
    index: u64,
    level: Option<u32>,
    #[serde(default)]
    format: Format,
}

async fn slice(
    State(store): State<Arc<Store>>,
    UrlPath(record): UrlPath<(String, String, String)>,
    Query(query): Query<SliceQuery>,
) -> Response {
    let opened = match open(store, record).await {
        Ok(opened) => opened,
        Err(response) => return *response,
    };
    let level = query.level.unwrap_or(opened.finest());
    if level < opened.finest() || level >= opened.info().levels {
        return problem(
            StatusCode::BAD_REQUEST,
            format!("no level {level} to slice"),
        );
    }
    let (w, h, d) = opened.size(level);
    let i = query.index;
    let (origin, size) = match query.axis {
        'z' if i < d => ((0, 0, i), (w, h, 1)),
        'y' if i < h => ((0, i, 0), (w, 1, d)),
        'x' if i < w => ((i, 0, 0), (1, h, d)),
        'x' | 'y' | 'z' => {
            return problem(
                StatusCode::BAD_REQUEST,
                format!("no index {i} across {}: {w} × {h} × {d}", query.axis),
            );
        }
        axis => {
            return problem(
                StatusCode::BAD_REQUEST,
                format!("no axis {axis}: x, y or z"),
            );
        }
    };
    answer(opened, level, origin, size, query.format).await
}

/// A brick's query: its box in full-size voxels, and its level.
#[derive(Debug, Deserialize)]
struct BrickQuery {
    x: Option<u64>,
    y: Option<u64>,
    z: Option<u64>,
    w: Option<u64>,
    h: Option<u64>,
    d: Option<u64>,
    level: Option<u32>,
    max: Option<u64>,
    #[serde(default)]
    format: Format,
}

async fn brick(
    State(store): State<Arc<Store>>,
    UrlPath(record): UrlPath<(String, String, String)>,
    Query(query): Query<BrickQuery>,
) -> Response {
    let opened = match open(store, record).await {
        Ok(opened) => opened,
        Err(response) => return *response,
    };
    let info = opened.info();
    let (x, y, z) = (
        query.x.unwrap_or(0),
        query.y.unwrap_or(0),
        query.z.unwrap_or(0),
    );
    let planes = u64::from(info.planes);
    let w = query.w.unwrap_or(info.width.saturating_sub(x));
    let h = query.h.unwrap_or(info.height.saturating_sub(y));
    let d = query.d.unwrap_or(planes.saturating_sub(z));
    let inside = w > 0 && h > 0 && d > 0;
    if !inside || x + w > info.width || y + h > info.height || z + d > planes {
        return problem(
            StatusCode::BAD_REQUEST,
            format!(
                "a box of {w} × {h} × {d} at ({x}, {y}, {z}): inside {} × {} × {planes}, and not empty",
                info.width, info.height
            ),
        );
    }
    // The box at each level: its first voxel's, to past its last's.
    let part = |level: u32| {
        let (f, fz) = (1_u64 << level, 1_u64 << info.z_halvings(level));
        let origin = (x / f, y / f, z / fz);
        let end = (
            (x + w).div_ceil(f),
            (y + h).div_ceil(f),
            (z + d).div_ceil(fz),
        );
        (
            origin,
            (end.0 - origin.0, end.1 - origin.1, end.2 - origin.2),
        )
    };
    let most = query.max.unwrap_or(MOST).max(1);
    let level = match query.level {
        Some(level) if level < opened.finest() || level >= info.levels => {
            return problem(StatusCode::BAD_REQUEST, format!("no level {level}"));
        }
        Some(level) => level,
        None => (opened.finest()..info.levels)
            .find(|&level| {
                let (_, (w, h, d)) = part(level);
                w * h * d <= most
            })
            .unwrap_or(info.levels - 1),
    };
    let (origin, size) = part(level);
    answer(opened, level, origin, size, query.format).await
}

/// A slice's or brick's answer: its header, then its voxels.
async fn answer(
    opened: Opened,
    level: u32,
    origin: (u64, u64, u64),
    size: (u64, u64, u64),
    format: Format,
) -> Response {
    let read = tokio::task::spawn_blocking(move || {
        let mut body = header(&opened, level, origin, size, format);
        body.extend(opened.read(level, origin, size, format)?);
        Ok::<_, StoreError>(body)
    });
    match read.await {
        Ok(Ok(body)) => {
            ([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response()
        }
        Ok(Err(error)) => failed(&error),
        Err(error) => problem(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}
