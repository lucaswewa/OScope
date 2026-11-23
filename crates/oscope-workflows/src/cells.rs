//! Cell counting as a job (P46): a stored image's nuclei found tile by
//! tile, each tile read with a halo and its objects kept where their
//! centroids lie in its core, so that none is counted twice or left out.
//! It makes a table, their outlines and a summary, and an annotation layer
//! on the image. P46's notes give the behaviour.

use std::fmt::Write as _;
use std::sync::Arc;

use oscope_core::ProjectId;
use oscope_imaging::cells::{Filters, Threshold, measure, otsu, outline, segment};
use oscope_imaging::stain::{DabCounts, DabThresholds, Stains, TISSUE, dab_class, optical_density};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::{
    Feature, Geometry, Input, NodeVersion, Provenance, RecordKind, ResultRecord, Store, StoreError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::stain::Colour;

/// The job's kind.
pub const COUNT: &str = "cells.count";

/// A tile's core's side, in pixels, and the halo read about it.
const CORE: u64 = 256;
const HALO: u64 = 32;

/// The channel segmented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Hematoxylin's amount, by colour deconvolution: brightfield.
    #[default]
    Hematoxylin,
    /// The channels' mean: fluorescence.
    Grey,
}

/// Counting's options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CountOptions {
    /// The channel segmented: hematoxylin unless given.
    pub channel: Channel,
    /// The threshold: Otsu's level over the image unless given.
    pub threshold: Threshold,
    /// No pixel within this many of a watershed's seed is higher: 2.
    pub seed_radius: usize,
    /// A seed's least distance from the background, in pixels: 1.5.
    pub min_distance: f32,
    /// The least area kept, in µm²: 10.
    pub min_area: f64,
    /// The greatest area kept, in µm²: 200.
    pub max_area: f64,
    /// The least circularity kept: 0.4.
    pub min_circularity: f64,
    /// The light through glass, from 0 to 1 of full scale: each channel's
    /// 99.5th percentile unless given.
    pub white: Option<[f32; 3]>,
    /// Whether to class each nucleus by its mean DAB, for an H-DAB slide.
    pub dab: bool,
    /// DAB's thresholds (P45).
    pub thresholds: DabThresholds,
}

impl Default for CountOptions {
    fn default() -> Self {
        let filters = Filters::default();
        Self {
            channel: Channel::Hematoxylin,
            threshold: Threshold::Otsu,
            seed_radius: 2,
            min_distance: 1.5,
            min_area: filters.min_area,
            max_area: filters.max_area,
            min_circularity: filters.min_circularity,
            white: None,
            dab: false,
            thresholds: DabThresholds::default(),
        }
    }
}

/// What a count found: its `summary.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CountSummary {
    /// The objects counted.
    pub count: u64,
    /// The tissue's area, in mm²: of summed optical density 0.05 or more,
    /// or the whole image's, in grey.
    pub tissue_mm2: f64,
    /// The objects per mm² of it.
    pub per_mm2: f64,
    /// Their mean area, in µm².
    pub mean_area: f64,
    /// Their mean circularity.
    pub mean_circularity: f64,
    /// The level thresholded at, if global.
    pub level: Option<f32>,
    /// The light through glass.
    pub white: [f32; 3],
    /// The nuclei by their mean DAB's class, if asked.
    pub dab: Option<DabCounts>,
}

/// What a count made: its result, the summary, and the outlines kept as
/// the image's layer `cells`.
#[derive(Clone, Debug)]
pub struct Counted {
    /// The result: `cells.csv`, `cells.geojson` and `summary.json`.
    pub result: ResultRecord,
    /// The summary.
    pub summary: CountSummary,
    /// The outlines in the annotation layer.
    pub annotated: usize,
}

/// A nucleus found, in µm from the image's top-left corner.
struct Found {
    centroid: [f64; 2],
    area: f64,
    perimeter: f64,
    circularity: f64,
    mean: f64,
    dab: Option<(f64, u8)>,
    ring: Vec<[f64; 2]>,
}

/// The channel's values of `rgb` against `white`; DAB's amounts, by H-DAB,
/// if `dab`; and which pixels are tissue.
fn channel(
    rgb: &[f32],
    white: [f32; 3],
    options: &CountOptions,
) -> (Vec<f32>, Vec<f32>, Vec<bool>) {
    let (pixels, _) = rgb.as_chunks::<3>();
    let tissue: Vec<bool> = pixels
        .iter()
        .map(|p| (0..3).map(|c| optical_density(p[c], white[c])).sum::<f32>() >= TISSUE)
        .collect();
    let values = match options.channel {
        Channel::Grey => pixels.iter().map(|p| p.iter().sum::<f32>() / 3.0).collect(),
        Channel::Hematoxylin => {
            let amounts = Stains::he().deconvolve(rgb, white);
            amounts.iter().step_by(3).copied().collect()
        }
    };
    let dab = match options.dab {
        true => Stains::hdab()
            .deconvolve(rgb, white)
            .iter()
            .skip(1)
            .step_by(3)
            .copied()
            .collect(),
        false => Vec::new(),
    };
    (values, dab, tissue)
}

/// Counts the nuclei of `source`'s image of `project`, as `options` say:
/// a result of the project, and the outlines kept as the image's layer
/// `cells`. `progress` is told the share done and what, and stops it by
/// answering false.
pub fn count(
    store: &Store,
    project: ProjectId,
    source: Input,
    options: &CountOptions,
    progress: &dyn Fn(f64, String) -> bool,
) -> Result<Counted, StoreError> {
    let refused = |why: String| StoreError::Refused(why);
    let (kind, id) = match source {
        Input::Acquisition(a) => (RecordKind::Acquisition, a.to_string()),
        Input::Result(r) => (RecordKind::Result, r.to_string()),
    };
    let (colour, info) = Colour::open(store, project, kind, &id)?;
    if colour.grey && (options.channel == Channel::Hematoxylin || options.dab) {
        return Err(refused(format!(
            "{} {id} is grey: hematoxylin and DAB are found in colour",
            kind.name()
        )));
    }
    let cell = colour.cell as f64;
    let pixel = info
        .pixel_size
        .map_or([1.0, 1.0], |s| [s.x * cell, s.y * cell]);
    // A first pass, every 8th pixel each way: the white, and Otsu's level.
    let mut white = options.white.unwrap_or([1.0; 3]);
    let otsu_wanted = options.threshold == Threshold::Otsu;
    let tiles = colour.tiles();
    let mut sample = Vec::new();
    if (options.white.is_none() && !colour.grey) || otsu_wanted {
        for (n, &tile) in tiles.iter().enumerate() {
            let rgb = colour.rgb(tile)?;
            let w = tile.2 as usize;
            for j in (0..tile.3 as usize).step_by(8) {
                for i in (0..w).step_by(8) {
                    sample.extend_from_slice(&rgb[(j * w + i) * 3..(j * w + i) * 3 + 3]);
                }
            }
            if !progress(
                0.2 * (n + 1) as f64 / tiles.len() as f64,
                "sampled".to_owned(),
            ) {
                return Err(StoreError::Cancelled);
            }
        }
    }
    if options.white.is_none() && !colour.grey {
        white = [0, 1, 2].map(|c| {
            let mut v: Vec<f32> = sample.iter().skip(c).step_by(3).copied().collect();
            v.sort_by(f32::total_cmp);
            v.get(v.len().saturating_sub(1) * 995 / 1000)
                .copied()
                .unwrap_or(1.0)
        });
    }
    let (threshold, level) = match options.threshold {
        Threshold::Otsu => {
            let level = otsu(&channel(&sample, white, options).0, 256);
            (Threshold::Level(level), Some(level))
        }
        Threshold::Level(l) => (Threshold::Level(l), Some(l)),
        other => (other, None),
    };
    // The second pass: each core with its halo, its objects kept by their
    // centroids.
    let filters = Filters {
        min_area: options.min_area,
        max_area: options.max_area,
        min_circularity: options.min_circularity,
    };
    let (w, h) = colour.size;
    let cores: Vec<(u64, u64)> = (0..h)
        .step_by(CORE as usize)
        .flat_map(|y| (0..w).step_by(CORE as usize).map(move |x| (x, y)))
        .collect();
    let (mut found, mut tissue) = (Vec::new(), 0_u64);
    for (n, &(cx, cy)) in cores.iter().enumerate() {
        let core = (cx..(cx + CORE).min(w), cy..(cy + CORE).min(h));
        let (x0, y0) = (cx.saturating_sub(HALO), cy.saturating_sub(HALO));
        let (x1, y1) = ((core.0.end + HALO).min(w), (core.1.end + HALO).min(h));
        let rw = (x1 - x0) as usize;
        let rgb = colour.rgb((x0, y0, x1 - x0, y1 - y0))?;
        let (values, dab, is_tissue) = channel(&rgb, white, options);
        let (labels, count) = segment(
            &values,
            rw,
            threshold,
            options.seed_radius,
            options.min_distance,
        );
        let mut channels: Vec<&[f32]> = vec![&values];
        if options.dab {
            channels.push(&dab);
        }
        for o in measure(&labels, rw, count, pixel, &channels) {
            let at = (
                x0 as f64 + o.centroid[0] / pixel[0],
                y0 as f64 + o.centroid[1] / pixel[1],
            );
            let inside = |v: f64, r: &std::ops::Range<u64>| r.start as f64 <= v && v < r.end as f64;
            if !inside(at.0, &core.0) || !inside(at.1, &core.1) || !filters.keep(&o) {
                continue;
            }
            let ring = outline(&labels, rw, &o)
                .iter()
                .map(|c| [(x0 as f64 + c[0]) * pixel[0], (y0 as f64 + c[1]) * pixel[1]])
                .collect();
            let dab = options.dab.then(|| {
                let mean = o.means[1];
                (mean, dab_class(TISSUE, mean as f32, &options.thresholds))
            });
            found.push(Found {
                centroid: [at.0 * pixel[0], at.1 * pixel[1]],
                area: o.area,
                perimeter: o.perimeter,
                circularity: o.circularity,
                mean: o.means[0],
                dab,
                ring,
            });
        }
        for y in core.1.clone() {
            for x in core.0.clone() {
                let i = (y - y0) as usize * rw + (x - x0) as usize;
                tissue += u64::from(colour.grey || is_tissue[i]);
            }
        }
        let share = 0.2 + 0.75 * (n + 1) as f64 / cores.len() as f64;
        if !progress(share, format!("tile {} of {}", n + 1, cores.len())) {
            return Err(StoreError::Cancelled);
        }
    }
    let n = found.len() as u64;
    let tissue_mm2 = tissue as f64 * pixel[0] * pixel[1] / 1e6;
    let mean = |f: fn(&Found) -> f64| match n {
        0 => 0.0,
        _ => found.iter().map(f).sum::<f64>() / n as f64,
    };
    let mut classes = DabCounts::default();
    found
        .iter()
        .filter_map(|f| f.dab)
        .for_each(|(_, c)| classes.add(c));
    let summary = CountSummary {
        count: n,
        tissue_mm2,
        per_mm2: if tissue_mm2 > 0.0 {
            n as f64 / tissue_mm2
        } else {
            0.0
        },
        mean_area: mean(|f| f.area),
        mean_circularity: mean(|f| f.circularity),
        level,
        white,
        dab: options.dab.then_some(classes),
    };
    // The table, the outlines and the summary, a result of the project.
    let mut csv = String::from("id,x_um,y_um,area_um2,perimeter_um,circularity,mean");
    csv += if options.dab {
        ",dab,class\r\n"
    } else {
        "\r\n"
    };
    let names = ["glass", "negative", "weak", "moderate", "strong"];
    for (k, f) in found.iter().enumerate() {
        let row = [
            k as f64 + 1.0,
            f.centroid[0],
            f.centroid[1],
            f.area,
            f.perimeter,
            f.circularity,
            f.mean,
        ];
        let row: Vec<String> = row.iter().map(f64::to_string).collect();
        let _ = write!(csv, "{}", row.join(","));
        if let Some((dab, class)) = f.dab {
            let _ = write!(csv, ",{dab},{}", names[class as usize]);
        }
        csv += "\r\n";
    }
    let features: Vec<Feature> = found
        .iter()
        .enumerate()
        .map(|(k, f)| {
            let mut properties = Map::new();
            properties.insert("name".to_owned(), json!(format!("cell {}", k + 1)));
            properties.insert("area".to_owned(), json!(f.area));
            properties.insert("circularity".to_owned(), json!(f.circularity));
            if let Some((_, class)) = f.dab {
                properties.insert("class".to_owned(), json!(names[class as usize]));
            }
            let ring = f.ring.iter().map(|c| c.to_vec()).collect();
            Feature {
                id: None,
                geometry: Geometry::Polygon(vec![ring]),
                properties,
            }
        })
        .collect();
    let collection = json!({"type": "FeatureCollection", "features": features});
    let name = record_name(store, project, kind, &id)?;
    let provenance = Provenance {
        recipe: json!({ COUNT: options, "white": white, "level": level }),
        nodes: vec![NodeVersion {
            node: COUNT.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            extension: None,
        }],
        inputs: vec![source],
        hashes: Default::default(),
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    };
    let draft = store.result(project, &format!("{name}: cells"), provenance, None)?;
    let text = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_default() + "\n";
    let files = [
        ("cells.csv", csv),
        ("cells.geojson", text(&collection)),
        ("summary.json", text(&json!(summary))),
    ];
    for (file, body) in files {
        let path = draft.folder().join(file);
        std::fs::write(&path, body).map_err(|source| StoreError::Io { path, source })?;
    }
    let result = draft.commit()?;
    let annotated = store.replace_layer(project, kind, &id, "cells", features)?;
    Ok(Counted {
        result,
        summary,
        annotated,
    })
}

/// The name of `kind` record `id` of `project`.
fn record_name(
    store: &Store,
    project: ProjectId,
    kind: RecordKind,
    id: &str,
) -> Result<String, StoreError> {
    let contents = store.contents(project)?;
    let name = match kind {
        RecordKind::Acquisition => contents
            .acquisitions
            .into_iter()
            .find(|a| a.id.to_string() == id)
            .map(|a| a.name),
        _ => contents
            .results
            .into_iter()
            .find(|r| r.id.to_string() == id)
            .map(|r| r.name),
    };
    Ok(name.unwrap_or_else(|| id.to_owned()))
}

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    input: Input,
    #[serde(default)]
    options: CountOptions,
}

/// Makes cell counting jobs, whose parameters are `project`, `input`, an
/// acquisition or a result, as `{"kind": "result", "id": …}`, and
/// `options`.
pub fn count_jobs(store: Arc<Store>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: COUNT.to_owned(),
            title: "count cells".to_owned(),
            account: None,
            params: value.clone(),
            resource: Resource::Compute,
            pausable: false,
            gpu_mib: 0,
        };
        let store = Arc::clone(&store);
        Ok(Job {
            spec,
            run: Box::new(move |ctx| Box::pin(run(store, params, ctx))),
        })
    })
}

async fn run(store: Arc<Store>, params: Params, ctx: JobCtx) -> Result<(), Stop> {
    let job = ctx.clone();
    let project = params.project;
    let work = tokio::task::spawn_blocking(move || {
        let progress = |share: f64, what: String| {
            job.progress(Some(share), what);
            !job.is_cancelled()
        };
        count(&store, project, params.input, &params.options, &progress)
    });
    match work.await {
        Ok(Ok(counted)) => {
            ctx.progress(Some(1.0), "done");
            ctx.add_result(format!("projects/{project}/results/{}", counted.result.id));
            Ok(())
        }
        Ok(Err(StoreError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
