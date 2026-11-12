//! OScope's interface: its [`teta_wot`] Things, the media-plane routes,
//! authentication and the control lease.
//!
//! Layer L5 of ADR-0005. It is the only crate that depends on `teta-wot`.
//!
//! An [`Api`] serves, at one origin (ADR-0006):
//!
//! - **the control plane:** the Things, under [`API_PREFIX`], such as the
//!   [`system`] Thing at `/api/v1/system/` and the [`logs`] Thing;
//! - **the media plane:** OScope's own routes, under [`MEDIA_PREFIX`];
//! - **`/health`;**
//! - **pages,** for the examples ([`ApiBuilder::pages`]).
//!
//! The media plane's routes and the pages get the same authentication, CORS
//! and graceful shutdown as the Things.

pub mod logs;
mod media;
mod pages;
mod server;
pub mod system;

pub use server::{API_PREFIX, Api, ApiBuilder, ApiError, MEDIA_PREFIX, Roots};
pub use system::{GIT_COMMIT, VERSION};
/// Completes when the process is asked to stop: Ctrl-C, and on Windows the
/// console closing, logging off or shutting down; on Linux `SIGTERM` and
/// `SIGHUP`.
pub use teta_wot::server::shutdown_signal;
