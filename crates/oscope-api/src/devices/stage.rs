//! The `stage` Thing: where it is, and moving it (plan §5.4; P16).

use oscope_core::{Um, Xyz};
use oscope_hal::{Axis, Error, PositionSample, StageCapabilities, StageStatus, UmPerS};
use teta_wot::prelude::*;

use super::{Devices, Link, action_error, property_error};

/// The stage: its position and status, moves, jogs, homing and stopping.
/// A move whose invocation is cancelled stops the stage where it is, as
/// `halt` does.
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
    /// The stage arrived where a move or homing sent it.
    #[event]
    moved: Event<PositionSample>,
    /// The stage went into alarm: the controller's reason.
    #[event]
    alarm: Event<String>,
    link: Link,
}

impl StageThing {
    /// The speed to use: `speed` if given, else the saved one, else the
    /// stage's default.
    fn speed(&self, speed: Option<f64>) -> Option<UmPerS> {
        speed.or(self.speed.get()).map(UmPerS)
    }

    /// Runs a move on the stage, its turn at the gate first: emits `moved`
    /// when it arrives, and `alarm` if it ends in one. If the invocation is
    /// cancelled, the stage stops.
    async fn travel(
        &self,
        ctx: &ActionCtx,
        go: impl AsyncFnOnce(&Devices) -> Result<PositionSample, Error>,
    ) -> Result<PositionSample, ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.stage.lock().await;
        let outcome = tokio::select! {
            outcome = go(devices) => outcome,
            _ = ctx.cancelled() => {
                devices.stage.stop().await.map_err(action_error)?;
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
        let devices = self.link.action()?;
        let _turn = devices.gates.stage.lock().await;
        let speed = self.speed(speed);
        devices
            .stage
            .jog(axis, towards_max, speed)
            .await
            .map_err(action_error)
    }

    /// Stops the stage.
    ///
    /// A jog, or a move in progress. It doesn't wait its turn. (`stop` is
    /// the name of every Thing's own stopping, in `teta-wot`: ADR-0034.)
    #[action]
    async fn halt(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        devices.stage.stop().await.map_err(action_error)
    }

    /// Clears an alarm, once its cause is dealt with.
    #[action]
    async fn clear_alarm(&self) -> Result<(), ActionError> {
        let devices = self.link.action()?;
        let _turn = devices.gates.stage.lock().await;
        devices.stage.clear_alarm().await.map_err(action_error)
    }
}
