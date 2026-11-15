//! The calibration wizard (P33; plan §5.13): a state machine of steps, each
//! with a prompt of what to do first, a job that measures, and a check of
//! what the job gave. It is saved after every change, so it resumes where
//! it was after the server restarts. The M2 app renders it; `ex-wizard`
//! drives it from the command line.
//!
//! A step waits, then prompts; confirmed, its job runs; the job's end
//! settles it, done or failed. A failed step may be retried or skipped, and
//! a prompting one skipped. The last step settled, the wizard is finished.
//!
//! The stage's step comes last: it homes, which takes the sample out of
//! view and back, and no step after it then waits for a frame (as P32's
//! stage calibration homes last).

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use oscope_core::JobId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::jobs::{CHECK, COLOUR, DARK, FLAT, GEOMETRY, STAGE};

/// The wizard's steps, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// The objective check.
    Objective,
    /// The geometry: the pixel size and the camera-to-stage matrix.
    Geometry,
    /// A dark frame.
    Dark,
    /// A flat field, and the white balance.
    Flat,
    /// The colour matrix.
    Colour,
    /// The stage: backlash, homing, soft limits.
    Stage,
}

impl StepKind {
    /// Every step, in order.
    pub const ALL: [StepKind; 6] = [
        StepKind::Objective,
        StepKind::Geometry,
        StepKind::Dark,
        StepKind::Flat,
        StepKind::Colour,
        StepKind::Stage,
    ];

    /// Its title.
    pub fn title(self) -> &'static str {
        match self {
            StepKind::Objective => "Check the objective",
            StepKind::Geometry => "Calibrate the geometry",
            StepKind::Dark => "Take a dark frame",
            StepKind::Flat => "Take a flat field",
            StepKind::Colour => "Calibrate colour",
            StepKind::Stage => "Calibrate the stage",
        }
    }

    /// What to do before confirming it.
    pub fn prompt(self) -> &'static str {
        match self {
            StepKind::Objective => {
                "Fit the objective that is in the light path, and focus on a textured part of \
                 the sample. Two small moves check that the image moves as that objective's should."
            }
            StepKind::Geometry => {
                "Keep a textured part of the sample in focus, with room around it for moves of \
                 a quarter of the field each way."
            }
            StepKind::Dark => {
                "Lights that can be switched are turned off for the dark frame, and back on. \
                 Block any other light from reaching the camera."
            }
            StepKind::Flat => {
                "Move to a blank part of the slide, clear glass with nothing on it, in focus, \
                 with the light on as it will be used."
            }
            StepKind::Colour => {
                "Load the colour target, and centre it in the field of the 5× or the 10×, in \
                 focus, with the light on as it will be used."
            }
            StepKind::Stage => {
                "Bring a textured part of the sample back into view, in focus: the stage \
                 measures its backlash there, then homes, and comes back."
            }
        }
    }

    /// The job that measures it.
    pub fn job(self) -> &'static str {
        match self {
            StepKind::Objective => CHECK,
            StepKind::Geometry => GEOMETRY,
            StepKind::Dark => DARK,
            StepKind::Flat => FLAT,
            StepKind::Colour => COLOUR,
            StepKind::Stage => STAGE,
        }
    }
}

/// Where a step stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StepState {
    /// Not reached yet.
    Waiting,
    /// Its prompt is shown: do what it asks, then confirm.
    Prompting,
    /// Its job runs.
    Running {
        /// The job.
        job: JobId,
    },
    /// Done.
    Done {
        /// What it made: a set, `calibration/<id>`, or a check's verdict.
        result: String,
    },
    /// Skipped.
    Skipped,
    /// Failed: it may be retried, or skipped.
    Failed {
        /// Why.
        error: String,
    },
}

/// A step: what it is, and where it stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Step {
    /// What it is.
    pub kind: StepKind,
    /// Its title.
    pub title: String,
    /// What to do before confirming it.
    pub prompt: String,
    /// Where it stands.
    pub state: StepState,
}

/// Why the wizard refused a change.
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("conflict (409): {0}")]
pub struct WizardError(pub String);

fn refuse<T>(why: impl Into<String>) -> Result<T, WizardError> {
    Err(WizardError(why.into()))
}

/// The calibration wizard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wizard {
    /// Its ID.
    pub id: Uuid,
    /// When it started.
    pub started: DateTime<Utc>,
    /// Its steps.
    pub steps: Vec<Step>,
    /// The step in hand, by index; the steps' count once finished.
    pub current: usize,
    /// Whether every step is settled, or it was cancelled.
    pub finished: bool,
    /// Whether it was cancelled.
    pub cancelled: bool,
}

impl Wizard {
    /// A wizard of `kinds`, in their order, its first prompting.
    pub fn new(kinds: &[StepKind]) -> Result<Wizard, WizardError> {
        if kinds.is_empty() {
            return refuse("a wizard needs a step");
        }
        let steps = kinds
            .iter()
            .enumerate()
            .map(|(i, kind)| Step {
                kind: *kind,
                title: kind.title().to_owned(),
                prompt: kind.prompt().to_owned(),
                state: if i == 0 {
                    StepState::Prompting
                } else {
                    StepState::Waiting
                },
            })
            .collect();
        Ok(Wizard {
            id: Uuid::now_v7(),
            started: Utc::now(),
            steps,
            current: 0,
            finished: false,
            cancelled: false,
        })
    }

    /// The step in hand, if it isn't finished.
    pub fn step(&self) -> Option<&Step> {
        self.steps.get(self.current).filter(|_| !self.finished)
    }

    fn here(&mut self) -> Result<&mut Step, WizardError> {
        if self.finished {
            return refuse("the wizard is finished");
        }
        Ok(&mut self.steps[self.current])
    }

    /// On to the next step, prompting; finished after the last.
    fn advance(&mut self) {
        self.current += 1;
        match self.steps.get_mut(self.current) {
            Some(next) => next.state = StepState::Prompting,
            None => self.finished = true,
        }
    }

    /// The prompt done: the step's job, `job`, has been submitted.
    pub fn confirm(&mut self, job: JobId) -> Result<(), WizardError> {
        let step = self.here()?;
        match step.state {
            StepState::Prompting => {
                step.state = StepState::Running { job };
                Ok(())
            }
            _ => refuse(format!("“{}” isn't waiting to be confirmed", step.title)),
        }
    }

    /// The job `job` ended: `Ok` with what the step made, or `Err` with
    /// why it failed. The step settles; done, the wizard moves on. A job
    /// that isn't the step in hand's changes nothing.
    pub fn ended(&mut self, job: JobId, outcome: Result<String, String>) {
        let Ok(step) = self.here() else { return };
        if step.state != (StepState::Running { job }) {
            return;
        }
        match outcome {
            Ok(result) => {
                step.state = StepState::Done { result };
                self.advance();
            }
            Err(error) => step.state = StepState::Failed { error },
        }
    }

    /// The step in hand, prompting or failed, skipped.
    pub fn skip(&mut self) -> Result<(), WizardError> {
        let step = self.here()?;
        match step.state {
            StepState::Prompting | StepState::Failed { .. } => {
                step.state = StepState::Skipped;
                self.advance();
                Ok(())
            }
            _ => refuse(format!(
                "“{}” can't be skipped while its job runs",
                step.title
            )),
        }
    }

    /// The step in hand, failed, prompting again.
    pub fn retry(&mut self) -> Result<(), WizardError> {
        let step = self.here()?;
        match step.state {
            StepState::Failed { .. } => {
                step.state = StepState::Prompting;
                Ok(())
            }
            _ => refuse(format!("“{}” hasn't failed", step.title)),
        }
    }

    /// Ends the wizard where it is: a running step's job carries on, but
    /// the wizard no longer waits for it.
    pub fn cancel(&mut self) -> Result<(), WizardError> {
        if self.finished {
            return refuse("the wizard is finished");
        }
        self.finished = true;
        self.cancelled = true;
        Ok(())
    }

    /// The job the step in hand waits for, if it runs.
    pub fn running(&self) -> Option<JobId> {
        match self.step()?.state {
            StepState::Running { job } => Some(job),
            _ => None,
        }
    }

    /// Saves it to `path`, whole: a part file, renamed into place.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let part = path.with_extension("json.part");
        fs::write(
            &part,
            serde_json::to_string_pretty(self).expect("the wizard's JSON"),
        )?;
        fs::rename(&part, path)
    }

    /// The wizard saved at `path`, if there is one.
    pub fn load(path: &Path) -> std::io::Result<Option<Wizard>> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(n: u128) -> JobId {
        JobId(Uuid::from_u128(n))
    }

    #[test]
    fn steps_prompt_run_and_settle_in_turn() {
        let mut wizard = Wizard::new(&[StepKind::Dark, StepKind::Flat, StepKind::Colour]).unwrap();
        assert_eq!(wizard.steps[0].state, StepState::Prompting);
        assert_eq!(wizard.steps[1].state, StepState::Waiting);
        // Confirmed, the first runs; another job's end changes nothing.
        wizard.confirm(job(1)).unwrap();
        assert_eq!(wizard.running(), Some(job(1)));
        wizard.ended(job(9), Ok("calibration/x".to_owned()));
        assert_eq!(wizard.running(), Some(job(1)));
        // Its job done, the next prompts.
        wizard.ended(job(1), Ok("calibration/a".to_owned()));
        let done = StepState::Done {
            result: "calibration/a".to_owned(),
        };
        assert_eq!((wizard.steps[0].state.clone(), wizard.current), (done, 1));
        assert_eq!(wizard.steps[1].state, StepState::Prompting);
        // Failed, retried, failed again, skipped.
        wizard.confirm(job(2)).unwrap();
        wizard.ended(job(2), Err("the field isn't blank".to_owned()));
        assert!(matches!(wizard.steps[1].state, StepState::Failed { .. }));
        wizard.retry().unwrap();
        wizard.confirm(job(3)).unwrap();
        wizard.ended(job(3), Err("still not blank".to_owned()));
        wizard.skip().unwrap();
        assert_eq!(
            (wizard.steps[1].state.clone(), wizard.current),
            (StepState::Skipped, 2)
        );
        // The last done, the wizard is finished.
        wizard.confirm(job(4)).unwrap();
        wizard.ended(job(4), Ok("calibration/c".to_owned()));
        assert!(wizard.finished && !wizard.cancelled && wizard.step().is_none());
    }

    #[test]
    fn what_a_step_can_t_do_is_refused() {
        let mut wizard = Wizard::new(&StepKind::ALL).unwrap();
        assert!(wizard.retry().is_err(), "nothing failed");
        wizard.confirm(job(1)).unwrap();
        assert!(wizard.confirm(job(2)).is_err(), "it already runs");
        assert!(wizard.skip().is_err(), "its job runs");
        wizard.cancel().unwrap();
        assert!(wizard.finished && wizard.cancelled);
        assert!(wizard.confirm(job(3)).is_err() && wizard.cancel().is_err());
        assert!(Wizard::new(&[]).is_err());
        let error = wizard.skip().unwrap_err().to_string();
        assert!(error.starts_with("conflict (409)"), "{error}");
    }

    #[test]
    fn a_saved_wizard_resumes_where_it_was() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("wizard.json");
        assert_eq!(Wizard::load(&path).unwrap(), None);
        let mut wizard = Wizard::new(&StepKind::ALL).unwrap();
        wizard.confirm(job(7)).unwrap();
        wizard.save(&path).unwrap();
        let mut back = Wizard::load(&path).unwrap().unwrap();
        assert_eq!(back, wizard);
        back.ended(job(7), Ok("plausible".to_owned()));
        assert_eq!(back.current, 1);
    }
}
