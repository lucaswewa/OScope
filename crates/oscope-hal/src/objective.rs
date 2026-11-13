//! Objectives: the [`ObjectiveCatalogue`], the [`ObjectiveMount`] trait, and
//! [`ManualObjectiveMount`].
//!
//! The instrument has a single mount, and objectives are swapped by hand
//! (D18), so the user says which one is fitted. The catalogue is a TOML
//! file; its defaults are Mitutoyo's published values, confirmed on the
//! instrument in M3. Lengths are in micrometres (ADR-0007).

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use oscope_core::Um;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Error, HalFuture, Result};

/// The tube lens Mitutoyo's objectives are designed for.
const MITUTOYO_TUBE_LENS: Um = Um(200_000.0);

fn mitutoyo_tube_lens() -> Um {
    MITUTOYO_TUBE_LENS
}

/// One objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    /// Its name, unique in the catalogue, such as `20x`. Frames record it.
    pub name: String,
    /// Its maker and model.
    #[serde(default)]
    pub model: String,
    /// Its magnification, with the tube lens it was designed for.
    pub magnification: f64,
    /// Its numerical aperture.
    pub na: f64,
    /// The distance from its front to the sample in focus.
    pub working_distance: Um,
    /// The tube lens it was designed for: 200 mm, Mitutoyo's, unless the
    /// catalogue says otherwise.
    #[serde(default = "mitutoyo_tube_lens")]
    pub design_tube_lens: Um,
}

/// The objectives that can be fitted, and the instrument's tube lens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveCatalogue {
    /// The instrument's tube lens.
    pub tube_lens: Um,
    /// The objectives, as `[[objective]]` tables in TOML.
    #[serde(rename = "objective")]
    pub objectives: Vec<Objective>,
}

impl ObjectiveCatalogue {
    /// Mitutoyo's M Plan Apo 5×, 10×, 20× and 50×, with their published
    /// numerical apertures and working distances, behind a 200 mm tube lens
    /// (plan §5.9).
    pub fn mitutoyo() -> Self {
        let objective = |magnification: f64, na: f64, working_distance_mm: f64| Objective {
            name: format!("{magnification}x"),
            model: format!("Mitutoyo M Plan Apo {magnification}×"),
            magnification,
            na,
            working_distance: Um::from_mm(working_distance_mm),
            design_tube_lens: MITUTOYO_TUBE_LENS,
        };
        Self {
            tube_lens: MITUTOYO_TUBE_LENS,
            objectives: vec![
                objective(5.0, 0.14, 34.0),
                objective(10.0, 0.28, 33.5),
                objective(20.0, 0.42, 20.0),
                objective(50.0, 0.55, 13.0),
            ],
        }
    }

    /// A catalogue read from TOML, and checked.
    pub fn from_toml(text: &str) -> Result<Self> {
        let catalogue: Self = toml::from_str(text).map_err(Error::CatalogueFile)?;
        catalogue.validate()?;
        Ok(catalogue)
    }

    /// Checks the catalogue's rules: at least one objective; names unique
    /// and not empty; magnifications, working distances and tube lenses more
    /// than 0; numerical apertures more than 0 and at most 1.6, an oil
    /// immersion objective's.
    pub fn validate(&self) -> Result<()> {
        let invalid = |reason: String| Err(Error::InvalidCatalogue { reason });
        if self.objectives.is_empty() {
            return invalid("it has no objectives".to_owned());
        }
        if !positive(self.tube_lens.0) {
            return invalid(format!("its tube lens is {}", self.tube_lens));
        }
        let mut names = HashSet::new();
        for o in &self.objectives {
            let name = &o.name;
            if name.trim().is_empty() {
                return invalid("an objective has no name".to_owned());
            }
            if !names.insert(name) {
                return invalid(format!("two objectives are named {name:?}"));
            }
            if !positive(o.magnification) {
                return invalid(format!("{name:?}'s magnification is {}", o.magnification));
            }
            if !positive(o.na) || o.na > 1.6 {
                return invalid(format!("{name:?}'s numerical aperture is {}", o.na));
            }
            if !positive(o.working_distance.0) {
                return invalid(format!(
                    "{name:?}'s working distance is {}",
                    o.working_distance
                ));
            }
            if !positive(o.design_tube_lens.0) {
                return invalid(format!(
                    "{name:?}'s design tube lens is {}",
                    o.design_tube_lens
                ));
            }
        }
        Ok(())
    }

    /// The objective named `name`.
    pub fn get(&self, name: &str) -> Result<&Objective> {
        self.objectives
            .iter()
            .find(|o| o.name == name)
            .ok_or_else(|| Error::Unknown {
                kind: "objective",
                name: name.to_owned(),
            })
    }

    /// An objective's magnification on this instrument: its own, scaled by
    /// the instrument's tube lens against the one it was designed for.
    pub fn magnification(&self, objective: &Objective) -> f64 {
        objective.magnification * (self.tube_lens / objective.design_tube_lens)
    }

    /// The size at the sample of a sensor pixel `pixel` wide, through
    /// `objective`.
    pub fn pixel_size(&self, objective: &Objective, pixel: Um) -> Um {
        pixel / self.magnification(objective)
    }
}

/// What an objective mount is and offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectiveMountCapabilities {
    /// Its maker and model, or `manual`.
    pub model: String,
    /// Whether it changes objectives itself, as a motorised turret does,
    /// rather than by hand.
    pub motorised: bool,
}

/// Where the objective is fitted (plan §5.9).
pub trait ObjectiveMount: Send + Sync {
    /// What it is and offers.
    fn capabilities(&self) -> &ObjectiveMountCapabilities;

    /// The objectives it can hold.
    fn catalogue(&self) -> &ObjectiveCatalogue;

    /// The objective fitted, or `None` if nobody has said yet.
    fn fitted(&self) -> HalFuture<'_, Option<Objective>>;

    /// Fits the objective named `name` from the catalogue: by turning to it,
    /// on a motorised mount, or by recording that the user fitted it, on a
    /// manual one.
    fn fit(&self, name: String) -> HalFuture<'_, Objective>;
}

/// A single mount, whose objective the user swaps by hand (D18). It records
/// which one the user says is fitted.
#[derive(Debug)]
pub struct ManualObjectiveMount {
    capabilities: ObjectiveMountCapabilities,
    catalogue: ObjectiveCatalogue,
    fitted: Mutex<Option<Objective>>,
}

impl ManualObjectiveMount {
    /// A mount for the objectives of `catalogue`, with none said to be
    /// fitted yet.
    pub fn new(catalogue: ObjectiveCatalogue) -> Self {
        Self {
            capabilities: ObjectiveMountCapabilities {
                model: "manual".to_owned(),
                motorised: false,
            },
            catalogue,
            fitted: Mutex::new(None),
        }
    }
}

impl ObjectiveMount for ManualObjectiveMount {
    fn capabilities(&self) -> &ObjectiveMountCapabilities {
        &self.capabilities
    }

    fn catalogue(&self) -> &ObjectiveCatalogue {
        &self.catalogue
    }

    fn fitted(&self) -> HalFuture<'_, Option<Objective>> {
        let fitted = self
            .fitted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Box::pin(async move { Ok(fitted) })
    }

    fn fit(&self, name: String) -> HalFuture<'_, Objective> {
        Box::pin(async move {
            let objective = self.catalogue.get(&name)?.clone();
            *self.fitted.lock().unwrap_or_else(PoisonError::into_inner) = Some(objective.clone());
            Ok(objective)
        })
    }
}

/// Whether `value` is a finite number more than 0: not 0, negative, NaN or
/// infinite.
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
