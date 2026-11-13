//! The fake stage.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use oscope_core::{Clock, Ms, Um, Xyz};

use super::Script;
use crate::{
    Axis, Error, HalFuture, PositionSample, PositionTrack, Result, Stage, StageCapabilities,
    StageStatus, Travel, UmPerS,
};

/// A stage that reaches each target at once, and reports it on its track
/// at the clock's time.
///
/// - **A jog** leaves it moving where it is, until a stop.
/// - **Homing** takes it to the least position of each axis, and clears an
///   alarm, as grblHAL's homing does.
/// - **[`FakeStage::raise_alarm`]** puts it in alarm, as a limit switch
///   would. It then refuses to move until the alarm is cleared or it is
///   homed.
pub struct FakeStage {
    capabilities: StageCapabilities,
    clock: Arc<dyn Clock>,
    script: Script,
    track: Arc<PositionTrack>,
    status: Mutex<StageStatus>,
    armed: Mutex<Option<(Axis, Vec<Um>)>>,
}

impl FakeStage {
    /// A fake with [`FakeStage::default_capabilities`].
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_capabilities(Self::default_capabilities(), clock)
    }

    /// 100 × 75 × 20 mm of travel from 0; 10 mm/s by default, 50 at most;
    /// homing and a trigger output, no encoders; a report every 10 ms.
    pub fn default_capabilities() -> StageCapabilities {
        let travel = |mm: f64| Travel {
            min: Um(0.0),
            max: Um::from_mm(mm),
        };
        StageCapabilities {
            model: "fake stage".to_owned(),
            x: travel(100.0),
            y: travel(75.0),
            z: travel(20.0),
            max_speed: UmPerS(50_000.0),
            default_speed: UmPerS(10_000.0),
            homing: true,
            encoders: false,
            trigger_output: true,
            report_interval: Ms(10.0),
        }
    }

    /// A fake with these capabilities, idle at the least position of each
    /// axis.
    pub fn with_capabilities(capabilities: StageCapabilities, clock: Arc<dyn Clock>) -> Self {
        let stage = Self {
            capabilities,
            clock,
            script: Script::default(),
            track: Arc::new(PositionTrack::new(1024)),
            status: Mutex::new(StageStatus::Idle),
            armed: Mutex::new(None),
        };
        stage.report(stage.least());
        stage
    }

    /// Its script.
    pub fn script(&self) -> &Script {
        &self.script
    }

    /// What its trigger output is armed with, if anything.
    pub fn armed(&self) -> Option<(Axis, Vec<Um>)> {
        self.armed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Puts it in alarm, for `reason`.
    pub fn raise_alarm(&self, reason: &str) {
        *self.status() = StageStatus::Alarm {
            reason: reason.to_owned(),
        };
    }

    fn status(&self) -> MutexGuard<'_, StageStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn least(&self) -> Xyz {
        let c = &self.capabilities;
        Xyz::new(c.x.min, c.y.min, c.z.min)
    }

    /// Reports `position` at the clock's time.
    fn report(&self, position: Xyz) -> PositionSample {
        let sample = PositionSample {
            time: self.clock.now(),
            position,
        };
        self.track.push(sample);
        sample
    }

    /// Fails if it is in alarm.
    fn ready(&self) -> Result<()> {
        match &*self.status() {
            StageStatus::Alarm { reason } => Err(Error::Alarm {
                reason: reason.clone(),
            }),
            _ => Ok(()),
        }
    }

    fn latest(&self) -> Result<PositionSample> {
        self.track.latest().ok_or(Error::Disconnected)
    }
}

impl Stage for FakeStage {
    fn capabilities(&self) -> &StageCapabilities {
        &self.capabilities
    }

    fn status(&self) -> HalFuture<'_, StageStatus> {
        Box::pin(async move {
            self.script.call("status")?;
            Ok(self.status().clone())
        })
    }

    fn position(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.script.call("position")?;
            self.latest()
        })
    }

    fn track(&self) -> Arc<PositionTrack> {
        Arc::clone(&self.track)
    }

    fn move_to(&self, target: Xyz, speed: Option<UmPerS>) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.script.call("move_to")?;
            self.ready()?;
            self.capabilities.check_target(target)?;
            self.capabilities.check_speed(speed)?;
            *self.status() = StageStatus::Idle;
            Ok(self.report(target))
        })
    }

    fn jog(&self, _: Axis, _: bool, speed: Option<UmPerS>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("jog")?;
            self.ready()?;
            self.capabilities.check_speed(speed)?;
            *self.status() = StageStatus::Moving;
            Ok(())
        })
    }

    fn stop(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("stop")?;
            let mut status = self.status();
            if status.is_moving() || *status == StageStatus::Hold {
                *status = StageStatus::Idle;
            }
            drop(status);
            let position = self.latest()?.position;
            self.report(position);
            Ok(())
        })
    }

    fn home(&self) -> HalFuture<'_, PositionSample> {
        Box::pin(async move {
            self.script.call("home")?;
            if !self.capabilities.homing {
                return Err(Error::Unsupported {
                    what: "homing".to_owned(),
                });
            }
            *self.status() = StageStatus::Idle;
            Ok(self.report(self.least()))
        })
    }

    fn clear_alarm(&self) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("clear_alarm")?;
            let mut status = self.status();
            if matches!(*status, StageStatus::Alarm { .. }) {
                *status = StageStatus::Idle;
            }
            Ok(())
        })
    }

    fn arm_trigger(&self, axis: Axis, positions: Vec<Um>) -> HalFuture<'_, ()> {
        Box::pin(async move {
            self.script.call("arm_trigger")?;
            if !self.capabilities.trigger_output {
                return Err(Error::Unsupported {
                    what: "a trigger output".to_owned(),
                });
            }
            *self.armed.lock().unwrap_or_else(PoisonError::into_inner) = Some((axis, positions));
            Ok(())
        })
    }
}
