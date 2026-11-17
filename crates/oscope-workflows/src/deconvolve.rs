//! Deconvolution as a job (P43): a volume acquisition deconvolved by Born
//! and Wolf's widefield PSF, by Richardson–Lucy over it padded by
//! mirroring, its result a volume of the project. A raw stack is
//! deconvolved on its cells' means, and any stack's background, its 1st
//! percentile, is taken off first. P43's notes give the behaviour.

use std::sync::Arc;

use oscope_compute::{Compute, ComputeConfig};
use oscope_core::{AcquisitionId, ProjectId};
use oscope_jobs::{Factory, Job, JobCtx, JobSpec, Resource, Stop};
use oscope_store::zoom::record_image;
use oscope_store::{
    ImageInfo, ImageReader, Input, NodeVersion, PixelSize, Provenance, RecordKind, ResultRecord,
    SampleType, Samples, Store, StoreError,
};
use oscope_volume::{DeconvolveOptions, Optics, Volume, VolumeError, deconvolve, psf};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::depth::{DepthError, Stack};

/// The job's kind.
pub const DECONVOLVE: &str = "volume.deconvolve";

fn green() -> f64 {
    0.519
}

fn air() -> f64 {
    1.0
}

fn twenty() -> u32 {
    20
}

/// Deconvolution's options.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeconvolveJobOptions {
    /// The objective's numerical aperture.
    pub na: f64,
    /// The emission's wavelength, in µm: the green dye's, 0.519, unless
    /// given.
    #[serde(default = "green")]
    pub wavelength_um: f64,
    /// The medium's refractive index: 1, a dry objective's, unless given.
    #[serde(default = "air")]
    pub medium: f64,
    /// Richardson–Lucy's iterations.
    #[serde(default = "twenty")]
    pub iterations: u32,
    /// Total variation's weight: 0, none, unless given.
    #[serde(default)]
    pub tv: f32,
}

/// Deconvolves acquisition `id` of `project`, a volume of known voxels,
/// through `compute`: a result of the project, a volume of the deconvolved
/// voxels in floats. `progress` is told the share done and what, and stops
/// it by answering false.
pub fn deconvolve_acquisition(
    store: &Store,
    project: ProjectId,
    id: AcquisitionId,
    options: &DeconvolveJobOptions,
    compute: &Compute,
    progress: &dyn Fn(f64, String) -> bool,
) -> Result<ResultRecord, DepthError> {
    let (path, pattern) = record_image(store, project, RecordKind::Acquisition, &id.to_string())?;
    let reader = ImageReader::open(&path)?;
    let info = reader.info().clone();
    let refused = |why: String| DepthError::Store(StoreError::Refused(why));
    let size = match (info.volume, info.pixel_size) {
        (true, Some(size)) => size,
        _ => {
            return Err(refused(format!(
                "acquisition {id} isn't a volume of known voxels: deconvolution needs one"
            )));
        }
    };
    let cell = if pattern.is_some() { 2.0 } else { 1.0 };
    let voxel = [size.x * cell, size.y * cell, size.z];
    let stack = Stack {
        reader,
        info: info.clone(),
        pattern,
    };
    let planes = (0..info.planes)
        .map(|k| stack.grey(k))
        .collect::<Result<Vec<_>, _>>()?;
    let mut volume = Volume::from_planes(&planes)?;
    // The background, such as a camera's black level, taken off first: the
    // stack's 1st percentile, from a sample of its voxels. Richardson-Lucy
    // would take it for light, spread everywhere.
    let step = (volume.data.len() / 100_000).max(1);
    let mut sample: Vec<f32> = volume.data.iter().step_by(step).copied().collect();
    sample.sort_by(f32::total_cmp);
    let background = sample[sample.len() / 100];
    volume
        .data
        .iter_mut()
        .for_each(|v| *v = (*v - background).max(0.0));
    let optics = Optics {
        na: options.na,
        wavelength_um: options.wavelength_um,
        medium: options.medium,
    };
    let kernel = psf(&optics, voxel, volume.size)?;
    let rl = DeconvolveOptions {
        iterations: options.iterations,
        tv: options.tv,
    };
    let total = options.iterations.max(1);
    let mut going = |k: u32| {
        let share = 0.95 * f64::from(k) / f64::from(total);
        progress(share, format!("iteration {k} of {total}"))
    };
    let sharpened =
        deconvolve(&volume, &kernel, &rl, compute, &mut going)?.ok_or(DepthError::Cancelled)?;

    let [w, h, d] = sharpened.size;
    let mut result = ImageInfo::new(w as u64, h as u64, 1, SampleType::F32);
    (result.planes, result.volume) = (d as u32, true);
    result.pixel_size = Some(PixelSize {
        x: voxel[0],
        y: voxel[1],
        z: voxel[2],
    });
    result.channel_names = vec!["deconvolved".to_owned()];
    let name = store
        .contents(project)?
        .acquisitions
        .into_iter()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_string(), |a| a.name);
    let provenance = Provenance {
        recipe: json!({ "deconvolve": options, "voxel_um": voxel }),
        nodes: vec![NodeVersion {
            node: DECONVOLVE.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            extension: None,
        }],
        inputs: vec![Input::Acquisition(id)],
        hashes: Default::default(),
        software: format!("OScope {}", env!("CARGO_PKG_VERSION")),
    };
    let draft = store.result(
        project,
        &format!("{name} deconvolved"),
        provenance,
        Some(&result),
    )?;
    let writer = draft.image().expect("a result with an image has a writer");
    for (k, plane) in sharpened.data.chunks(w * h).enumerate() {
        writer.write(
            k as u32,
            (0, 0),
            (w as u64, h as u64),
            &Samples::F32(plane.to_vec()),
        )?;
    }
    let record = draft.commit()?;
    progress(1.0, "done".to_owned());
    Ok(record)
}

/// The job's parameters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    project: ProjectId,
    acquisition: AcquisitionId,
    options: DeconvolveJobOptions,
}

/// Makes deconvolution jobs, whose parameters are `project`, `acquisition`
/// and `options`, its `na` at least. Each runs on the GPU when there is one.
pub fn deconvolve_jobs(store: Arc<Store>) -> Factory {
    Arc::new(move |value| {
        let params = Params::deserialize(value).map_err(|e| e.to_string())?;
        let spec = JobSpec {
            kind: DECONVOLVE.to_owned(),
            title: format!("deconvolve acquisition {}", params.acquisition),
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
        deconvolve_acquisition(
            &store,
            project,
            params.acquisition,
            &params.options,
            &compute,
            &progress,
        )
    });
    match work.await {
        Ok(Ok(record)) => {
            ctx.progress(Some(1.0), "done");
            ctx.add_result(format!("projects/{project}/results/{}", record.id));
            Ok(())
        }
        Ok(Err(DepthError::Cancelled)) => Err(Stop::Cancelled),
        Ok(Err(error)) => Err(Stop::failed(error)),
        Err(error) => Err(Stop::failed(error)),
    }
}
