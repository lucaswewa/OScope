//! Stain analysis as a job (P45): a stored brightfield image's stains
//! separated tile by tile, by colour deconvolution, into three results of
//! its project: the stains' amounts, and each stain's image. A raw image is
//! taken on its 2 × 2 cells. P45's notes give the behaviour.

use std::sync::Arc;

use oscope_core::{BayerColour, BayerPattern, ProjectId};
use oscope_imaging::stain::{
    DabCounts, DabThresholds, StainChoice, Stains, TISSUE, Vector, dab_class, macenko,
    optical_density,
};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::zoom::record_image;
use oscope_store::{
    CHUNK, ImageInfo, ImageReader, Input, NodeVersion, PixelSize, Provenance, RecordKind,
    ResultRecord, SampleType, Samples, Store, StoreError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The job's kind.
pub const SEPARATE: &str = "stain.separate";

/// Stain separation's options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct StainOptions {
    /// The stains: H&E unless given.
    pub stains: StainChoice,
    /// Whether to estimate the first two stains' vectors from the image, by
    /// Macenko's method.
    pub estimate: bool,
    /// The light through glass, from 0 to 1 of full scale: each channel's
    /// 99.5th percentile unless given.
    pub white: Option<[f32; 3]>,
    /// Whether to bin the second stain as DAB: so for H-DAB unless given.
    pub dab: Option<bool>,
    /// DAB's thresholds.
    pub thresholds: DabThresholds,
}

/// DAB's bins over a slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DabSummary {
    /// The pixels in each class.
    pub counts: DabCounts,
    /// The positive pixels' percentage of the tissue's.
    pub positive_percent: f64,
    /// The H-score, from 0 to 300.
    pub h_score: f64,
}

/// What a separation found: its `summary.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StainSummary {
    /// The stains' names: the first, the second, and the third or the
    /// residual.
    pub names: [String; 3],
    /// Their vectors, as used.
    pub vectors: [Vector; 3],
    /// Whether the first two were Macenko's estimate.
    pub estimated: bool,
    /// The light through glass, from 0 to 1 of full scale.
    pub white: [f32; 3],
    /// The tissue's pixels: those of summed optical density 0.05 or more.
    pub tissue: u64,
    /// The first two stains' mean amounts over the tissue.
    pub means: [f64; 2],
    /// The third channel's root mean square over the tissue: a residual's
    /// is what the stains don't explain.
    pub residual_rms: f64,
    /// DAB's bins, if the second stain is DAB.
    pub dab: Option<DabSummary>,
}

/// What a separation made: the stains' amounts and each stain's image,
/// results of the project, and its summary.
#[derive(Clone, Debug)]
pub struct Separated {
    /// The amounts, three channels of floats, with `summary.json` in its
    /// folder.
    pub amounts: ResultRecord,
    /// The first stain's image and the second's, in 8-bit RGB.
    pub images: [ResultRecord; 2],
    /// The summary.
    pub summary: StainSummary,
}

/// A stored image read in colour, tile by tile: a raw one on its 2 × 2
/// cells, each one RGB pixel.
struct Colour {
    reader: ImageReader,
    pattern: Option<BayerPattern>,
    size: (u64, u64),
}

impl Colour {
    /// Its tiles on its chunks' grid: left, top, width and height.
    fn tiles(&self) -> Vec<(u64, u64, u64, u64)> {
        let side = if self.pattern.is_some() {
            CHUNK / 2
        } else {
            CHUNK
        };
        let (w, h) = self.size;
        let ys = (0..h).step_by(side as usize);
        ys.flat_map(|y| {
            let xs = (0..w).step_by(side as usize);
            xs.map(move |x| (x, y, side.min(w - x), side.min(h - y)))
        })
        .collect()
    }

    /// A tile's RGB, from 0 to 1 of full scale.
    fn rgb(&self, (x, y, w, h): (u64, u64, u64, u64)) -> Result<Vec<f32>, StoreError> {
        let Some(pattern) = self.pattern else {
            return Ok(self.reader.read(0, 0, (x, y), (w, h))?.to_unit());
        };
        let raw = self
            .reader
            .read(0, 0, (2 * x, 2 * y), (2 * w, 2 * h))?
            .to_unit();
        let mut rgb = vec![0.0_f32; (w * h * 3) as usize];
        for j in 0..h as usize {
            for i in 0..w as usize {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let v = raw[(2 * j + dy) * 2 * w as usize + 2 * i + dx];
                    let (c, share) = match pattern.colour_at(dx as u32, dy as u32) {
                        BayerColour::Red => (0, 1.0),
                        BayerColour::Green => (1, 0.5),
                        BayerColour::Blue => (2, 1.0),
                    };
                    rgb[(j * w as usize + i) * 3 + c] += share * v;
                }
            }
        }
        Ok(rgb)
    }
}

/// The stains' names, by the choice.
fn names(choice: &StainChoice) -> [String; 3] {
    let names = match choice {
        StainChoice::He => ["hematoxylin", "eosin", "residual"],
        StainChoice::Hdab => ["hematoxylin", "DAB", "residual"],
        StainChoice::Vectors(v) if v.len() == 3 => ["first stain", "second stain", "third stain"],
        StainChoice::Vectors(_) => ["first stain", "second stain", "residual"],
    };
    names.map(str::to_owned)
}

/// Separates the stains of `source`'s image of `project`, as `options` say,
/// into three results of the project. `progress` is told the share done
/// and what, and stops it by answering false.
pub fn separate(
    store: &Store,
    project: ProjectId,
    source: Input,
    options: &StainOptions,
    progress: &dyn Fn(f64, String) -> bool,
) -> Result<Separated, StoreError> {
    let refused = |why: String| StoreError::Refused(why);
    let (kind, id) = match source {
        Input::Acquisition(a) => (RecordKind::Acquisition, a.to_string()),
        Input::Result(r) => (RecordKind::Result, r.to_string()),
    };
    let (path, pattern) = record_image(store, project, kind, &id)?;
    let reader = ImageReader::open(&path)?;
    let info = reader.info().clone();
    let cell = match (pattern, info.channels) {
        (Some(_), _) => 2,
        (None, 3) => 1,
        _ => {
            return Err(refused(format!(
                "{} {id} isn't in colour: stains are separated in RGB or a raw mosaic",
                kind.name()
            )));
        }
    };
    let colour = Colour {
        reader,
        pattern,
        size: (info.width / cell, info.height / cell),
    };
    let tiles = colour.tiles();
    let mut stains = options
        .stains
        .stains()
        .map_err(|e| refused(e.to_string()))?;
    // A first pass, for the white and the vectors if they're to be found:
    // every 8th pixel each way.
    let mut white = options.white.unwrap_or([1.0; 3]);
    if options.white.is_none() || options.estimate {
        let mut sample = Vec::new();
        for (n, &tile) in tiles.iter().enumerate() {
            let rgb = colour.rgb(tile)?;
            let (w, h) = (tile.2 as usize, tile.3 as usize);
            for j in (0..h).step_by(8) {
                for i in (0..w).step_by(8) {
                    sample.extend_from_slice(&rgb[(j * w + i) * 3..(j * w + i) * 3 + 3]);
                }
            }
            if !progress(
                0.3 * (n + 1) as f64 / tiles.len() as f64,
                "sampled".to_owned(),
            ) {
                return Err(StoreError::Cancelled);
            }
        }
        if options.white.is_none() {
            white = [0, 1, 2].map(|c| {
                let mut v: Vec<f32> = sample.iter().skip(c).step_by(3).copied().collect();
                v.sort_by(f32::total_cmp);
                v.get((v.len().saturating_sub(1)) * 995 / 1000)
                    .copied()
                    .unwrap_or(1.0)
            });
        }
        if options.estimate {
            let [a, b] = macenko(&sample, white, 0.15, 1.0).map_err(|e| refused(e.to_string()))?;
            stains = Stains::new(a, b, None).map_err(|e| refused(e.to_string()))?;
        }
    }
    let dab = options.dab.unwrap_or(options.stains == StainChoice::Hdab);
    let names = names(&options.stains);
    let name = record_name(store, project, kind, &id)?;
    let name = match options.estimate {
        true => format!("{name} (Macenko)"),
        false => name,
    };
    let provenance = Provenance {
        recipe: json!({ SEPARATE: options, "white": white, "vectors": stains.vectors() }),
        nodes: vec![NodeVersion {
            node: SEPARATE.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            extension: None,
        }],
        inputs: vec![source],
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    };
    let (w, h) = colour.size;
    let scaled = info.pixel_size.map(|s| PixelSize {
        x: s.x * cell as f64,
        y: s.y * cell as f64,
        z: s.z,
    });
    let made = |sample: SampleType, channels: [&str; 3]| {
        let mut made = ImageInfo::new(w, h, 3, sample);
        made.pixel_size = scaled;
        made.channel_names = channels.map(str::to_owned).to_vec();
        made
    };
    let amounts_info = made(SampleType::F32, [&names[0], &names[1], &names[2]]);
    let rgb_info = made(SampleType::U8, ["red", "green", "blue"]);
    let amounts_draft = store.result(
        project,
        &format!("{name}: stain amounts"),
        provenance.clone(),
        Some(&amounts_info),
    )?;
    let image_drafts = [0, 1].map(|k| {
        store.result(
            project,
            &format!("{name}: {}", names[k]),
            provenance.clone(),
            Some(&rgb_info),
        )
    });
    let [first_draft, second_draft] = image_drafts;
    let image_drafts = [first_draft?, second_draft?];
    // The second pass: each tile deconvolved, written, and summed.
    let (mut tissue, mut sums, mut squares) = (0_u64, [0.0_f64; 2], 0.0_f64);
    let mut counts = DabCounts::default();
    for (n, &tile) in tiles.iter().enumerate() {
        let rgb = colour.rgb(tile)?;
        let amounts = stains.deconvolve(&rgb, white);
        for (pixel, a) in rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(amounts.as_chunks::<3>().0)
        {
            let od: f32 = (0..3).map(|c| optical_density(pixel[c], white[c])).sum();
            if od >= TISSUE {
                tissue += 1;
                sums[0] += f64::from(a[0]);
                sums[1] += f64::from(a[1]);
                squares += f64::from(a[2]) * f64::from(a[2]);
            }
            if dab {
                counts.add(dab_class(od, a[1], &options.thresholds));
            }
        }
        let at = (tile.0, tile.1);
        let size = (tile.2, tile.3);
        let writer = amounts_draft
            .image()
            .expect("a result with an image has a writer");
        writer.write(0, at, size, &Samples::F32(amounts.clone()))?;
        for (k, draft) in image_drafts.iter().enumerate() {
            let alone = stains.alone(k, &amounts, white);
            let bytes = alone
                .iter()
                .enumerate()
                .map(|(i, v)| (255.0 * v / white[i % 3]).round().clamp(0.0, 255.0) as u8)
                .collect();
            let writer = draft.image().expect("a result with an image has a writer");
            writer.write(0, at, size, &Samples::U8(bytes))?;
        }
        if !progress(
            0.3 + 0.65 * (n + 1) as f64 / tiles.len() as f64,
            format!("tile {} of {}", n + 1, tiles.len()),
        ) {
            return Err(StoreError::Cancelled);
        }
    }
    let per = |v: f64| if tissue == 0 { 0.0 } else { v / tissue as f64 };
    let summary = StainSummary {
        names,
        vectors: stains.vectors(),
        estimated: options.estimate,
        white,
        tissue,
        means: sums.map(per),
        residual_rms: per(squares).sqrt(),
        dab: dab.then(|| DabSummary {
            counts,
            positive_percent: counts.positive_percent(),
            h_score: counts.h_score(),
        }),
    };
    let text = serde_json::to_string_pretty(&summary).map_err(|e| refused(e.to_string()))?;
    let file = amounts_draft.folder().join("summary.json");
    std::fs::write(&file, text + "\n").map_err(|source| StoreError::Io { path: file, source })?;
    let amounts = amounts_draft.commit()?;
    let [a, b] = image_drafts;
    let images = [a.commit()?, b.commit()?];
    Ok(Separated {
        amounts,
        images,
        summary,
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
    options: StainOptions,
}

/// Makes stain separation jobs, whose parameters are `project`, `input`, an
/// acquisition or a result, as `{"kind": "result", "id": …}`, and
/// `options`.
pub fn stain_jobs(store: Arc<Store>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: SEPARATE.to_owned(),
            title: "separate stains".to_owned(),
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
        separate(&store, project, params.input, &params.options, &progress)
    });
    match work.await {
        Ok(Ok(made)) => {
            ctx.progress(Some(1.0), "done");
            let records = [&made.amounts, &made.images[0], &made.images[1]];
            for record in records {
                ctx.add_result(format!("projects/{project}/results/{}", record.id));
            }
            Ok(())
        }
        Ok(Err(StoreError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
