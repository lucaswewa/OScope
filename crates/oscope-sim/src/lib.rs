//! The simulated world: samples, optics, and a camera, a stage and lights that
//! implement `oscope-hal`'s traits (plan §5.10, ADR-0013).
//!
//! Layer L2 of ADR-0005. So far (P10) it holds what the camera sees:
//!
//! - [`world`]: the [`World`], a [`Slide`] with its coverslip and sample on
//!   the stage, and [`World::render`], which gives the image of a [`Field`]
//!   in transmitted or reflected [`Light`];
//! - [`optics`]: the [`Optics`] of an objective and sensor: the pixel at the
//!   sample, the blur out of focus, the depth of field;
//! - [`sample`]: the [`Sample`] trait, for 3D samples made of [`Layer`]s;
//! - [`tissue`]: [`HeTissue`], the procedural H&E section, with its
//!   [`Nucleus`] ground truth;
//! - [`target`]: [`TestTarget`], a sample with exact answers;
//! - [`noise`]: the seeded hashing and noise that make every sample the
//!   same however it is rendered.
//!
//! **Everything is deterministic from a seed.** Images are linear RGB, as
//! `oscope-compute`'s [`Image`](oscope_compute::Image).

pub mod noise;
pub mod optics;
pub mod sample;
pub mod target;
pub mod tissue;
pub mod world;

pub use optics::Optics;
pub use sample::{Layer, LayerKind, Sample};
pub use target::TestTarget;
pub use tissue::{HeTissue, Nucleus};
pub use world::{Field, Light, Slide, World};
