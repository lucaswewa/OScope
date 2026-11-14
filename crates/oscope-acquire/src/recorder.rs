//! The recorder (plan §5.5, ADR-0022): every frame on the bus, written to a
//! recording on the scratch disk from a thread of its own.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use oscope_core::{Frame, RecordingId};
use oscope_store::{Recording, RecordingOptions, RecordingWriter, Scratch, StoreError};
use tokio::sync::{Notify, mpsc};

use crate::bus::{FrameBus, WhenFull};

/// The frames a recorder's queue holds by default: about a second at the
/// design rate, 640 MB of the frame pool's buffers.
pub const QUEUE: usize = 16;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a recorder has done so far.
#[derive(Clone, Debug, PartialEq)]
pub struct RecorderStats {
    /// Its recording.
    pub recording: RecordingId,
    /// The frames written.
    pub frames: u64,
    /// Their pixels, in bytes.
    pub bytes: u64,
    /// Frames the camera dropped between them.
    pub dropped: u64,
    /// Frames deleted with the oldest segments, when rolling.
    pub discarded: u64,
    /// Seconds since it started.
    pub seconds: f64,
    /// The most frames its queue held at once, the one being written among
    /// them.
    pub queued_max: usize,
    /// Why it stopped early, if it did.
    pub error: Option<String>,
}

#[derive(Default)]
struct Counts {
    frames: AtomicU64,
    bytes: AtomicU64,
    dropped: AtomicU64,
    discarded: AtomicU64,
    queued_max: AtomicUsize,
    error: Mutex<Option<String>>,
}

/// Writes every frame published on a bus to a recording, from a thread of
/// its own, until stopped.
///
/// It is a lossless subscriber: while its queue is full, the camera's thread
/// waits. A frame the camera loses meanwhile is counted in the next one's
/// `dropped_before`, which the recording adds up. A write that fails, on a
/// full disk say, ends the recording there, its frames kept, and the camera
/// waits no more.
pub struct Recorder {
    id: RecordingId,
    started: Instant,
    counts: Arc<Counts>,
    stop: Arc<Notify>,
    thread: Option<JoinHandle<Result<Recording, StoreError>>>,
}

impl Recorder {
    /// Starts recording the frames published on `bus` into `scratch`, with a
    /// queue of `queue` frames.
    pub fn start(
        bus: &FrameBus,
        scratch: &Scratch,
        options: RecordingOptions,
        queue: usize,
    ) -> Result<Self, StoreError> {
        let writer = scratch.record(options)?;
        let id = writer.id();
        let frames = bus.subscribe_lossless("recorder", queue, WhenFull::Wait);
        let (counts, stop) = (Arc::new(Counts::default()), Arc::new(Notify::new()));
        let (c, s) = (Arc::clone(&counts), Arc::clone(&stop));
        let thread = std::thread::Builder::new()
            .name("recorder".to_owned())
            .spawn(move || record(writer, frames, &c, &s))
            .map_err(|source| StoreError::Io {
                path: scratch.root().to_owned(),
                source,
            })?;
        Ok(Self {
            id,
            started: Instant::now(),
            counts,
            stop,
            thread: Some(thread),
        })
    }

    /// Its recording's ID.
    pub fn id(&self) -> RecordingId {
        self.id
    }

    /// What it has done so far.
    pub fn stats(&self) -> RecorderStats {
        let c = &self.counts;
        RecorderStats {
            recording: self.id,
            frames: c.frames.load(Ordering::Relaxed),
            bytes: c.bytes.load(Ordering::Relaxed),
            dropped: c.dropped.load(Ordering::Relaxed),
            discarded: c.discarded.load(Ordering::Relaxed),
            seconds: self.started.elapsed().as_secs_f64(),
            queued_max: c.queued_max.load(Ordering::Relaxed),
            error: lock(&c.error).clone(),
        }
    }

    /// Stops: the frames already queued are written, then the recording
    /// ends.
    pub fn stop(mut self) -> Result<Recording, StoreError> {
        self.stop.notify_one();
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(ended)) => ended,
            _ => Err(StoreError::Refused(
                "the recorder's thread failed".to_owned(),
            )),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stop.notify_one();
            let _ = thread.join();
        }
    }
}

/// The recorder's thread: writes frames until stopped, or until a write
/// fails, then ends the recording.
fn record(
    mut writer: RecordingWriter,
    mut frames: mpsc::Receiver<Frame>,
    counts: &Counts,
    stop: &Notify,
) -> Result<Recording, StoreError> {
    let written = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(runtime) => runtime.block_on(write(&mut writer, &mut frames, counts, stop)),
        Err(source) => Err(StoreError::Io {
            path: writer.folder().to_owned(),
            source,
        }),
    };
    drop(frames);
    let error = written.err().map(|e| e.to_string());
    lock(&counts.error).clone_from(&error);
    writer.finish(error)
}

async fn write(
    writer: &mut RecordingWriter,
    frames: &mut mpsc::Receiver<Frame>,
    counts: &Counts,
    stop: &Notify,
) -> Result<(), StoreError> {
    let mut stopping = false;
    loop {
        let frame = if stopping {
            frames.recv().await
        } else {
            tokio::select! {
                biased;
                () = stop.notified() => {
                    // No more frames come; those queued are written.
                    frames.close();
                    stopping = true;
                    continue;
                }
                frame = frames.recv() => frame,
            }
        };
        let Some(frame) = frame else {
            return Ok(());
        };
        counts
            .queued_max
            .fetch_max(frames.len() + 1, Ordering::Relaxed);
        writer.append(frame.meta(), frame.bytes())?;
        drop(frame);
        let recording = writer.recording();
        counts.frames.store(recording.frames, Ordering::Relaxed);
        counts.bytes.store(recording.bytes, Ordering::Relaxed);
        counts.dropped.store(recording.dropped, Ordering::Relaxed);
        counts
            .discarded
            .store(recording.discarded, Ordering::Relaxed);
    }
}
