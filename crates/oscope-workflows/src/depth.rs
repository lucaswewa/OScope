//! Depth from focus as a job (P42): a volume acquisition's heights, their
//! confidence and its all-in-focus image, each a result of its project,
//! and its mesh if asked. P42's notes give the behaviour.
//!
//! - **Grey and colour planes:** a raw stack is measured on its cells'
//!   means and coloured by its cells, its red, greens' mean and blue, at
//!   half its size; other stacks by their channels' mean, and as they are.
//! - **Two passes,** a plane at a time: the focus measures to the heights,
//!   then the planes again to the all-in-focus image, so memory is a few
//!   planes', whatever the stack's depth.
//! - **On the GPU** when there is one, through the compute layer.

use std::path::PathBuf;
use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig, Image, Lane, Shape, cpu};
use oscope_core::{AcquisitionId, BayerPattern, ProjectId};
use oscope_imaging::color::{Demosaic, demosaic};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::zoom::record_image;
use oscope_store::{
    ImageInfo, ImageReader, Input, NodeVersion, PixelSize, Provenance, RecordKind, ResultRecord,
    SampleType, Samples, Store, StoreError,
};
use oscope_volume::depth::{self, AllInFocus, FocusOptions, Peaks, Planes};
use oscope_volume::{Mesh, MeshFormat, MeshOptions, VolumeError, write_mesh};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The job's kind.
pub const DEPTH: &str = "volume.depth_from_focus";

/// A mesh's vertices at most: its stride is chosen to keep under it.
const MESH_VERTICES: u64 = 1_000_000;

/// Depth from focus's options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct DepthOptions {
    /// The focus measure and the median.
    pub focus: FocusOptions,
    /// A mesh of the heights, in this format, if asked.
    pub mesh: Option<MeshFormat>,
    /// The confidence a mesh's square needs at its four corners.
    pub min_confidence: f32,
}

/// Why depth from focus, or deconvolution (P43), stopped.
#[derive(Debug, thiserror::Error)]
pub enum DepthError {
    /// The store failed, or refused.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Depth from focus refused, or failed.
    #[error(transparent)]
    Volume(#[from] VolumeError),
    /// It was cancelled.
    #[error("cancelled")]
    Cancelled,
}

/// What depth from focus made.
#[derive(Clone, Debug)]
pub struct Depths {
    /// The heights, in µm.
    pub heights: ResultRecord,
    /// Their confidence, from 0 to 1.
    pub confidence: ResultRecord,
    /// The all-in-focus image.
    pub image: ResultRecord,
    /// The mesh's files, if one was asked for.
    pub mesh: Vec<PathBuf>,
    /// The share of pixels whose confidence is 0.5 or more.
    pub sure: f64,
}

/// A stack's planes, read one at a time: grey to measure, colour to fuse.
pub(crate) struct Stack {
    pub(crate) reader: ImageReader,
    pub(crate) info: ImageInfo,
    pub(crate) pattern: Option<BayerPattern>,
}

impl Stack {
    /// Plane `k` in unit samples, at full size.
    fn read(&self, k: u32) -> Result<Image, DepthError> {
        let (w, h) = (self.info.width, self.info.height);
        let unit = self.reader.read(0, k, (0, 0), (w, h))?.to_unit();
        let shape = Shape::new(w as u32, h as u32, self.info.channels as usize);
        Ok(Image::from_vec(shape, unit).map_err(VolumeError::from)?)
    }

    /// Plane `k` to measure: a raw plane's cells' means, or the channels'
    /// mean.
    pub(crate) fn grey(&self, k: u32) -> Result<Image, DepthError> {
        let plane = self.read(k)?;
        let c = plane.channels();
        let (scale, image) = match self.pattern {
            Some(_) => (0.25, cpu::bin2x2(&plane)),
            None => (1.0 / c as f32, plane),
        };
        let data = image
            .data()
            .chunks(image.channels())
            .map(|p| scale * p.iter().sum::<f32>())
            .collect();
        let shape = Shape::new(image.width(), image.height(), 1);
        Ok(Image::from_vec(shape, data).map_err(VolumeError::from)?)
    }

    /// Plane `k` to fuse: a raw plane's cells as RGB pixels, or as it is.
    fn colour(&self, k: u32) -> Result<Image, DepthError> {
        let plane = self.read(k)?;
        match self.pattern {
            Some(pattern) => demosaic(&plane, pattern, Demosaic::Superpixel)
                .map_err(|e| VolumeError::Refused(e.to_string()).into()),
            None => Ok(plane),
        }
    }
}

/// Depth from focus over acquisition `id` of `project`, a volume of 3
/// planes or more, measured through `compute`. `progress` is told the share
/// done and what, and stops it by answering false. Its results are the
/// project's, and its mesh, if asked, is in the heights' `exports` folder.
pub fn depth_from_focus(
    store: &Store,
    project: ProjectId,
    id: AcquisitionId,
    options: &DepthOptions,
    compute: &Compute,
    progress: &dyn Fn(f64, String) -> bool,
) -> Result<Depths, DepthError> {
    let (path, pattern) = record_image(store, project, RecordKind::Acquisition, &id.to_string())?;
    let reader = ImageReader::open(&path)?;
    let info = reader.info().clone();
    let refused = |why: String| DepthError::Store(StoreError::Refused(why));
    if !info.volume || info.planes < 3 {
        return Err(refused(format!(
            "acquisition {id} is {}a volume of {} planes: depth from focus needs a z-stack of 3 or more",
            if info.volume { "" } else { "not " },
            info.planes
        )));
    }
    let contents = store.contents(project)?;
    let record = contents.acquisitions.iter().find(|a| a.id == id);
    let first_z = record
        .and_then(|a| a.frames.first())
        .and_then(|f| f.position)
        .map_or(0.0, |p| p.z.0);
    let step = info.pixel_size.map_or(1.0, |p| p.z);
    let planes = Planes {
        z0_um: first_z,
        step_um: step,
    };
    let name = record.map_or_else(|| id.to_string(), |a| a.name.clone());
    let stack = Stack {
        reader,
        info: info.clone(),
        pattern,
    };
    let n = info.planes;
    let focus = options.focus;
    let go = |share: f64, what: String| match progress(share, what) {
        true => Ok(()),
        false => Err(DepthError::Cancelled),
    };

    // The heights, from each plane's focus measures.
    let mut peaks: Option<Peaks> = None;
    for k in 0..n {
        go(
            0.5 * f64::from(k) / f64::from(n),
            format!("plane {} of {n} measured", k + 1),
        )?;
        let grey = stack.grey(k)?;
        let measures = compute
            .focus_sml(Lane::Batch, &grey, focus.step, focus.radius)
            .map_err(VolumeError::from)?;
        let peaks = peaks.get_or_insert_with(|| Peaks::new(grey.width(), grey.height()));
        peaks.push(&measures)?;
    }
    let found = peaks.expect("3 planes or more").finish(planes)?;
    let heights = depth::median(&found.heights, focus.median)?;

    // The all-in-focus image, from each plane again.
    let channels = match pattern {
        Some(_) => 3,
        None => info.channels as usize,
    };
    let mut fused = AllInFocus::new(&heights, planes, n, channels)?;
    for k in 0..n {
        go(
            0.5 + 0.45 * f64::from(k) / f64::from(n),
            format!("plane {} of {n} fused", k + 1),
        )?;
        fused.push(&stack.colour(k)?)?;
    }
    let fused = fused.finish()?;
    go(0.95, "the results".to_owned())?;

    // The results, at the planes' working size: a raw stack's pixels are
    // its cells.
    let cell = match pattern {
        Some(_) => 2.0,
        None => 1.0,
    };
    let pixel = info.pixel_size.map(|p| PixelSize {
        x: p.x * cell,
        y: p.y * cell,
        z: p.z,
    });
    let provenance = Provenance {
        recipe: json!({ "depth_from_focus": options, "planes": planes }),
        nodes: vec![NodeVersion {
            node: DEPTH.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            extension: None,
        }],
        inputs: vec![Input::Acquisition(id)],
        hashes: Default::default(),
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    };
    let save = |what: &str, image: &Image, sample: SampleType, names: &[&str]| {
        let mut info = ImageInfo::new(
            u64::from(image.width()),
            u64::from(image.height()),
            image.channels() as u32,
            sample,
        );
        info.pixel_size = pixel;
        info.channel_names = names.iter().map(|n| (*n).to_owned()).collect();
        let draft = store.result(
            project,
            &format!("{name} {what}"),
            provenance.clone(),
            Some(&info),
        )?;
        let size = (u64::from(image.width()), u64::from(image.height()));
        let samples = match sample {
            SampleType::F32 => Samples::F32(image.data().to_vec()),
            other => Samples::from_unit(other, image.data()),
        };
        let writer = draft.image().expect("a result with an image has a writer");
        writer.write(0, (0, 0), size, &samples)?;
        Ok::<_, DepthError>(draft.commit()?)
    };
    let colours: &[&str] = match channels {
        3 => &["red", "green", "blue"],
        _ => &["grey"],
    };
    let sure = found
        .confidence
        .data()
        .iter()
        .filter(|c| **c >= 0.5)
        .count() as f64
        / found.confidence.data().len().max(1) as f64;
    let heights_record = save("heights", &heights, SampleType::F32, &["height"])?;
    let confidence = save(
        "confidence",
        &found.confidence,
        SampleType::F32,
        &["confidence"],
    )?;
    let image = save("all in focus", &fused, info.sample, colours)?;

    // The mesh, if asked, of a million vertices at most.
    let mut mesh = Vec::new();
    if let Some(format) = options.mesh {
        let pixels = u64::from(heights.width()) * u64::from(heights.height());
        let stride = (pixels as f64 / MESH_VERTICES as f64)
            .sqrt()
            .ceil()
            .max(1.0) as u32;
        let mesh_options = MeshOptions {
            pixel_um: pixel.map_or(1.0, |p| p.x),
            stride,
            min_confidence: options.min_confidence,
        };
        let surface = Mesh::new(
            &heights,
            Some(&found.confidence),
            Some(&fused),
            &mesh_options,
        )?;
        let folder = store
            .folder(project)
            .join("results")
            .join(heights_record.id.to_string())
            .join("exports");
        mesh = write_mesh(&surface, format, Some(&fused), &folder, "surface")?;
    }
    progress(1.0, "done".to_owned());
    Ok(Depths {
        heights: heights_record,
        confidence,
        image,
        mesh,
        sure,
    })
}

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    acquisition: AcquisitionId,
    #[serde(default)]
    options: DepthOptions,
}

/// Makes depth-from-focus jobs, whose parameters are `project`,
/// `acquisition` and, optionally, `options`. Each runs on the GPU when there
/// is one.
pub fn depth_jobs(store: Arc<Store>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: DEPTH.to_owned(),
            title: format!("depth from focus of acquisition {}", params.acquisition),
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
        let compute = Compute::new(&ComputeConfig::default())
            .map_err(|e| DepthError::Volume(VolumeError::from(e)))?;
        let progress = |share: f64, what: String| {
            job.progress(Some(share), what);
            !job.is_cancelled()
        };
        depth_from_focus(
            &store,
            project,
            params.acquisition,
            &params.options,
            &compute,
            &progress,
        )
    });
    match work.await {
        Ok(Ok(made)) => {
            ctx.progress(Some(1.0), "done");
            for record in [&made.heights, &made.confidence, &made.image] {
                ctx.add_result(format!("projects/{project}/results/{}", record.id));
            }
            ctx.add_result(format!(
                "{:.0} % of the pixels sure; {} mesh files",
                100.0 * made.sure,
                made.mesh.len()
            ));
            Ok(())
        }
        Ok(Err(DepthError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
