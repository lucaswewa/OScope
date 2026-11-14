//! The C ABI between OScope and a Rust extension's library, version 1
//! (ADR-0019).
//!
//! The library exports [`ENTRY`], which gives its [`Extension`] table. The
//! host calls the table's functions, lending each call its own [`Host`]
//! table and a [`Call`] to pass back to it.
//!
//! - **Versions.** Each table starts with its ABI version, so that a host
//!   can read it whatever the version. The host loads only a library of its
//!   own version.
//! - **Ownership.** The host owns every buffer: inputs, and the outputs it
//!   allocates through [`Host::output`]. Text and images passed to a call
//!   are borrowed for that call only. Nothing is freed across the boundary.
//! - **Errors and panics.** A function returns a [`Status`]. Before it
//!   returns [`FAILED`] or [`PANICKED`], it tells the host why through
//!   [`Host::fail`]. No panic crosses the boundary: each side catches its
//!   own.
//! - **Threads.** The host may call [`Extension::run`] from several
//!   threads at once.
//!
//! The layouts here are the ABI: changing one is a new version.

use std::ffi::c_void;

/// The ABI's version.
pub const ABI_VERSION: u32 = 1;

/// The name of the function every extension exports, ending with its NUL:
/// an [`Entry`].
pub const ENTRY: &[u8] = b"oscope_extension\0";

/// What [`ENTRY`] is. It gives the extension's table, which lives as long
/// as the library.
pub type Entry = extern "C" fn() -> *const Extension;

/// What a call returned.
pub type Status = u32;

/// The call did what it should.
pub const OK: Status = 0;

/// The call failed, and said why through [`Host::fail`].
pub const FAILED: Status = 1;

/// The extension panicked during the call, and said where through
/// [`Host::fail`].
pub const PANICKED: Status = 2;

/// UTF-8 text, borrowed for a call. Not NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Str {
    /// The first byte; dangling if `len` is 0.
    pub data: *const u8,
    /// Bytes.
    pub len: usize,
}

impl Str {
    /// Borrows `text` as a [`Str`], for a call.
    pub fn new(text: &str) -> Str {
        Str {
            data: text.as_ptr(),
            len: text.len(),
        }
    }
}

/// An image, borrowed for a call: `height` rows of `width` pixels, each of
/// `channels` 32-bit floats, interleaved, the rows one after the other.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ImageRef {
    /// The first sample; dangling for an empty image.
    pub data: *const f32,
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Samples per pixel.
    pub channels: u32,
}

/// An output's image, which the host made for the extension to fill, laid
/// out as [`ImageRef`]: zeros at first. `data` is null when the host
/// refused it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ImageMut {
    /// The first sample.
    pub data: *mut f32,
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Samples per pixel.
    pub channels: u32,
}

/// A call's context: the host's own, which the extension passes back to
/// the host's functions as it is.
pub type Call = c_void;

/// What the host lends the extension for a call.
#[repr(C)]
#[derive(Debug)]
pub struct Host {
    /// The ABI version the host speaks.
    pub abi_version: u32,
    /// Takes the extension's description: a JSON array of its nodes, as
    /// [`crate::Descriptor`] serialises them.
    pub describe: extern "C" fn(call: *mut Call, json: Str),
    /// Makes the image of output `port`, of zeros. Its `data` is null when
    /// the port doesn't exist, was made already, or the image is too large.
    pub output: extern "C" fn(
        call: *mut Call,
        port: u32,
        width: u32,
        height: u32,
        channels: u32,
    ) -> ImageMut,
    /// Takes why the call failed.
    pub fail: extern "C" fn(call: *mut Call, message: Str),
}

/// What an extension gives the host.
#[repr(C)]
#[derive(Debug)]
pub struct Extension {
    /// The ABI version the extension speaks: first, so that any host can
    /// read it.
    pub abi_version: u32,
    /// Describes the extension's nodes, through [`Host::describe`].
    pub describe: extern "C" fn(host: *const Host, call: *mut Call) -> Status,
    /// Runs node `node`, by its place in the description, with `params`,
    /// JSON that has passed the node's schema, on `count` `inputs`, one for
    /// each of the node's inputs in order. It makes every output through
    /// [`Host::output`].
    pub run: extern "C" fn(
        host: *const Host,
        call: *mut Call,
        node: u32,
        params: Str,
        inputs: *const ImageRef,
        count: u32,
    ) -> Status,
}
