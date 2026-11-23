//! OScope's interface: its [`teta_wot`] Things, the media-plane routes,
//! authentication and the control lease.
//!
//! Layer L5 of ADR-0005. It is the only crate that depends on `teta-wot`.
//!
//! An [`Api`] serves, at one origin (ADR-0006):
//!
//! - **the control plane:** the Things, under [`API_PREFIX`], such as the
//!   [`system`] Thing at `/api/v1/system/`, the [`logs`] Thing, the
//!   instrument's [`devices`], the [`jobs`], the
//!   [`extensions`] and the [`projects`];
//! - **the media plane:** OScope's own routes, under [`MEDIA_PREFIX`];
//! - **`/health`;**
//! - **pages,** for the examples ([`ApiBuilder::pages`]).
//!
//! The media plane's routes and the pages get the same authentication, CORS
//! and graceful shutdown as the Things.

pub mod auth;
mod data;
pub mod devices;
pub mod extensions;
mod files;
pub mod jobs;
pub mod logs;
mod media;
pub mod origins;
mod pages;
pub mod power;
pub mod projects;
mod server;
pub mod settings;
pub mod status;
pub mod system;
pub mod upload;
mod volume;

pub use files::Formats;
pub use server::{API_PREFIX, Api, ApiBuilder, ApiError, MEDIA_PREFIX, Restart, Roots};
pub use system::{GIT_COMMIT, VERSION};
/// Completes on the Linux process shutdown signals, including Ctrl-C and SIGTERM.
pub use teta_wot::server::shutdown_signal;
