//! Previews for the desktop app's fast path (D21, ADR-0015): the newest
//! frames, made into previews as the `preview` Thing's defaults say, and
//! written to the shared-memory ring as ADR-0014's messages.

use std::sync::Arc;
use std::time::{Duration, Instant};

use oscope_core::Clock;
use oscope_shm::RingWriter;

use super::wire::Header;
use super::{Look, Preview, PreviewRequest};
use crate::LatestReceiver;

/// How the ring's previews are made, read before each one.
#[derive(Clone, Debug, PartialEq)]
pub struct RingSettings {
    /// The viewport, ROI and quality.
    pub request: PreviewRequest,
    /// How they look.
    pub look: Look,
    /// Whether to use the GPU, when there is one.
    pub gpu: bool,
    /// The most a second.
    pub max_fps: f64,
}

/// Writes previews of `frames`, the newest each time, to `ring`, until the
/// frames end. `settings` says how to make each; `clock` is the camera's,
/// for the headers' times. A message larger than the ring's slots is
/// skipped, and said once.
pub async fn publish(
    preview: Preview,
    mut frames: LatestReceiver,
    clock: Arc<dyn Clock>,
    mut ring: RingWriter,
    settings: impl Fn() -> RingSettings,
) {
    let (mut next, mut warned) = (Instant::now(), false);
    while let Some(mut frame) = frames.recv().await {
        if Instant::now() < next {
            tokio::time::sleep_until(next.into()).await;
            frame = frames.try_recv().unwrap_or(frame);
        }
        let RingSettings {
            request,
            look,
            gpu,
            max_fps,
        } = settings();
        next = Instant::now() + Duration::from_secs_f64(1.0 / max_fps.max(0.1));
        let quality = request.quality;
        let (maker, frame) = (preview.clone(), Arc::new(frame));
        let shown = Arc::clone(&frame);
        let made = tokio::task::spawn_blocking(move || maker.render(&shown, &request, &look, gpu));
        let sequence = (ring.written() + 1) as u32;
        let outcome = match made.await {
            Ok(Ok(rendered)) => {
                let header = Header::of(&frame, &rendered, quality, sequence, clock.now());
                ring.write(&header.message(&rendered.jpeg, &[]))
                    .map(drop)
                    .map_err(|e| e.to_string())
            }
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => return,
        };
        if let Err(error) = outcome
            && !std::mem::replace(&mut warned, true)
        {
            tracing::warn!("the preview ring {}: {error}", ring.name());
        }
    }
}
