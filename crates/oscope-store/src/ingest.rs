//! Ingest (plan §5.5, ADR-0022): a recording's frames into an acquisition's
//! OME-Zarr store, compressed, several frames at once.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use oscope_core::{FrameMeta, PixelFormat, ProjectId};
use rayon::prelude::*;
use serde_json::{Value, json};

use crate::image::{ImageInfo, PixelSize, SampleType, Samples};
use crate::recording::{RecordingReader, in_progress};
use crate::records::{Acquisition, FrameRecord};
use crate::store::Store;
use crate::{Result, StoreError, io};

/// Frames written at once, at most: each holds its pixels twice, 80 MB for
/// a 20-megapixel frame.
const AT_ONCE: usize = 8;

/// An image's samples, channels and channel names for frames of `format`.
fn layout(format: PixelFormat) -> Result<(SampleType, u32, Vec<String>)> {
    let names = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    Ok(match format {
        PixelFormat::Bayer16 { .. } => (SampleType::U16, 1, names(&["raw"])),
        PixelFormat::Bayer8 { .. } => (SampleType::U8, 1, names(&["raw"])),
        PixelFormat::Mono16 => (SampleType::U16, 1, names(&["grey"])),
        PixelFormat::Mono8 => (SampleType::U8, 1, names(&["grey"])),
        PixelFormat::Rgb16 => (SampleType::U16, 3, names(&["red", "green", "blue"])),
        PixelFormat::Rgb8 => (SampleType::U8, 3, names(&["red", "green", "blue"])),
        PixelFormat::Rgba8 => (SampleType::U8, 4, names(&["red", "green", "blue", "alpha"])),
        PixelFormat::PlanarF32 { .. } => {
            return Err(StoreError::Refused(
                "frames of planar floats can't be ingested".to_owned(),
            ));
        }
    })
}

/// The camera's settings, as at a frame.
fn camera(meta: &FrameMeta) -> Value {
    json!({
        "mode": meta.mode,
        "format": meta.format,
        "width": meta.width,
        "height": meta.height,
        "roi": meta.roi,
        "binning": meta.binning,
        "exposure_ms": meta.exposure,
        "gain": meta.gain,
        "black_level": meta.black_level,
        "line_time_ms": meta.line_time,
    })
}

/// A scan's pixel size, from its `plan` (ADR-0025) and its frames' `width`:
/// the frame's top edge, its corners' distance on the stage, over its
/// pixels; and for a z-scan, a volume's voxel, the stack's step between
/// planes. None for a plan without a field.
fn voxel(plan: &Value, width: u32) -> Option<PixelSize> {
    if width == 0 {
        return None;
    }
    let corner = |k: usize, axis: &str| plan["field"]["corners_um"][k][axis].as_f64();
    let across = (corner(1, "x")? - corner(0, "x")?).hypot(corner(1, "y")? - corner(0, "y")?);
    let pixel = across / f64::from(width);
    // A z-scan's planes are a step apart; others' are frames elsewhere.
    let step = match plan["pattern"] == "z_scan" {
        true => plan["stack"]["step_um"].as_f64()?,
        false => 1.0,
    };
    (pixel > 0.0 && step > 0.0).then_some(PixelSize {
        x: pixel,
        y: pixel,
        z: step,
    })
}

/// Writes the frames of the recording in `folder` into a new acquisition of
/// `project`, named `name`, or as the recording if none: a plane for each
/// frame, compressed, with its pyramid, and each frame's position and time
/// in its record, and a scan's plan, from the recording's source's `plan`.
/// `progress` is told how many frames are written, of how
/// many, and stops the ingest by answering false. Once the acquisition is
/// committed, the recording is deleted, unless `keep`.
pub fn ingest(
    store: &Store,
    project: ProjectId,
    folder: &Path,
    name: Option<&str>,
    keep: bool,
    progress: &(dyn Fn(u64, u64) -> bool + Sync),
) -> Result<Acquisition> {
    let reader = RecordingReader::open(folder)?;
    let recording = reader.recording();
    if recording.ended.is_none() || in_progress(folder) {
        return Err(StoreError::Refused(format!(
            "recording {} hasn't ended",
            recording.id
        )));
    }
    let frames = reader.frames();
    let Some(first) = frames.first() else {
        return Err(StoreError::Refused(format!(
            "recording {} has no frames",
            recording.id
        )));
    };
    let shape = |m: &FrameMeta| (m.width, m.height, m.format);
    if let Some(other) = frames.iter().find(|f| shape(&f.meta) != shape(&first.meta)) {
        let (w, h, format) = shape(&other.meta);
        let (w0, h0, format0) = shape(&first.meta);
        return Err(StoreError::Refused(format!(
            "frame {} is {w} × {h} {format:?}, and frame {} {w0} × {h0} {format0:?}: \
             a recording's frames go into one image, of one size and format",
            other.index, first.index
        )));
    }
    let (sample, channels, channel_names) = layout(first.meta.format)?;
    let mut info = ImageInfo::new(
        u64::from(first.meta.width),
        u64::from(first.meta.height),
        channels,
        sample,
    );
    info.planes = frames.len() as u32;
    info.channel_names = channel_names;
    // Any scan's pixels have their size (P44); a z-scan's planes, at one
    // place, are a volume (ADR-0029).
    let plan = &recording.source["plan"];
    if let Some(voxel) = voxel(plan, first.meta.width) {
        info.volume = plan["pattern"] == "z_scan";
        info.pixel_size = Some(voxel);
    }
    let name = name.unwrap_or(&recording.name);
    let mut draft = store.acquisition(project, name, &info)?;
    let record = draft.record();
    record.frames = frames
        .iter()
        .map(|f| FrameRecord {
            index: f.index,
            position: f.meta.position,
            timestamp: Some(f.meta.exposure_midpoint()),
            trigger: None,
        })
        .collect();
    record.camera = camera(&first.meta);
    record.objective = first
        .meta
        .objective
        .clone()
        .map_or(Value::Null, Value::String);
    // A scan's plan, which its recording's source carries, is the
    // acquisition's (ADR-0025).
    let mut source = recording.source.clone();
    let plan = source.as_object_mut().and_then(|s| s.remove("plan"));
    record.plan = plan.unwrap_or(Value::Null);
    record.instrument = json!({
        "recording": recording.id,
        "source": source,
        "lights": first.meta.lights,
    });
    let Some(image) = draft.image() else {
        unreachable!("an acquisition has an image");
    };
    let (total, done) = (frames.len() as u64, AtomicU64::new(0));
    let threads = std::thread::available_parallelism().map_or(AT_ONCE, |n| n.get().min(AT_ONCE));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| io(folder)(std::io::Error::other(e)))?;
    pool.install(|| {
        frames
            .par_iter()
            .enumerate()
            .try_for_each(|(plane, frame)| {
                let pixels = reader.pixels(frame)?;
                let size = (u64::from(frame.meta.width), u64::from(frame.meta.height));
                image.write(
                    plane as u32,
                    (0, 0),
                    size,
                    &Samples::from_le_bytes(sample, pixels),
                )?;
                match progress(done.fetch_add(1, Ordering::Relaxed) + 1, total) {
                    true => Ok(()),
                    false => Err(StoreError::Cancelled),
                }
            })
    })?;
    let acquisition = draft.commit()?;
    if !keep && let Err(error) = fs::remove_dir_all(folder) {
        tracing::warn!(
            "recording {} is ingested, but wasn't deleted: {error}",
            reader.recording().id
        );
    }
    Ok(acquisition)
}
