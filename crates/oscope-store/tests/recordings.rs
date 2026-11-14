//! The scratch frame log: frames round-tripped through segments, with their
//! metadata; recordings a crash cut short, recovered; rolling recordings;
//! damaged frames and other formats refused; and the scratch space's health
//! and clean-up.

use std::fs;
use std::path::{Path, PathBuf};

use oscope_core::{
    BayerPattern, FrameId, FrameMeta, Ms, PixelFormat, RecordingId, Timestamp, Um, Xyz,
};
use oscope_store::{
    Recording, RecordingOptions, RecordingReader, RecordingWriter, Scratch, ScratchPolicy,
    ScratchState, StoreError,
};
use serde_json::json;

/// A scratch space that never counts as low or full.
fn scratch(root: &Path) -> Scratch {
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    Scratch::open(root, policy).unwrap()
}

/// Segments of 64 KiB: two of the small frames each.
fn small(name: &str) -> RecordingOptions {
    RecordingOptions {
        name: name.to_owned(),
        source: json!({ "camera": "test" }),
        segment_bytes: 64 << 10,
        keep_bytes: None,
    }
}

/// Frame `n`: 100 × 80 grey 16-bit samples of its own, at a position of its
/// own, dropping `(n + 1) % 3` frames before it.
fn frame(n: u64) -> (FrameMeta, Vec<u8>) {
    let mut meta = FrameMeta::new(
        FrameId(n),
        PixelFormat::Mono16,
        100,
        80,
        Timestamp(1_000_000 * n),
    );
    meta.mode = "full".to_owned();
    meta.exposure = Ms(2.5);
    meta.position = Some(Xyz::new(Um(n as f64), Um(2.0 * n as f64), Um(500.0)));
    meta.dropped_before = (n + 1) % 3;
    let pixels = (0..16_000)
        .map(|i| ((i * 7 + n * 13) % 251) as u8)
        .collect();
    (meta, pixels)
}

/// The length of frame `n`'s metadata, in its record.
fn meta_len(n: u64) -> u64 {
    serde_json::to_vec(&frame(n).0).unwrap().len() as u64
}

fn write_frames(writer: &mut RecordingWriter, frames: std::ops::Range<u64>) {
    for n in frames {
        let (meta, pixels) = frame(n);
        writer.append(&meta, &pixels).unwrap();
    }
}

fn segments(folder: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "frames"))
        .collect();
    paths.sort();
    paths
}

/// Checks that `reader` holds frames `expected`, each as written.
fn check_frames(reader: &RecordingReader, expected: std::ops::Range<u64>) {
    let indices: Vec<u64> = reader.frames().iter().map(|f| f.index).collect();
    assert_eq!(indices, expected.clone().collect::<Vec<_>>());
    for entry in reader.frames() {
        let (meta, pixels) = frame(entry.index);
        assert_eq!(entry.meta, meta, "frame {}'s metadata", entry.index);
        assert_eq!(entry.pixels_len, pixels.len() as u64);
        assert_eq!(
            reader.pixels(entry).unwrap(),
            pixels,
            "frame {}",
            entry.index
        );
    }
}

#[test]
fn frames_round_trip_through_segments_with_their_metadata() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path());
    let mut writer = scratch.record(small("nine frames")).unwrap();
    let folder = writer.folder().to_owned();
    write_frames(&mut writer, 0..9);
    let recording = writer.finish(None).unwrap();

    assert_eq!(recording.name, "nine frames");
    assert_eq!(recording.source, json!({ "camera": "test" }));
    assert_eq!((recording.frames, recording.bytes), (9, 9 * 16_000));
    // The camera's drops between the frames recorded, not before the first.
    assert_eq!(recording.dropped, (1..9).map(|n| (n + 1) % 3).sum::<u64>());
    assert!(recording.ended.is_some() && recording.error.is_none() && !recording.recovered);

    let reader = RecordingReader::open(&folder).unwrap();
    assert_eq!(reader.recording(), &recording);
    check_frames(&reader, 0..9);
    // Two frames to a segment, each trimmed when it closed, with its footer.
    let files = segments(&folder);
    assert_eq!(files.len(), 5);
    for file in &files {
        assert!(fs::metadata(file).unwrap().len() < 64 << 10);
        assert_footer(file);
    }
}

/// Checks that the segment at `path` ends with its index footer.
fn assert_footer(path: &Path) {
    let bytes = fs::read(path).unwrap();
    let footer = u64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap()) as usize;
    assert_eq!(
        &bytes[footer..footer + 12],
        b"OSCOPE.INDEX",
        "{}",
        path.display()
    );
}

#[test]
fn a_recording_holds_the_michrome_20_s_frames() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path());
    let options = RecordingOptions {
        name: "full frames".to_owned(),
        ..RecordingOptions::default()
    };
    let mut writer = scratch.record(options).unwrap();
    let folder = writer.folder().to_owned();
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 12,
    };
    let (width, height) = (5472, 3648);
    let base: Vec<u8> = (0..format.frame_bytes(width, height))
        .map(|i| (i % 253) as u8)
        .collect();
    let pixels = |n: u8| -> Vec<u8> {
        let mut pixels = base.clone();
        pixels[..4096].fill(n);
        pixels
    };
    for n in 0..3_u8 {
        let meta = FrameMeta::new(FrameId(n.into()), format, width, height, Timestamp(0));
        writer.append(&meta, &pixels(n)).unwrap();
    }
    let recording = writer.finish(None).unwrap();
    assert_eq!(recording.bytes, 3 * 5472 * 3648 * 2);

    let reader = RecordingReader::open(&folder).unwrap();
    assert_eq!(reader.frames().len(), 3);
    for (n, entry) in reader.frames().iter().enumerate() {
        assert_eq!(reader.pixels(entry).unwrap(), pixels(n as u8));
    }
    // One segment, of the default 4 GiB, trimmed to what it holds.
    let files = segments(&folder);
    assert_eq!(files.len(), 1);
    assert!(fs::metadata(&files[0]).unwrap().len() < 121 << 20);
}

/// The offset of the last record in the segment at `path`.
fn last_record(path: &Path) -> u64 {
    let bytes = fs::read(path).unwrap();
    (4096..bytes.len())
        .step_by(4096)
        .rfind(|&at| &bytes[at..at + 4] == b"FRAM")
        .unwrap() as u64
}

/// How a crash left a recording's last segment.
#[derive(Clone, Copy, Debug)]
enum Crash {
    /// Its preallocated tail is still there, its last record whole.
    Whole,
    /// Its last record's pixels are cut short.
    InPixels,
    /// Its last record's metadata is cut short.
    InMetadata,
    /// Its last record's header is cut short.
    InHeader,
    /// Its last record's pixels are partly zeros, as when a power cut loses
    /// pages written out of order.
    Torn,
    /// A new segment was made, but its header never written.
    NewSegment,
    /// Its last record's header claims a terabyte of pixels.
    HugeLength,
}

#[test]
fn recordings_cut_short_are_recovered() {
    use Crash::*;
    for (crash, kept) in [
        (Whole, 5),
        (InPixels, 4),
        (InMetadata, 4),
        (InHeader, 4),
        (Torn, 4),
        (NewSegment, 5),
        (HugeLength, 4),
    ] {
        let root = tempfile::tempdir().unwrap();
        let scratch = scratch(root.path());
        let mut writer = scratch.record(small("cut short")).unwrap();
        let folder = writer.folder().to_owned();
        write_frames(&mut writer, 0..5);
        // The crash: no footer, no end, the lock let go.
        drop(writer);
        let files = segments(&folder);
        let last = files.last().unwrap();
        let (at, meta) = (last_record(last), meta_len(4));
        let file = fs::OpenOptions::new().write(true).open(last).unwrap();
        match crash {
            Whole => {}
            InPixels => file.set_len(at + 64 + meta + 8_000).unwrap(),
            InMetadata => file.set_len(at + 64 + meta / 2).unwrap(),
            InHeader => file.set_len(at + 30).unwrap(),
            Torn => {
                let mut bytes = fs::read(last).unwrap();
                let start = (at + 64 + meta + 1_000) as usize;
                bytes[start..start + 4096].fill(0);
                fs::write(last, bytes).unwrap();
            }
            NewSegment => fs::write(folder.join("000003.frames"), vec![0; 64 << 10]).unwrap(),
            HugeLength => {
                let mut bytes = fs::read(last).unwrap();
                let field = at as usize + 16;
                bytes[field..field + 8].copy_from_slice(&(1_u64 << 40).to_le_bytes());
                fs::write(last, bytes).unwrap();
            }
        }
        drop(file);

        // The scratch space recovers it when it next opens.
        let _again = Scratch::open(root.path(), ScratchPolicy::default()).unwrap();
        let reader = RecordingReader::open(&folder).unwrap();
        check_frames(&reader, 0..kept);
        let recording = reader.recording();
        assert!(
            recording.recovered && recording.ended.is_some(),
            "{crash:?}"
        );
        assert_eq!(recording.frames, kept, "{crash:?}");
        assert_eq!(recording.bytes, kept * 16_000, "{crash:?}");
        assert_eq!(
            recording.dropped,
            (1..kept).map(|n| (n + 1) % 3).sum::<u64>()
        );
        // Each segment has its footer, after its last whole record.
        assert_eq!(segments(&folder).len(), 3, "{crash:?}");
        for path in segments(&folder) {
            assert_footer(&path);
        }
    }
}

#[test]
fn a_rolling_recording_keeps_within_its_bytes() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path());
    let keep = 3 * (64 << 10);
    let options = RecordingOptions {
        keep_bytes: Some(keep),
        ..small("rolling")
    };
    let mut writer = scratch.record(options).unwrap();
    let folder = writer.folder().to_owned();
    write_frames(&mut writer, 0..20);
    let recording = writer.finish(None).unwrap();

    let on_disk: u64 = segments(&folder)
        .iter()
        .map(|p| fs::metadata(p).unwrap().len())
        .sum();
    assert!(on_disk <= keep, "{on_disk} bytes on disk");
    let reader = RecordingReader::open(&folder).unwrap();
    let kept = reader.frames().len() as u64;
    assert!(kept >= 4, "{kept} frames kept");
    check_frames(&reader, 20 - kept..20);
    assert_eq!(recording.discarded, 20 - kept);
    assert_eq!(recording.frames, 20);
}

#[test]
fn damaged_frames_other_formats_and_bad_options_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path());
    let mut writer = scratch.record(small("damaged")).unwrap();
    let folder = writer.folder().to_owned();
    write_frames(&mut writer, 0..2);
    writer.finish(None).unwrap();
    let path = &segments(&folder)[0];
    let mut bytes = fs::read(path).unwrap();
    let at = (last_record(path) + 64 + meta_len(1) + 900) as usize;
    bytes[at] ^= 1;
    fs::write(path, &bytes).unwrap();
    let reader = RecordingReader::open(&folder).unwrap();
    let error = reader.pixels(&reader.frames()[1]).unwrap_err().to_string();
    assert!(
        error.contains("frame 1 doesn't match its checksum"),
        "{error}"
    );
    assert!(reader.pixels(&reader.frames()[0]).is_ok());

    // A format this version can't read.
    bytes[16] = 2;
    fs::write(path, &bytes).unwrap();
    let error = RecordingReader::open(&folder).err().unwrap().to_string();
    assert!(error.contains("frames of format 2"), "{error}");

    let refused = |options: RecordingOptions| match scratch.record(options) {
        Err(StoreError::Refused(why)) => why,
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("accepted"),
    };
    let tiny = RecordingOptions {
        segment_bytes: 1 << 10,
        ..small("tiny")
    };
    assert!(refused(tiny).contains("at least 64 KiB"));
    let one = RecordingOptions {
        keep_bytes: Some(64 << 10),
        ..small("one segment")
    };
    assert!(refused(one).contains("at least two segments"));
    assert!(refused(small(" ")).contains("1 to 200 characters"));
}

#[test]
fn the_scratch_space_reports_its_health_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let scratch = scratch(root.path());
    fs::write(root.path().join("tmp").join("left"), b"x").unwrap();
    let health = scratch.health().unwrap();
    assert_eq!(health.state, ScratchState::Ok);
    assert_eq!((health.recordings, health.recordings_bytes), (0, 0));
    assert!(health.available_bytes > 0 && health.total_bytes >= health.available_bytes);

    // A recording in progress is neither recovered nor deleted.
    let mut writer = scratch.record(small("first")).unwrap();
    let first = writer.id();
    write_frames(&mut writer, 0..3);
    let again = Scratch::open(root.path(), ScratchPolicy::default()).unwrap();
    assert!(!root.path().join("tmp").join("left").exists());
    let listed: Vec<Recording> = again.recordings().unwrap();
    assert_eq!(listed[0].ended, None);
    let error = scratch.delete(first).unwrap_err().to_string();
    assert!(error.contains("still recording"), "{error}");
    writer.finish(None).unwrap();

    let mut writer = scratch.record(small("second")).unwrap();
    write_frames(&mut writer, 0..1);
    let second = writer.finish(None).unwrap();
    let names: Vec<String> = scratch
        .recordings()
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, ["second", "first"]);
    let health = scratch.health().unwrap();
    assert_eq!(health.recordings, 2);
    let folders = [
        scratch.folder(first).unwrap(),
        scratch.folder(second.id).unwrap(),
    ];
    let bytes: u64 = folders
        .iter()
        .flat_map(|f| fs::read_dir(f).unwrap())
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    assert_eq!(health.recordings_bytes, bytes);

    scratch.delete(first).unwrap();
    assert!(matches!(
        scratch.folder(first),
        Err(StoreError::NotFound {
            kind: "recording",
            ..
        })
    ));
    assert!(scratch.folder(RecordingId::generate()).is_err());

    // Low, then full: a recording is refused its first segment.
    let low = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: u64::MAX / 2,
    };
    let low = Scratch::open(root.path(), low).unwrap();
    assert_eq!(low.health().unwrap().state, ScratchState::Low);
    let full = ScratchPolicy {
        reserve_bytes: u64::MAX / 4,
        low_bytes: 0,
    };
    let full = Scratch::open(root.path(), full).unwrap();
    assert_eq!(full.health().unwrap().state, ScratchState::Full);
    let mut writer = full.record(small("no room")).unwrap();
    let (meta, pixels) = frame(0);
    let error = writer.append(&meta, &pixels).unwrap_err().to_string();
    assert!(error.contains("too little for a segment"), "{error}");
    let ended = writer.finish(Some(error.clone())).unwrap();
    assert_eq!((ended.frames, ended.error), (0, Some(error)));
}
