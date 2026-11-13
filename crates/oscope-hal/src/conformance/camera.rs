//! The camera's conformance suite.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use oscope_core::{FrameBuffer, FrameMeta, FramePool, HeapAllocator, Ms, PixelFormat, PixelRect};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{Failure, ensure, expect, refused};
use crate::{Camera, CameraCapabilities, CameraSettings, Edge, Error, FrameSink, Shutter, Trigger};

/// How long to wait for a frame beyond its interval and exposure.
const ALLOWANCE: Duration = Duration::from_secs(10);

/// Frames from the camera, through a sink into a channel.
struct Tap {
    pool: FramePool,
    frames: UnboundedReceiver<(FrameBuffer, FrameMeta)>,
    /// Set once `stop` has returned; the sink then marks any later call.
    stopped: Arc<AtomicBool>,
    late: Arc<AtomicBool>,
    patience: Duration,
    /// A frame's interval, plus its exposure.
    period: Duration,
}

impl Tap {
    /// Starts `camera` streaming, as set to `settings`, into a pool of
    /// `slabs` slabs.
    async fn start(
        camera: &dyn Camera,
        settings: &CameraSettings,
        slabs: usize,
    ) -> Result<Tap, Failure> {
        let caps = camera.capabilities();
        let (_, width, height) = expect(caps.frame_shape(settings), "start streaming")?;
        let pool = FramePool::new(
            settings.format.frame_bytes(width, height),
            slabs,
            &HeapAllocator,
        );
        let (sender, frames) = unbounded_channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let late = Arc::new(AtomicBool::new(false));
        let sink: FrameSink = {
            let (stopped, late) = (Arc::clone(&stopped), Arc::clone(&late));
            Arc::new(move |buffer, meta| {
                if stopped.load(Ordering::Acquire) {
                    late.store(true, Ordering::Release);
                }
                let _ = sender.send((buffer, meta));
            })
        };
        expect(camera.start(pool.clone(), sink).await, "start streaming")?;
        ensure(camera.streaming(), "start streaming", || {
            "it doesn't say it streams".into()
        })?;
        let fps = caps
            .mode(&settings.mode)
            .ok()
            .and_then(|m| m.fps(settings.format));
        let interval = Duration::from_secs_f64(1.0 / fps.unwrap_or(1.0).max(0.001));
        let exposure = settings.exposure.to_duration().unwrap_or_default();
        Ok(Tap {
            pool,
            frames,
            stopped,
            late,
            patience: ALLOWANCE + 2 * (interval + exposure),
            period: interval + exposure,
        })
    }

    async fn next(&mut self, check: &'static str) -> Result<(FrameBuffer, FrameMeta), Failure> {
        let patience = self.patience;
        match tokio::time::timeout(patience, self.frames.recv()).await {
            Ok(Some(frame)) => Ok(frame),
            Ok(None) => Err(Failure {
                check,
                detail: "the camera dropped its sink".into(),
            }),
            Err(_) => Err(Failure {
                check,
                detail: format!("no frame within {patience:?}"),
            }),
        }
    }

    /// Stops the camera, and gives the flag that marks a later sink call.
    async fn stop(self, camera: &dyn Camera) -> Result<Arc<AtomicBool>, Failure> {
        expect(camera.stop().await, "stop streaming")?;
        self.stopped.store(true, Ordering::Release);
        ensure(!camera.streaming(), "stop streaming", || {
            "it still says it streams".into()
        })?;
        Ok(self.late)
    }
}

/// Settings in `mode` and `format`: a 1 ms exposure, or the nearest the
/// camera allows; the least gain; no ROI; free-running.
fn plain(caps: &CameraCapabilities, mode: &str, format: PixelFormat) -> CameraSettings {
    CameraSettings {
        mode: mode.to_owned(),
        format,
        exposure: Ms(1.0f64.clamp(caps.exposure_min.0, caps.exposure_max.0)),
        gain: caps.gain_min,
        roi: None,
        trigger: Trigger::FreeRunning,
    }
}

/// Checks the camera's capabilities make sense.
fn capabilities(caps: &CameraCapabilities) -> Result<(), Failure> {
    let check = "capabilities that make sense";
    ensure(!caps.modes.is_empty(), check, || "no modes".into())?;
    let names: HashSet<_> = caps.modes.iter().map(|m| &m.name).collect();
    ensure(names.len() == caps.modes.len(), check, || {
        "two modes share a name".into()
    })?;
    for m in &caps.modes {
        let fits = m.binning >= 1
            && m.width > 0
            && m.height > 0
            && m.width * m.binning <= caps.sensor_width
            && m.height * m.binning <= caps.sensor_height;
        let rates = !m.rates.is_empty() && m.rates.iter().all(|r| r.fps > 0.0 && r.fps.is_finite());
        ensure(fits && rates, check, || format!("mode {:?}: {m:?}", m.name))?;
        ensure(
            m.line_time.is_some() == (caps.shutter == Shutter::Rolling),
            check,
            || format!("mode {:?}'s line time doesn't match the shutter", m.name),
        )?;
    }
    let ranges = caps.pixel_size.0 > 0.0
        && caps.exposure_min.0 > 0.0
        && caps.exposure_min <= caps.exposure_max
        && caps.gain_min <= caps.gain_max;
    ensure(ranges, check, || "the pixel, exposure or gain".into())?;
    let roi = caps
        .roi
        .is_none_or(|r| r.x_step >= 1 && r.y_step >= 1 && r.min_size >= 1);
    ensure(roi, check, || format!("the ROI rules {:?}", caps.roi))
}

/// Checks a streamed frame against its settings, and the frame before it.
fn frame(
    caps: &CameraCapabilities,
    settings: &CameraSettings,
    (buffer, meta): &(FrameBuffer, FrameMeta),
    before: Option<&FrameMeta>,
) -> Result<(), Failure> {
    let check = "frames that match their settings";
    let (roi, width, height) = expect(caps.frame_shape(settings), check)?;
    let mode = expect(caps.mode(&settings.mode), check)?;
    let shape = meta.format == settings.format
        && (meta.width, meta.height, meta.roi) == (width, height, roi)
        && meta.mode == settings.mode
        && meta.binning == mode.binning;
    ensure(shape, check, || format!("{meta:?} for {settings:?}"))?;
    ensure(
        buffer.bytes().len() >= meta.format.frame_bytes(width, height),
        check,
        || "a slab too small for the frame".into(),
    )?;
    let exposure = (meta.exposure.0 - settings.exposure.0).abs() <= settings.exposure.0 * 0.01;
    let times = meta.exposure_start <= meta.host_time
        && meta.line_time.is_some() == (caps.shutter == Shutter::Rolling)
        && meta.sensor_time.is_some() == caps.sensor_timestamps;
    ensure(exposure && times && meta.dropped_before == 0, check, || {
        format!("its exposure, times or drops: {meta:?}")
    })?;
    if let Some(before) = before {
        let order = meta.id > before.id && meta.host_time >= before.host_time;
        ensure(order, check, || {
            format!(
                "{} after {}, at {} after {}",
                meta.id, before.id, meta.host_time, before.host_time
            )
        })?;
    }
    Ok(())
}

/// Checks what the camera must refuse, and that a refusal changes nothing.
async fn refusals(camera: &dyn Camera, base: &CameraSettings) -> Result<(), Failure> {
    let caps = camera.capabilities();
    let before = expect(camera.settings().await, "read the settings")?;
    let with = |change: &dyn Fn(&mut CameraSettings)| {
        let mut s = base.clone();
        change(&mut s);
        s
    };
    let unknown = with(&|s| s.mode = "no such mode".into());
    refused(
        camera.configure(unknown).await,
        "refuse an unknown mode",
        |e| matches!(e, Error::Unknown { .. }),
    )?;
    let long = with(&|s| s.exposure = Ms(caps.exposure_max.0 * 2.0 + 1.0));
    refused(
        camera.configure(long).await,
        "refuse an exposure over the longest",
        |e| matches!(e, Error::OutOfRange { .. }),
    )?;
    let gain = with(&|s| s.gain = caps.gain_max + 1.0);
    refused(
        camera.configure(gain).await,
        "refuse a gain over the most",
        |e| matches!(e, Error::OutOfRange { .. }),
    )?;
    let unsupported = |e: &Error| matches!(e, Error::Unsupported { .. });
    if !caps.trigger.external {
        let external = with(&|s| s.trigger = Trigger::External { edge: Edge::Rising });
        refused(
            camera.configure(external).await,
            "refuse an external trigger",
            unsupported,
        )?;
    }
    if !caps.trigger.software {
        let software = with(&|s| s.trigger = Trigger::Software);
        refused(
            camera.configure(software).await,
            "refuse a software trigger",
            unsupported,
        )?;
    }
    let (w, h) = (caps.sensor_width, caps.sensor_height);
    let outside = with(&|s| s.roi = Some(PixelRect::new(w / 2, h / 2, w, h)));
    let check = if caps.roi.is_some() {
        "refuse an ROI outside the sensor"
    } else {
        "refuse an ROI"
    };
    refused(camera.configure(outside).await, check, unsupported)?;
    let after = expect(camera.settings().await, "read the settings")?;
    ensure(after == before, "change nothing when refusing", || {
        format!("{before:?} became {after:?}")
    })
}

/// Checks that `camera` does what the [`Camera`] trait promises.
pub async fn camera(camera: &dyn Camera) -> Result<(), Failure> {
    let caps = camera.capabilities();
    capabilities(caps)?;
    ensure(!camera.streaming(), "not streaming at first", || {
        "it says it streams".into()
    })?;
    let original = expect(camera.settings().await, "read the settings")?;
    ensure(
        caps.check(&original).is_ok(),
        "settings within its capabilities",
        || format!("{original:?}"),
    )?;

    let check = "configure every mode and format";
    for mode in &caps.modes {
        for rate in &mode.rates {
            let asked = plain(caps, &mode.name, rate.format);
            let applied = expect(camera.configure(asked.clone()).await, check)?;
            let read = expect(camera.settings().await, check)?;
            let expected = expect(caps.check(&asked), check)?;
            ensure(applied == expected && read == applied, check, || {
                format!("asked {asked:?}, applied {applied:?}, read {read:?}")
            })?;
        }
    }
    let base = plain(caps, &caps.modes[0].name, caps.modes[0].rates[0].format);
    expect(camera.configure(base.clone()).await, check)?;
    refusals(camera, &base).await?;
    let mut lates = Vec::new();

    // Streaming, and what can't change while it streams.
    let mut tap = Tap::start(camera, &base, 8).await?;
    let mut before: Option<FrameMeta> = None;
    for _ in 0..4 {
        let delivered = tap.next("stream frames").await?;
        frame(caps, &base, &delivered, before.as_ref())?;
        before = Some(delivered.1);
    }
    let idle: FrameSink = Arc::new(|_, _| {});
    let spare = FramePool::new(1, 1, &HeapAllocator);
    refused(
        camera.start(spare, idle).await,
        "refuse to start twice",
        |e| matches!(e, Error::Busy { .. }),
    )?;
    let mut others = caps
        .modes
        .iter()
        .flat_map(|m| m.rates.iter().map(move |r| (&m.name, r.format)));
    if let Some((name, format)) = others.find(|(n, f)| (*n, *f) != (&base.mode, base.format)) {
        let changed = plain(caps, name, format);
        refused(
            camera.configure(changed).await,
            "refuse a new frame while streaming",
            |e| matches!(e, Error::Busy { .. }),
        )?;
    }
    lates.push(tap.stop(camera).await?);

    // An ROI, as the camera rounds it.
    if caps.roi.is_some() {
        let check = "apply an ROI as rounded";
        let (w, h) = (caps.sensor_width, caps.sensor_height);
        let asked = CameraSettings {
            roi: Some(PixelRect::new(w / 4 + 1, h / 4 + 1, w / 2 + 1, h / 2 + 1)),
            ..base.clone()
        };
        let applied = expect(camera.configure(asked.clone()).await, check)?;
        let rounded = asked.roi.and_then(|r| caps.round_roi(r));
        ensure(applied.roi == rounded, check, || {
            format!(
                "{:?} applied as {:?}, not {rounded:?}",
                asked.roi, applied.roi
            )
        })?;
        let mut tap = Tap::start(camera, &applied, 4).await?;
        let delivered = tap.next(check).await?;
        frame(caps, &applied, &delivered, None)?;
        lates.push(tap.stop(camera).await?);
        expect(camera.configure(base.clone()).await, check)?;
    }

    // Frames lost while the pool is full are counted.
    let check = "count the frames lost to a full pool";
    let mut tap = Tap::start(camera, &base, 1).await?;
    let held = tap.next(check).await?;
    let deadline = tokio::time::Instant::now() + tap.patience;
    while tap.pool.stats().exhausted == 0 {
        ensure(tokio::time::Instant::now() < deadline, check, || {
            "it never asked for another slab".into()
        })?;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    drop(held);
    let (_, meta) = tap.next(check).await?;
    ensure(meta.dropped_before >= 1, check, || {
        format!(
            "the frame after a full pool says {} lost",
            meta.dropped_before
        )
    })?;
    lates.push(tap.stop(camera).await?);

    // One frame for each software trigger.
    let check = "one frame for each software trigger";
    if caps.trigger.software {
        let triggered = CameraSettings {
            trigger: Trigger::Software,
            ..base.clone()
        };
        expect(camera.configure(triggered.clone()).await, check)?;
        let mut tap = Tap::start(camera, &triggered, 8).await?;
        for _ in 0..3 {
            expect(camera.trigger().await, check)?;
            tap.next(check).await?;
        }
        // Proving that nothing comes takes a wait: a camera that ignored
        // the trigger would deliver within three frame periods.
        tokio::time::sleep(3 * tap.period).await;
        let extra = tap.frames.try_recv().is_ok();
        ensure(!extra, check, || "a frame came without a trigger".into())?;
        let late = Arc::clone(&tap.late);
        let stopped = Arc::clone(&tap.stopped);
        expect(camera.stop().await, check)?;
        stopped.store(true, Ordering::Release);
        lates.push(late);
        expect(camera.configure(base.clone()).await, check)?;
    } else {
        refused(camera.trigger().await, check, |e| {
            matches!(e, Error::Unsupported { .. })
        })?;
    }

    let check = "a temperature, if it has one";
    let reading = camera.temperature().await;
    if caps.temperature {
        let celsius = expect(reading, check)?;
        ensure(celsius.is_finite(), check, || format!("{celsius} °C"))?;
    } else {
        refused(reading, check, |e| matches!(e, Error::Unsupported { .. }))?;
    }

    let late = lates.iter().any(|l| l.load(Ordering::Acquire));
    ensure(!late, "no frames after stopping", || {
        "the sink was called after stop returned".into()
    })?;
    expect(camera.configure(original).await, "put the settings back")?;
    Ok(())
}
