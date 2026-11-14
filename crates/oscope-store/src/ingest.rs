//! Ingest (plan §5.5, ADR-0022): a recording's frames into an acquisition's
//! OME-Zarr store, compressed, several frames at once.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use oscope_core::{FrameMeta, PixelFormat, ProjectId};
use rayon::prelude::*;
use serde_json::{Value, json};

use crate::image::{ImageInfo, SampleType, Samples};
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

/// A frame's bytes, little-endian, as samples of `sample`.
fn samples(sample: SampleType, pixels: Vec<u8>) -> Samples {
    match sample {
        SampleType::U8 => Samples::U8(pixels),
        SampleType::U16 => Samples::U16(
            pixels
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .collect(),
        ),
        SampleType::F32 => Samples::F32(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect(),
        ),
    }
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

/// Writes the frames of the recording in `folder` into a new acquisition of
/// `project`, named `name`, or as the recording if none: a plane for each
/// frame, compressed, with its pyramid, and each frame's position and time
/// in its record. `progress` is told how many frames are written, of how
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
    let name = name.unwrap_or(&recording.name);
    let mut draft = store.acquisition(project, name, &info)?;
    let record = draft.record();
    record.frames = frames
        .iter()
        .map(|f| FrameRecord {
            index: f.index,
            position: f.meta.position,
            timestamp: Some(f.meta.exposure_midpoint()),
        })
        .collect();
    record.camera = camera(&first.meta);
    record.objective = first
        .meta
        .objective
        .clone()
        .map_or(Value::Null, Value::String);
    record.instrument = json!({
        "recording": recording.id,
        "source": recording.source,
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
                image.write(plane as u32, (0, 0), size, &samples(sample, pixels))?;
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
