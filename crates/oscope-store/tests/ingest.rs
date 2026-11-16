//! Ingest: a recording's frames into an acquisition equal to them, with
//! each frame's position and time; and what ingest refuses, and how it
//! stops.

use std::path::Path;
use std::sync::Mutex;

use oscope_core::{
    BayerPattern, FrameId, FrameMeta, Ms, PixelFormat, ProjectId, Timestamp, Um, Xyz,
};
use oscope_store::{
    ImageReader, NewProject, RecordingOptions, Samples, Scratch, ScratchPolicy, Store, StoreError,
    ingest,
};
use serde_json::{Map, json};

const RAW: PixelFormat = PixelFormat::Bayer16 {
    pattern: BayerPattern::Rggb,
    significant_bits: 12,
};

fn open(root: &Path) -> (Store, Scratch, ProjectId) {
    let data = root.join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    let scratch = Scratch::open(&root.join("scratch"), policy).unwrap();
    let project = NewProject {
        name: "slide 7".to_owned(),
        sample: "H&E".to_owned(),
        tags: Vec::new(),
        owner: "lab".to_owned(),
        metadata: Map::new(),
    };
    let project = store.create(project).unwrap().id;
    (store, scratch, project)
}

/// Frame `n` of `width` × `height` in `format`: samples of its own, at a
/// position and time of its own.
fn frame(n: u64, format: PixelFormat, width: u32, height: u32) -> (FrameMeta, Vec<u8>) {
    let mut meta = FrameMeta::new(FrameId(n), format, width, height, Timestamp(5_000_000 * n));
    meta.mode = "full".to_owned();
    meta.exposure = Ms(4.0);
    meta.objective = Some("20x".to_owned());
    meta.position = Some(Xyz::new(Um(100.0 * n as f64), Um(50.0), Um(1_000.0)));
    let pixels = (0..format.frame_bytes(width, height) as u64)
        .map(|i| ((i * 31 + n * 17) % 241) as u8)
        .collect();
    (meta, pixels)
}

/// A recording's options: segments of 256 MiB, room for six full frames.
/// The default's 4 GiB, each preallocated, would crowd the disk when suites
/// run side by side.
fn options() -> RecordingOptions {
    RecordingOptions {
        name: "a recording".to_owned(),
        segment_bytes: 256 << 20,
        ..RecordingOptions::default()
    }
}

/// Records `frames` frames, as [`frame`] makes them, and returns the
/// recording's folder.
fn record(
    scratch: &Scratch,
    frames: u64,
    format: PixelFormat,
    size: (u32, u32),
) -> std::path::PathBuf {
    let options = options();
    let mut writer = scratch.record(options).unwrap();
    for n in 0..frames {
        let (meta, pixels) = frame(n, format, size.0, size.1);
        writer.append(&meta, &pixels).unwrap();
    }
    let folder = writer.folder().to_owned();
    writer.finish(None).unwrap();
    folder
}

fn as_u16(pixels: &[u8]) -> Vec<u16> {
    pixels
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect()
}

/// Ingests the recording in `folder`, counting its progress.
fn ingest_all(
    store: &Store,
    project: ProjectId,
    folder: &Path,
    keep: bool,
) -> (oscope_store::Acquisition, Vec<(u64, u64)>) {
    let calls = Mutex::new(Vec::new());
    let progress = |done, total| {
        calls.lock().unwrap().push((done, total));
        true
    };
    let acquisition = ingest(store, project, folder, None, keep, &progress).unwrap();
    let mut calls = calls.into_inner().unwrap();
    calls.sort_unstable();
    (acquisition, calls)
}

#[test]
fn ingested_data_equals_the_recorded_frames() {
    let root = tempfile::tempdir().unwrap();
    let (store, scratch, project) = open(root.path());
    let (width, height) = (1301, 1029);
    let folder = record(&scratch, 6, RAW, (width, height));
    let (acquisition, calls) = ingest_all(&store, project, &folder, false);

    assert_eq!(calls, (1..=6).map(|done| (done, 6)).collect::<Vec<_>>());
    assert_eq!(acquisition.name, "a recording");
    let image = &acquisition.image;
    assert_eq!((image.width, image.height, image.planes), (1301, 1029, 6));
    assert_eq!((image.channels, image.levels), (1, 3));
    assert_eq!(image.channel_names, ["raw"]);
    assert_eq!(acquisition.camera["mode"], "full");
    assert_eq!(acquisition.camera["format"], json!(RAW));
    assert_eq!(acquisition.objective, "20x");
    for (n, record) in acquisition.frames.iter().enumerate() {
        let (meta, _) = frame(n as u64, RAW, width, height);
        assert_eq!(record.index, n as u64);
        assert_eq!(record.position, meta.position);
        assert_eq!(record.timestamp, Some(meta.exposure_midpoint()));
    }
    // Every plane, as recorded, and the recording gone.
    let store_path = store
        .folder(project)
        .join("acquisitions")
        .join(acquisition.id.to_string())
        .join("raw.ome.zarr");
    let reader = ImageReader::open(&store_path).unwrap();
    for n in 0..6 {
        let (_, pixels) = frame(n, RAW, width, height);
        let read = reader.read(0, n as u32, (0, 0), (1301, 1029)).unwrap();
        assert_eq!(read, Samples::U16(as_u16(&pixels)), "plane {n}");
    }
    assert!(!folder.exists());
    let contents = store.contents(project).unwrap();
    assert_eq!(contents.acquisitions.len(), 1);
}

#[test]
fn a_recording_of_full_michrome_20_frames_is_ingested() {
    let root = tempfile::tempdir().unwrap();
    let (store, scratch, project) = open(root.path());
    let folder = record(&scratch, 2, RAW, (5472, 3648));
    let (acquisition, _) = ingest_all(&store, project, &folder, true);
    assert_eq!(acquisition.image.levels, 5);
    let store_path = store
        .folder(project)
        .join("acquisitions")
        .join(acquisition.id.to_string())
        .join("raw.ome.zarr");
    let reader = ImageReader::open(&store_path).unwrap();
    let (_, pixels) = frame(1, RAW, 5472, 3648);
    let read = reader.read(0, 1, (0, 0), (5472, 3648)).unwrap();
    assert_eq!(read, Samples::U16(as_u16(&pixels)));
    // Kept, as asked.
    assert!(folder.exists());
}

#[test]
fn ingest_takes_each_format_s_channels() {
    let root = tempfile::tempdir().unwrap();
    let (store, scratch, project) = open(root.path());
    for (format, names) in [
        (PixelFormat::Mono8, vec!["grey"]),
        (PixelFormat::Rgb8, vec!["red", "green", "blue"]),
        (PixelFormat::Rgba8, vec!["red", "green", "blue", "alpha"]),
    ] {
        let folder = record(&scratch, 2, format, (64, 48));
        let (acquisition, _) = ingest_all(&store, project, &folder, false);
        assert_eq!(acquisition.image.channel_names, names);
        let store_path = store
            .folder(project)
            .join("acquisitions")
            .join(acquisition.id.to_string())
            .join("raw.ome.zarr");
        let read = ImageReader::open(&store_path)
            .unwrap()
            .read(0, 1, (0, 0), (64, 48))
            .unwrap();
        assert_eq!(read, Samples::U8(frame(1, format, 64, 48).1));
    }
}

#[test]
fn ingest_refuses_what_it_cannot_take_and_stops_when_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let (store, scratch, project) = open(root.path());
    let refused = |folder: &Path| -> String {
        match ingest(&store, project, folder, None, false, &|_, _| true) {
            Err(StoreError::Refused(why)) => why,
            other => panic!("{other:?}"),
        }
    };

    // Still recording.
    let mut writer = scratch.record(options()).unwrap();
    let (meta, pixels) = frame(0, RAW, 64, 48);
    writer.append(&meta, &pixels).unwrap();
    assert!(refused(writer.folder()).contains("hasn't ended"));
    writer.finish(None).unwrap();

    // No frames.
    let empty = record(&scratch, 0, RAW, (64, 48));
    assert!(refused(&empty).contains("has no frames"));

    // Frames of two sizes.
    let mut writer = scratch.record(options()).unwrap();
    for (n, size) in [(0, (64, 48)), (1, (32, 24))] {
        let (meta, pixels) = frame(n, RAW, size.0, size.1);
        writer.append(&meta, &pixels).unwrap();
    }
    let mixed = writer.folder().to_owned();
    writer.finish(None).unwrap();
    let why = refused(&mixed);
    assert!(
        why.contains("frame 1 is 32 × 24") && why.contains("one size and format"),
        "{why}"
    );

    // Planar floats.
    let planar = record(&scratch, 1, PixelFormat::PlanarF32 { planes: 1 }, (8, 8));
    assert!(refused(&planar).contains("planar floats"));

    // Cancelled after the second frame: no acquisition, the recording kept.
    let folder = record(&scratch, 6, RAW, (64, 48));
    let cancelled = ingest(
        &store,
        project,
        &folder,
        Some("stopped"),
        false,
        &|done, _| done < 2,
    );
    assert!(matches!(cancelled, Err(StoreError::Cancelled)));
    assert!(store.contents(project).unwrap().acquisitions.is_empty());
    assert!(folder.exists());
}
