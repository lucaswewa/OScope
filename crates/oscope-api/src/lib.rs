//! OScope's interface: its [`teta_wot`] Things, the media-plane routes,
//! authentication and the control lease.
//!
//! Layer L5 of ADR-0005. It is the only crate that depends on `teta-wot`.
//!
//! An [`Api`] serves, at one origin (ADR-0006):
//!
//! - **the control plane:** the Things, under [`API_PREFIX`], such as the
//!   [`system`] Thing at `/api/v1/system/`, the [`logs`] Thing, the
//!   instrument's [`devices`], the live [`preview`], the [`jobs`], the
//!   [`extensions`] and the [`projects`];
//! - **the media plane:** OScope's own routes, under [`MEDIA_PREFIX`];
//! - **`/health`;**
//! - **pages,** for the examples ([`ApiBuilder::pages`]).
//!
//! The media plane's routes and the pages get the same authentication, CORS
//! and graceful shutdown as the Things.

mod batch;
pub mod calibration;
pub mod capture;
mod data;
pub mod devices;
pub mod extensions;
mod files;
pub mod focus;
pub mod jobs;
pub mod logs;
mod media;
pub mod mosaic;
mod pages;
pub mod preview;
pub mod projects;
pub mod scan;
mod server;
pub mod system;
mod zoom;

pub use files::Formats;
pub use server::{API_PREFIX, Api, ApiBuilder, ApiError, MEDIA_PREFIX, Restart, Roots};
pub use system::{GIT_COMMIT, VERSION};
/// Running as a Windows service: `teta-wot`'s (P19, ADR-0015), which only
/// this crate may depend on.
#[cfg(windows)]
pub use teta_wot::server::service;
/// Completes when the process is asked to stop: Ctrl-C, and on Windows the
/// console closing, logging off or shutting down; on Linux `SIGTERM` and
/// `SIGHUP`.
pub use teta_wot::server::shutdown_signal;
