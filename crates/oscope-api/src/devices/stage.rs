//! The `stage` Thing: where it is, and moving it (plan §5.4; P16).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::time::Instant;

use oscope_core::{Um, Xyz};
use oscope_hal::{Axis, Error, PositionSample, StageCapabilities, StageStatus, UmPerS};
use teta_wot::prelude::*;

use super::{Devices, Link, action_error, property_error};

/// The stage: its position and status, moves, jogs, homing and stopping.
/// A move whose invocation is cancelled stops the stage where it is, as
/// `halt` does. While a hardware job runs, moves, jogs and homing are
/// refused as busy (409); `halt` and `clear_alarm` never are.
#[derive(Thing)]
#[thing(
    title = "Stage",
    semantic_type = "saref:Actuator",
    context(saref = "https://saref.etsi.org/core/")
)]
pub struct StageThing {
    /// The speed for moves and jogs that don't give one.
    ///
    /// Saved. None for the stage's own default.
    #[setting(unit = "µm/s")]
    speed: Prop<Option<f64>>,
    /// The stage came to rest after moving, and where.
    ///
    /// A move or homing arrived, or `halt` or a cancelled invocation stopped
    /// one, or a jog.
    #[event]
    moved: Event<PositionSample>,
    /// The stage went into alarm: the controller's reason.
    #[event]
    alarm: Event<String>,
    link: Link,
    /// Whether a jog was started since the last `halt`: a jog that reached
    /// a limit has stopped by itself, but still gets its `moved`.
    jogged: AtomicBool,
}

impl StageThing {
    /// The speed to use: `speed` if given, else the saved one, else the
    /// stage's default.
    fn speed(&self, speed: Option<f64>) -> Option<UmPerS> {
        speed.or(self.speed.get()).map(UmPerS)
    }

    /// After a stop: `moved`, where the stage came to rest, or `alarm` if
    /// the stop left it in one. The stage's latest report may be from just
    /// before it was still, so this waits for one from after, five report
    /// intervals at most.
    async fn rested(&self, devices: &Devices) {
        let stopped = devices.clock.now();
        match devices.stage.status().await {
            Ok(StageStatus::Alarm { reason }) => {
                self.alarm.emit(reason);
                return;
            }
            Ok(_) => {}
            Err(_) => return,
        }
        let interval = devices.stage.capabilities().report_interval.0.max(1.0) / 1000.0;
        let pause = Duration::from_secs_f64(interval / 2.0);
        let deadline = Instant::now()
            + Duration::from_secs_f64(5.0 * interval).max(Duration::from_millis(100));
        loop {
            let Ok(sample) = devices.stage.position().await else {
                return;
            };
            if sample.time >= stopped || Instant::now() >= deadline {
                self.moved.emit(sample);
                return;
            }
            tokio::time::sleep(pause).await;
        }
    }

    /// Runs a move on the stage, its turn at the gate first: emits `moved`
    /// when it arrives, and `alarm` if it ends in one. If the invocation is
    /// cancelled, the stage stops, and says where. A move a `halt` ends is
    /// `halt`'s to report.
    async fn travel(
        &self,
        ctx: &ActionCtx,
        go: impl AsyncFnOnce(&Devices) -> Result<PositionSample, Error>,
    ) -> Result<PositionSample, ActionError> {
        let devices = self.link.manual()?;
        let _turn = devices.gates.stage.lock().await;
        let outcome = tokio::select! {
            outcome = go(devices) => outcome,
            _ = ctx.cancelled() => {
                devices.stage.stop().await.map_err(action_error)?;
                self.rested(devices).await;
                return Err(ActionError::from(Cancelled));
            }
        };
        match outcome {
            Ok(sample) => {
                self.moved.emit(sample);
                Ok(sample)
            }
            Err(error) => {
                if let Error::Alarm { reason } = &error {
                    self.alarm.emit(reason.clone());
                }
                Err(action_error(error))
            }
        }
    }
}

#[thing_impl]
impl StageThing {
    #[on_start]
    async fn connect(&self, ctx: ThingCtx) -> anyhow::Result<()> {
        self.link.connect(&ctx)
    }

    /// What the stage is and offers.
    ///
    /// Its axes' travel, speeds, homing, encoders and trigger output.
    #[property]
    async fn capabilities(&self) -> Result<StageCapabilities, PropertyError> {
        Ok(self.link.property()?.stage.capabilities().clone())
    }

    /// Where it is, as last reported, and when.
    #[property(unit = "µm")]
    async fn position(&self) -> Result<PositionSample, PropertyError> {
        let devices = self.link.property()?;
        devices.stage.position().await.map_err(property_error)
    }

    /// What it is doing.
    #[property]
    async fn status(&self) -> Result<StageStatus, PropertyError> {
        let devices = self.link.property()?;
        devices.stage.status().await.map_err(property_error)
    }

    /// Whether it is moving or homing.
    #[property]
    async fn moving(&self) -> Result<bool, PropertyError> {
        let status = self.status().await?;
        Ok(matches!(status, StageStatus::Moving | StageStatus::Homing))
    }

    /// Moves to (`x`, `y`, `z`) µm, at `speed` µm/s or the saved speed.
    #[action]
    async fn move_to(
        &self,
        ctx: ActionCtx,
        x: f64,
        y: f64,
        z: f64,
        #[param(default)] speed: Option<f64>,
    ) -> Result<PositionSample, ActionError> {
        let speed = self.speed(speed);
        let target = Xyz::new(Um(x), Um(y), Um(z));
        self.travel(&ctx, async move |d: &Devices| {
            d.stage.move_to(target, speed).await
        })
        .await
    }

    /// Moves by (`dx`, `dy`, `dz`) µm.
    #[action]
    async fn move_by(
        &self,
        ctx: ActionCtx,
        dx: f64,
        dy: f64,
        dz: f64,
        #[param(default)] speed: Option<f64>,
    ) -> Result<PositionSample, ActionError> {
        let speed = self.speed(speed);
        let delta = Xyz::new(Um(dx), Um(dy), Um(dz));
        self.travel(&ctx, async move |d: &Devices| {
            d.stage.move_by(delta, speed).await
        })
        .await
    }

    /// Homes the stage, which needs it before moving.
    #[action]
    async fn home(&self, ctx: ActionCtx) -> Result<PositionSample, ActionError> {
        self.travel(&ctx, async |d: &Devices| d.stage.home().await)
            .await
    }

    /// Starts jogging along an axis.
    ///
    /// Along `axis`, towards its maximum or its minimum, until `halt`.
    #[action]
    async fn jog(
        &self,
        axis: Axis,
        towards_max: bool,
        #[param(default)] speed: Option<f64>,
    ) -> Result<(), ActionError> {
        let devices = self.link.manual()?;
        let _turn = devices.gates.stage.lock().await;
        let speed = self.speed(speed);
        devices
            .stage
            .jog(axis, towards_max, speed)
            .await
            .map_err(action_error)?;
        self.jogged.store(true, Ordering::Release);
        Ok(())
    }

    /// Stops the stage.
    ///
    /// A jog, or a move in progress. It doesn't wait its turn. If the stage
    /// was moving, `moved` then says where it came to rest. (`stop` is the
    /// name of every Thing's own stopping, in `teta-wot`: ADR-0034.)
    #[action]
    async fn halt(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        let status = devices.stage.status().await;
        let moving = status.is_ok_and(|s| s.is_moving() || matches!(s, StageStatus::Hold));
        let jogged = self.jogged.swap(false, Ordering::AcqRel);
        devices.stage.stop().await.map_err(action_error)?;
        if moving || jogged {
            self.rested(devices).await;
        }
        Ok(())
    }

    /// Clears an alarm, once its cause is dealt with.
    #[action]
    async fn clear_alarm(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.stage.lock().await;
        devices.stage.clear_alarm().await.map_err(action_error)
    }
}
