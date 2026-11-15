//! The samples the simulator offers, as its configuration lists them, and
//! the slide loader that puts one on the stage (D23, D24).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use oscope_core::{Point2, Um};
use serde::Deserialize;

use crate::cells::CellCulture;
use crate::colour_target::ColourTargetSample;
use crate::fluorescence::Dye;
use crate::import::{ImportLight, Imported};
use crate::sample::Sample;
use crate::structures::Structures;
use crate::surface::Surface;
use crate::target::TestTarget;
use crate::tissue::HeTissue;
use crate::world::{Slide, World};

/// What a sample is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SampleKind {
    /// [`HeTissue`] under the coverslip.
    Tissue,
    /// The [`TestTarget`], at the coverslip's middle.
    TestTarget,
    /// The reflective [`Surface`], at the coverslip's middle.
    Surface,
    /// The fluorescent [`CellCulture`] under the coverslip.
    Cells,
    /// The sub-diffraction [`Structures`] under the coverslip.
    Structures,
    /// The [`ColourTargetSample`], at the coverslip's middle (P33).
    ColourTarget,
    /// An image file, at the coverslip's middle.
    Image,
    /// A stack in a multi-page TIFF, at the coverslip's middle.
    Stack,
}

impl SampleKind {
    /// Its name in a configuration, such as `test-target`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tissue => "tissue",
            Self::TestTarget => "test-target",
            Self::Surface => "surface",
            Self::Cells => "cells",
            Self::Structures => "structures",
            Self::ColourTarget => "colour-target",
            Self::Image => "image",
            Self::Stack => "stack",
        }
    }
}

/// What an imported file's values are: see [`ImportLight`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Imaging {
    /// Brightfield: the light each pixel passes.
    #[default]
    Transmitted,
    /// The light each pixel reflects.
    Reflected,
    /// Each pixel's amount of the entry's `dye`.
    Fluorescent,
}

/// One sample in the catalogue:
///
/// ```toml
/// [[samples]]
/// name = "H&E, another seed"
/// kind = "tissue"           # tissue, test-target, surface, cells,
/// seed = 7                  # structures, image or stack
///
/// [[samples]]
/// name = "my section"
/// kind = "image"            # PNG, JPEG or TIFF
/// path = "slides/section.png"   # relative to the configuration's folder
/// um_per_pixel = 0.25
/// light = "transmitted"     # transmitted, reflected or fluorescent
///
/// [[samples]]
/// name = "my confocal stack"
/// kind = "stack"            # a multi-page TIFF
/// path = "stacks/cells.tif"
/// um_per_pixel = 0.1
/// z_step = 0.3
/// light = "fluorescent"
/// dye = "green"             # dapi, green, orange or far-red
/// ```
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleEntry {
    /// Its name, which loads it.
    pub name: String,
    /// What it is.
    pub kind: SampleKind,
    /// A procedural sample's seed: 1 unless given.
    #[serde(default = "first_seed")]
    pub seed: u64,
    /// An import's file.
    pub path: Option<PathBuf>,
    /// An import's scale.
    pub um_per_pixel: Option<f64>,
    /// A stack's spacing between pages.
    pub z_step: Option<f64>,
    /// What an import's values are.
    #[serde(default)]
    pub light: Imaging,
    /// A fluorescent import's dye: green unless given.
    #[serde(default = "green")]
    pub dye: Dye,
}

fn first_seed() -> u64 {
    1
}

fn green() -> Dye {
    Dye::Green
}

impl SampleEntry {
    /// A procedural sample of `kind` from seed 1.
    pub fn procedural(name: &str, kind: SampleKind) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            seed: 1,
            path: None,
            um_per_pixel: None,
            z_step: None,
            light: Imaging::default(),
            dye: Dye::Green,
        }
    }

    /// Whether it carries ground truth: every procedural sample does, an
    /// import doesn't.
    pub fn has_ground_truth(&self) -> bool {
        !matches!(self.kind, SampleKind::Image | SampleKind::Stack)
    }

    /// Builds the sample, with an import's path taken from `base`.
    pub fn build(&self, base: &Path) -> Result<Arc<dyn Sample>, String> {
        let cover = Slide::COVERSLIP;
        let middle = Point2::new(
            cover.origin.x + cover.size.width / 2.0,
            cover.origin.y + cover.size.height / 2.0,
        );
        let seed = self.seed;
        Ok(match self.kind {
            SampleKind::Tissue => Arc::new(HeTissue::new(seed, cover)),
            SampleKind::TestTarget => Arc::new(TestTarget::new(
                Point2::new(middle.x - 2_000.0, middle.y - 2_000.0),
                0.5,
                0.5,
            )),
            SampleKind::Surface => {
                let half = Surface::SIDE.0 / 2.0;
                Arc::new(Surface::new(
                    seed,
                    Point2::new(middle.x - half, middle.y - half),
                ))
            }
            SampleKind::Cells => Arc::new(CellCulture::new(seed, cover)),
            SampleKind::Structures => Arc::new(Structures::new(seed, cover)),
            SampleKind::ColourTarget => Arc::new(ColourTargetSample::new(middle)),
            SampleKind::Image | SampleKind::Stack => {
                let what = |field: &str| format!("{}: an import needs `{field}`", self.name);
                let path = base.join(self.path.as_ref().ok_or_else(|| what("path"))?);
                let scale = Um(self.um_per_pixel.ok_or_else(|| what("um_per_pixel"))?);
                let z_step = match self.kind {
                    SampleKind::Stack => Um(self.z_step.ok_or_else(|| what("z_step"))?),
                    _ => Um(0.0),
                };
                let light = match self.light {
                    Imaging::Transmitted => ImportLight::Transmitted,
                    Imaging::Reflected => ImportLight::Reflected,
                    Imaging::Fluorescent => ImportLight::Fluorescent(self.dye),
                };
                let imported = Imported::read(&path, scale, z_step, Point2::new(0.0, 0.0), light)?;
                Arc::new(imported.centred_on(middle))
            }
        })
    }
}

/// The samples the simulator offers: its built-in ones, and those its
/// configuration adds, as `[[samples]]` (see [`SampleEntry`]).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleCatalogue {
    /// The samples, by name.
    #[serde(default)]
    pub samples: Vec<SampleEntry>,
}

impl SampleCatalogue {
    /// The built-in samples, each from seed 1.
    pub fn builtin() -> Self {
        let entry = SampleEntry::procedural;
        Self {
            samples: vec![
                entry("H&E tissue", SampleKind::Tissue),
                entry("test target", SampleKind::TestTarget),
                entry("reflective surface", SampleKind::Surface),
                entry("fluorescent cells", SampleKind::Cells),
                entry("sub-diffraction structures", SampleKind::Structures),
                entry("colour target", SampleKind::ColourTarget),
            ],
        }
    }

    /// The built-in samples, and those `toml` lists as `[[samples]]`; one
    /// of the same name as a built-in replaces it.
    pub fn from_toml(toml: &str) -> Result<Self, String> {
        let listed: Self = toml::from_str(toml).map_err(|e| e.to_string())?;
        let mut catalogue = Self::builtin();
        for entry in listed.samples {
            catalogue.samples.retain(|s| s.name != entry.name);
            catalogue.samples.push(entry);
        }
        Ok(catalogue)
    }

    /// The sample called `name`.
    pub fn get(&self, name: &str) -> Option<&SampleEntry> {
        self.samples.iter().find(|s| s.name == name)
    }
}

/// Puts samples from a catalogue on the stage, and takes them off: the
/// world's slide, which the camera sees.
pub struct SlideLoader {
    world: Arc<RwLock<World>>,
    catalogue: RwLock<SampleCatalogue>,
    base: PathBuf,
    /// Where a slide's corner sits on the stage.
    slide_at: Point2,
    loaded: Mutex<Option<String>>,
}

impl SlideLoader {
    /// A loader for `world`, from `catalogue`, with imports' paths taken
    /// from `base`, putting slides with their corner at `slide_at` on the
    /// stage.
    pub fn new(
        world: Arc<RwLock<World>>,
        catalogue: SampleCatalogue,
        base: &Path,
        slide_at: Point2,
    ) -> Self {
        Self {
            world,
            catalogue: RwLock::new(catalogue),
            base: base.to_owned(),
            slide_at,
            loaded: Mutex::new(None),
        }
    }

    /// The catalogue.
    pub fn catalogue(&self) -> SampleCatalogue {
        self.catalogue
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Adds `entry` to the catalogue, in place of one of the same name,
    /// once it builds: an import's file must read.
    pub fn add(&self, entry: SampleEntry) -> Result<(), String> {
        entry.build(&self.base)?;
        let mut catalogue = self
            .catalogue
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        catalogue.samples.retain(|s| s.name != entry.name);
        catalogue.samples.push(entry);
        Ok(())
    }

    /// Loads the sample called `name`, in place of any slide on the stage.
    /// If it can't be built, the stage stays as it was.
    pub fn load(&self, name: &str) -> Result<(), String> {
        let entry = self
            .catalogue()
            .get(name)
            .cloned()
            .ok_or_else(|| format!("no sample called {name:?}"))?;
        let sample = entry.build(&self.base)?;
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        self.world
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .slide = Some(Slide::new(sample, self.slide_at));
        *loaded = Some(name.to_owned());
        Ok(())
    }

    /// Takes the slide off the stage, leaving it empty, and says which
    /// sample it was.
    pub fn unload(&self) -> Option<String> {
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        self.world
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .slide = None;
        loaded.take()
    }

    /// The sample on the stage, if any.
    pub fn loaded(&self) -> Option<String> {
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
