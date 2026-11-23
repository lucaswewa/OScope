//! The simulated world: samples, optics, and a camera, a stage and lights that
//! implement `oscope-hal`'s traits (plan §5.10, ADR-0013).
//!
//! Layer L2 of ADR-0005. It holds the camera (P12), the clock and the stage
//! (P11), and what the camera sees (P10, P13):
//!
//! - [`camera`]: the [`SimCamera`], a MIChrome 20 stand-in, with its
//!   [`Mounting`] on the stage;
//! - [`sensor`]: the [`SensorModel`], from light to counts;
//! - [`clock`]: the [`SimClock`], at 1×, scaled or stepped, which every
//!   simulated device shares;
//! - [`devices`]: [`SimLights`] and [`SimMount`], which set what the camera
//!   sees, and the whole [`Simulator`] built from the configuration;
//! - [`motion`]: [`Segment`]s, straight moves with grblHAL's acceleration;
//! - [`stage`]: the [`SimStage`], a grblHAL-like stage on the clock;
//!
//! - [`tiles`]: the [`TileCache`] of rendered tiles the camera samples, so
//!   that it renders each piece of the world once, not every frame;
//! - `gpu`, with the `cuda` feature: the tiles in the GPU's memory, the H&E
//!   section's made there, and the camera's exposure there (ADR-0062);
//! - [`world`]: the [`World`], a [`Slide`] with its coverslip and sample on
//!   the stage, and [`World::render`], which gives the image of a [`Field`]
//!   in transmitted, reflected or fluorescent [`Light`];
//! - [`optics`]: the [`Optics`] of an objective and sensor: the pixel at the
//!   sample, the blur out of focus, the depth of field;
//! - [`blur`]: the Gaussian that stands in for the point spread function;
//! - [`fluorescence`]: [`Dye`]s, [`Excitation`] lines, and a confocal's
//!   sectioning, in the simulator only;
//! - [`sample`]: the [`Sample`] trait, for 3D samples made of [`Layer`]s;
//! - [`tissue`]: [`HeTissue`], the procedural H&E section, with its
//!   [`Nucleus`] ground truth;
//! - [`surface`]: [`Surface`], a reflective chip whose height map is the
//!   ground truth for depth from focus;
//! - [`cells`]: [`CellCulture`], cells in 3D in three dyes, with their
//!   [`Cell`]s and their volume as ground truth;
//! - [`structures`]: [`Structures`], filaments and hollow tubules finer than
//!   the optics resolve, with their fine-scale truth;
//! - [`import`]: [`Imported`], your own image or stack as a sample;
//! - [`catalogue`]: the [`SampleCatalogue`] a configuration lists, and the
//!   [`SlideLoader`] that loads and unloads its samples;
//! - [`target`]: [`TestTarget`], a sample with exact answers;
//! - [`noise`]: the seeded hashing and noise that make every sample the
//!   same however it is rendered.
//!
//! **Everything is deterministic from a seed.** Images are linear RGB, as
//! `oscope-compute`'s [`Image`](oscope_compute::Image).

pub mod blur;
pub mod camera;
pub mod catalogue;
pub mod cells;
pub mod clock;
pub mod colour_target;
pub mod devices;
pub mod fluorescence;
#[cfg(feature = "cuda")]
pub mod gpu;
pub mod import;
pub mod motion;
pub mod noise;
pub mod optics;
pub mod sample;
pub mod sensor;
pub mod stage;
pub mod structures;
pub mod surface;
pub mod target;
pub mod tiles;
pub mod tissue;
pub mod world;

pub use camera::{Mounting, SimCamera, SimCameraConfig};
pub use catalogue::{SampleCatalogue, SampleEntry, SampleKind, SlideLoader};
pub use cells::{Cell, CellCulture};
pub use clock::{Rate, SimClock};
pub use colour_target::ColourTargetSample;
pub use devices::{SimFaults, SimLights, SimMount, Simulator, SimulatorConfig};
pub use fluorescence::{Dye, Excitation};
pub use import::{ImportLight, Imported};
pub use motion::Segment;
pub use optics::Optics;
pub use sample::{GpuModel, Layer, LayerKind, Sample};
pub use sensor::SensorModel;
pub use stage::{LostSteps, Pulse, Settling, SimStage, SimStageConfig, TruePath};
pub use structures::Structures;
pub use surface::Surface;
pub use target::TestTarget;
pub use tiles::{CacheStats, TileCache, TiledField};
pub use tissue::{HeTissue, Nucleus};
pub use world::{Field, Light, Slide, World};
