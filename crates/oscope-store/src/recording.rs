//! The scratch frame log (plan §5.5, ADR-0022): a recording's frames as the
//! camera gave them, on the scratch disk until they are ingested.
//!
//! ```text
//! {scratch root}/recordings/<recording>/recording.json, .lock,
//!     000000.frames, 000001.frames, …
//! ```
//!
//! A recording's frames go to segment files, each preallocated, then trimmed
//! when it closes. A segment holds, in little-endian numbers:
//!
//! - **a header** of 4,096 bytes: `OSCOPE.FRAMES`, the format's version, the
//!   segment's number and the recording's ID;
//! - **records,** each at a multiple of 4,096 bytes: `FRAM`, the lengths of
//!   the frame's metadata (JSON) and of its pixels, the frame's number in the
//!   recording, and a CRC-32 of the metadata and the pixels, which follow;
//! - **an index footer,** written when it closes: `OSCOPE.INDEX`, how many
//!   records, their offsets and a CRC-32 of all that, then the footer's own
//!   offset, in the file's last 8 bytes.
//!
//! A segment without its footer, which a crash cut short, is read by
//! scanning its records, as far as the first that doesn't hold together.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs4::FileExt as _;
use oscope_core::{FrameMeta, RecordingId, Versioned};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::{checked_name, read, write};
use crate::{Result, StoreError, io};

const MAGIC: &[u8; 16] = b"OSCOPE.FRAMES\0\0\0";
const INDEX: &[u8; 16] = b"OSCOPE.INDEX\0\0\0\0";
const RECORD: &[u8; 4] = b"FRAM";
/// The format's version.
const FORMAT: u32 = 1;
/// Records start at multiples of this, and the header is this long.
const ALIGN: u64 = 4096;
/// A record's header.
const RECORD_HEADER: u64 = 64;
/// The most a frame's metadata may take: more is a damaged header.
const META_MAX: u64 = 1 << 20;
static ZEROS: [u8; ALIGN as usize] = [0; ALIGN as usize];

/// A recording's document.
pub(crate) const DOCUMENT: &str = "recording.json";
/// The file a recording in progress keeps locked.
const LOCK: &str = ".lock";

/// A segment's size by default, preallocated: 4 GiB, about 7 s at the
/// design rate.
pub const SEGMENT_BYTES: u64 = 4 << 30;

/// A recording: frames on the scratch disk until they are ingested into a
/// project. Its document, `recording.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Recording {
    /// Its ID, which is also its folder's name.
    pub id: RecordingId,
    /// Its name.
    pub name: String,
    /// When it started.
    pub created: DateTime<Utc>,
    /// What it recorded, such as the camera and its mode.
    #[serde(default)]
    pub source: Value,
    /// When it ended: none while it records.
    #[serde(default)]
    pub ended: Option<DateTime<Utc>>,
    /// Why it ended early, if it did: a full disk, say.
    #[serde(default)]
    pub error: Option<String>,
    /// Whether a crash cut it short, and its segments were recovered.
    #[serde(default)]
    pub recovered: bool,
    /// The frames it holds.
    pub frames: u64,
    /// Their pixels, in bytes.
    pub bytes: u64,
    /// Frames the camera dropped between those it holds.
    pub dropped: u64,
    /// Frames deleted with the oldest segments, when rolling.
    pub discarded: u64,
}

impl Versioned for Recording {
    const SCHEMA: &'static str = "oscope.recording";
    const VERSION: u32 = 1;
}

/// How a recording is written.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingOptions {
    /// Its name.
    pub name: String,
    /// What it records.
    pub source: Value,
    /// A segment's size, preallocated: [`SEGMENT_BYTES`] by default, and at
    /// least 64 KiB.
    pub segment_bytes: u64,
    /// When rolling, the most its segments may take on disk: the oldest are
    /// deleted to keep within it. At least two segments. None keeps every
    /// frame.
    pub keep_bytes: Option<u64>,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            name: "recording".to_owned(),
            source: Value::Null,
            segment_bytes: SEGMENT_BYTES,
            keep_bytes: None,
        }
    }
}

/// `n` rounded up to a multiple of [`ALIGN`].
fn align(n: u64) -> u64 {
    n.div_ceil(ALIGN) * ALIGN
}

/// A footer's length, for `records` records.
fn footer_len(records: usize) -> u64 {
    16 + 8 + 8 * records as u64 + 4 + 4 + 8
}

/// A footer listing `offsets`, to be written at `at`.
fn footer(offsets: &[u64], at: u64) -> Vec<u8> {
    let mut footer = Vec::with_capacity(footer_len(offsets.len()) as usize);
    footer.extend_from_slice(INDEX);
    footer.extend_from_slice(&(offsets.len() as u64).to_le_bytes());
    for offset in offsets {
        footer.extend_from_slice(&offset.to_le_bytes());
    }
    let crc = crc32fast::hash(&footer);
    footer.extend_from_slice(&crc.to_le_bytes());
    footer.extend_from_slice(&[0; 4]);
    footer.extend_from_slice(&at.to_le_bytes());
    footer
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

fn damaged(path: &Path, reason: impl Into<String>) -> StoreError {
    StoreError::Document {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

/// Whether the recording in `folder` is recording: its lock is held, in
/// this process or another.
pub(crate) fn in_progress(folder: &Path) -> bool {
    File::open(folder.join(LOCK))
        .is_ok_and(|file| matches!(file.try_lock(), Err(TryLockError::WouldBlock)))
}

/// The segment files in `folder`, in order.
fn segments(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(folder).map_err(io(folder))? {
        let path = entry.map_err(io(folder))?.path();
        if path.extension().is_some_and(|e| e == "frames") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

struct Segment {
    file: File,
    path: PathBuf,
    /// Where the next record goes.
    end: u64,
    /// Its preallocated size.
    size: u64,
    offsets: Vec<u64>,
}

struct Closed {
    path: PathBuf,
    bytes: u64,
    frames: u64,
}

/// Writes `segment`'s footer after its last whole record, trims its
/// preallocation, and closes it, synchronised to the disk if `sync`.
fn close(segment: Segment, sync: bool) -> Result<Closed> {
    let Segment {
        mut file,
        path,
        end,
        offsets,
        ..
    } = segment;
    let footer = footer(&offsets, end);
    let bytes = end + footer.len() as u64;
    file.seek(SeekFrom::Start(end))
        .and_then(|_| file.write_all(&footer))
        .and_then(|()| file.set_len(bytes))
        .and_then(|()| if sync { file.sync_all() } else { Ok(()) })
        .map_err(io(&path))?;
    Ok(Closed {
        path,
        bytes,
        frames: offsets.len() as u64,
    })
}

/// Writes a recording's frames, in order, then [`finish`](Self::finish)es
/// it. One dropped unfinished is a recording a crash cut short, recovered
/// when the scratch space next opens.
pub struct RecordingWriter {
    folder: PathBuf,
    options: RecordingOptions,
    reserve: u64,
    recording: Recording,
    _lock: File,
    open: Option<Segment>,
    closed: VecDeque<Closed>,
    next: u32,
}

impl RecordingWriter {
    /// Starts a recording in a new folder in `recordings`, leaving at least
    /// `reserve` bytes free on its disk.
    pub(crate) fn create(
        recordings: &Path,
        options: RecordingOptions,
        reserve: u64,
    ) -> Result<Self> {
        if options.segment_bytes < 64 << 10 {
            return Err(StoreError::Refused(format!(
                "a segment of {} bytes: at least 64 KiB",
                options.segment_bytes
            )));
        }
        if options
            .keep_bytes
            .is_some_and(|keep| keep < 2 * options.segment_bytes)
        {
            return Err(StoreError::Refused(
                "a rolling recording keeps at least two segments".to_owned(),
            ));
        }
        let recording = Recording {
            id: RecordingId::generate(),
            name: checked_name(&options.name)?,
            created: Utc::now(),
            source: options.source.clone(),
            ended: None,
            error: None,
            recovered: false,
            frames: 0,
            bytes: 0,
            dropped: 0,
            discarded: 0,
        };
        let folder = recordings.join(recording.id.to_string());
        fs::create_dir_all(&folder).map_err(io(&folder))?;
        let path = folder.join(LOCK);
        let lock = File::create(&path).map_err(io(&path))?;
        lock.lock().map_err(io(&path))?;
        write(&folder.join(DOCUMENT), &recording)?;
        Ok(Self {
            folder,
            options,
            reserve,
            recording,
            _lock: lock,
            open: None,
            closed: VecDeque::new(),
            next: 0,
        })
    }

    /// The recording's ID.
    pub fn id(&self) -> RecordingId {
        self.recording.id
    }

    /// The recording, as written so far.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// Its folder.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Appends a frame: its metadata, and its pixels as its buffer holds
    /// them.
    pub fn append(&mut self, meta: &FrameMeta, pixels: &[u8]) -> Result<()> {
        let metadata = serde_json::to_vec(meta)
            .map_err(|e| StoreError::Refused(format!("a frame's metadata: {e}")))?;
        let length = RECORD_HEADER + metadata.len() as u64 + pixels.len() as u64;
        let size = align(length);
        let fits = self.open.as_ref().is_some_and(|segment| {
            segment.end + size + footer_len(segment.offsets.len() + 1) <= segment.size
        });
        if !fits {
            self.roll(size)?;
        }
        let index = self.recording.frames;
        let Some(segment) = self.open.as_mut() else {
            unreachable!("roll opens a segment");
        };
        let mut crc = crc32fast::Hasher::new();
        crc.update(&metadata);
        crc.update(pixels);
        let mut header = [0_u8; RECORD_HEADER as usize];
        header[..4].copy_from_slice(RECORD);
        header[4..8].copy_from_slice(&(metadata.len() as u32).to_le_bytes());
        header[8..16].copy_from_slice(&index.to_le_bytes());
        header[16..24].copy_from_slice(&(pixels.len() as u64).to_le_bytes());
        header[24..28].copy_from_slice(&crc.finalize().to_le_bytes());
        let file = &mut segment.file;
        file.write_all(&header)
            .and_then(|()| file.write_all(&metadata))
            .and_then(|()| file.write_all(pixels))
            .and_then(|()| file.write_all(&ZEROS[..(size - length) as usize]))
            .map_err(io(&segment.path))?;
        segment.offsets.push(segment.end);
        segment.end += size;
        let recording = &mut self.recording;
        if index > 0 {
            recording.dropped += meta.dropped_before;
        }
        recording.frames += 1;
        recording.bytes += pixels.len() as u64;
        Ok(())
    }

    /// Closes the open segment, if there is one, and opens the next, large
    /// enough for a record of `size` bytes.
    fn roll(&mut self, size: u64) -> Result<()> {
        if let Some(segment) = self.open.take() {
            let closed = close(segment, false)?;
            self.closed.push_back(closed);
        }
        let allocation = align(ALIGN + size + footer_len(1)).max(self.options.segment_bytes);
        self.discard(allocation)?;
        let available = fs4::available_space(&self.folder).map_err(io(&self.folder))?;
        if available < allocation + self.reserve {
            return Err(StoreError::Refused(format!(
                "the scratch disk has {} free: too little for a segment of {} and the reserve of {}",
                gb(available),
                gb(allocation),
                gb(self.reserve)
            )));
        }
        let number = self.next;
        let path = self.folder.join(format!("{number:06}.frames"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io(&path))?;
        let mut header = [0_u8; ALIGN as usize];
        header[..16].copy_from_slice(MAGIC);
        header[16..20].copy_from_slice(&FORMAT.to_le_bytes());
        header[20..24].copy_from_slice(&number.to_le_bytes());
        header[24..40].copy_from_slice(self.recording.id.0.as_bytes());
        file.allocate(allocation)
            .and_then(|()| file.write_all(&header))
            .map_err(io(&path))?;
        self.next += 1;
        self.open = Some(Segment {
            file,
            path,
            end: ALIGN,
            size: allocation,
            offsets: Vec::new(),
        });
        Ok(())
    }

    /// When rolling, deletes the oldest closed segments while they and a new
    /// one of `allocation` bytes would take more than allowed.
    fn discard(&mut self, allocation: u64) -> Result<()> {
        let Some(keep) = self.options.keep_bytes else {
            return Ok(());
        };
        while self.closed.iter().map(|c| c.bytes).sum::<u64>() + allocation > keep {
            let Some(oldest) = self.closed.pop_front() else {
                break;
            };
            fs::remove_file(&oldest.path).map_err(io(&oldest.path))?;
            self.recording.discarded += oldest.frames;
        }
        Ok(())
    }

    /// Closes the last segment, and records that the recording ended: early,
    /// for `error`'s reason, if there is one.
    pub fn finish(mut self, error: Option<String>) -> Result<Recording> {
        if let Some(segment) = self.open.take() {
            let closed = close(segment, true)?;
            self.closed.push_back(closed);
        }
        self.recording.ended = Some(Utc::now());
        self.recording.error = error;
        write(&self.folder.join(DOCUMENT), &self.recording)?;
        Ok(self.recording)
    }
}

/// A frame in a recording: its metadata, and where its pixels are.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameEntry {
    /// Its number in the recording, from 0.
    pub index: u64,
    /// Its metadata.
    pub meta: FrameMeta,
    /// Its pixels' length, in bytes.
    pub pixels_len: u64,
    segment: PathBuf,
    offset: u64,
    meta_len: u64,
    crc: u32,
}

/// A record's header, read.
struct Head {
    meta_len: u64,
    index: u64,
    pixels_len: u64,
    crc: u32,
}

fn read_at(file: &mut File, offset: u64, buffer: &mut [u8]) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(buffer)
}

/// The record header at `offset`, if one is there, and the record fits in
/// a file of `length` bytes.
fn read_head(file: &mut File, offset: u64, length: u64) -> Option<Head> {
    let mut bytes = [0_u8; RECORD_HEADER as usize];
    read_at(file, offset, &mut bytes).ok()?;
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or_default());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap_or_default());
    let head = Head {
        meta_len: u64::from(u32_at(4)),
        index: u64_at(8),
        pixels_len: u64_at(16),
        crc: u32_at(24),
    };
    let end = (offset + RECORD_HEADER + head.meta_len).checked_add(head.pixels_len)?;
    (&bytes[..4] == RECORD && head.meta_len <= META_MAX && end <= length).then_some(head)
}

/// A segment's records: each one's entry, and where the last ends.
struct Scan {
    entries: Vec<FrameEntry>,
    end: u64,
    footer: bool,
}

/// Reads the segment at `path` of recording `id`: by its footer, or, if it
/// has none, by scanning its records and checking each one's CRC-32.
fn read_segment(path: &Path, id: RecordingId) -> Result<Scan> {
    let mut file = File::open(path).map_err(io(path))?;
    let length = file.metadata().map_err(io(path))?.len();
    let mut header = [0_u8; 40];
    if read_at(&mut file, 0, &mut header).is_err() || &header[..16] != MAGIC {
        return Err(damaged(path, "not a segment of frames"));
    }
    let version = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
    if version != FORMAT {
        return Err(StoreError::Refused(format!(
            "{}: frames of format {version}, which this version of OScope can't read",
            path.display()
        )));
    }
    if header[24..40] != *id.0.as_bytes() {
        return Err(damaged(path, "a segment of another recording"));
    }
    let entry = |file: &mut File, head: Head, offset: u64| -> Option<FrameEntry> {
        let mut metadata = vec![0; head.meta_len as usize];
        read_at(file, offset + RECORD_HEADER, &mut metadata).ok()?;
        Some(FrameEntry {
            index: head.index,
            meta: serde_json::from_slice(&metadata).ok()?,
            pixels_len: head.pixels_len,
            segment: path.to_owned(),
            offset,
            meta_len: head.meta_len,
            crc: head.crc,
        })
    };
    if let Some((offsets, at)) = index(&mut file, length) {
        let mut entries = Vec::with_capacity(offsets.len());
        for offset in offsets {
            let found = read_head(&mut file, offset, length)
                .and_then(|head| entry(&mut file, head, offset));
            entries.push(found.ok_or_else(|| damaged(path, format!("no record at {offset}")))?);
        }
        return Ok(Scan {
            entries,
            end: at,
            footer: true,
        });
    }
    let (mut entries, mut offset) = (Vec::new(), ALIGN);
    while let Some(head) = read_head(&mut file, offset, length) {
        let end = offset + RECORD_HEADER + head.meta_len + head.pixels_len;
        let mut bytes = vec![0; (head.meta_len + head.pixels_len) as usize];
        if read_at(&mut file, offset + RECORD_HEADER, &mut bytes).is_err()
            || crc32fast::hash(&bytes) != head.crc
        {
            break;
        }
        let Some(found) = entry(&mut file, head, offset) else {
            break;
        };
        entries.push(found);
        offset = align(end);
    }
    Ok(Scan {
        entries,
        end: offset,
        footer: false,
    })
}

/// A segment's record offsets and the footer's own, from its footer, if it
/// has a whole one.
fn index(file: &mut File, length: u64) -> Option<(Vec<u64>, u64)> {
    let mut last = [0_u8; 8];
    read_at(file, length.checked_sub(8)?, &mut last).ok()?;
    let at = u64::from_le_bytes(last);
    let size = length.checked_sub(at)?;
    let most = footer_len((length / ALIGN) as usize);
    if at < ALIGN
        || size < footer_len(0)
        || size > most
        || !(size - footer_len(0)).is_multiple_of(8)
    {
        return None;
    }
    let mut bytes = vec![0; size as usize];
    read_at(file, at, &mut bytes).ok()?;
    let count = (size - footer_len(0)) / 8;
    let listed = 24 + 8 * count as usize;
    let stored = u32::from_le_bytes(bytes[listed..listed + 4].try_into().ok()?);
    let ok = &bytes[..16] == INDEX
        && u64::from_le_bytes(bytes[16..24].try_into().ok()?) == count
        && crc32fast::hash(&bytes[..listed]) == stored;
    let offsets = bytes[24..listed]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect();
    ok.then_some((offsets, at))
}

/// A recording, open to read: its document, and its frames in order.
pub struct RecordingReader {
    recording: Recording,
    frames: Vec<FrameEntry>,
}

impl RecordingReader {
    /// Opens the recording in `folder`: each segment by its footer, or, if it
    /// has none, by scanning its records.
    pub fn open(folder: &Path) -> Result<Self> {
        let recording: Recording = read(&folder.join(DOCUMENT))?;
        let mut frames = Vec::new();
        for path in segments(folder)? {
            frames.extend(read_segment(&path, recording.id)?.entries);
        }
        Ok(Self { recording, frames })
    }

    /// The recording's document.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// Its frames, in order.
    pub fn frames(&self) -> &[FrameEntry] {
        &self.frames
    }

    /// A frame's pixels, checked against its CRC-32.
    pub fn pixels(&self, frame: &FrameEntry) -> Result<Vec<u8>> {
        let path = &frame.segment;
        let mut file = File::open(path).map_err(io(path))?;
        let mut bytes = vec![0; (frame.meta_len + frame.pixels_len) as usize];
        read_at(&mut file, frame.offset + RECORD_HEADER, &mut bytes).map_err(io(path))?;
        if crc32fast::hash(&bytes) != frame.crc {
            return Err(damaged(
                path,
                format!("frame {} doesn't match its checksum", frame.index),
            ));
        }
        Ok(bytes.split_off(frame.meta_len as usize))
    }
}

/// Recovers the recording in `folder`, which a crash cut short: each segment
/// without its footer is given one after its last whole record and trimmed
/// there, a last segment with no header yet is deleted, and its document
/// counts what remains.
pub(crate) fn recover(folder: &Path) -> Result<Recording> {
    let mut recording: Recording = read(&folder.join(DOCUMENT))?;
    let paths = segments(folder)?;
    let (mut frames, mut bytes, mut dropped, mut first) = (0, 0, 0, None);
    for (k, path) in paths.iter().enumerate() {
        let scan = match read_segment(path, recording.id) {
            Ok(scan) => scan,
            Err(StoreError::Document { .. }) if k + 1 == paths.len() => {
                fs::remove_file(path).map_err(io(path))?;
                continue;
            }
            Err(error) => return Err(error),
        };
        if !scan.footer {
            let offsets: Vec<u64> = scan.entries.iter().map(|e| e.offset).collect();
            let segment = Segment {
                file: OpenOptions::new()
                    .write(true)
                    .open(path)
                    .map_err(io(path))?,
                path: path.clone(),
                end: scan.end,
                size: 0,
                offsets,
            };
            close(segment, true)?;
        }
        for entry in &scan.entries {
            first.get_or_insert(entry.index);
            if entry.index > 0 {
                dropped += entry.meta.dropped_before;
            }
            frames += 1;
            bytes += entry.pixels_len;
        }
    }
    recording.frames = frames;
    recording.bytes = bytes;
    recording.dropped = dropped;
    recording.discarded = first.unwrap_or(0);
    recording.ended = Some(Utc::now());
    recording.recovered = true;
    write(&folder.join(DOCUMENT), &recording)?;
    Ok(recording)
}
