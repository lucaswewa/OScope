//! The SDK that authors of Rust extensions build against (ADR-0019).
//!
//! Layer L3 of ADR-0005.
//! Unsafe code is allowed here, in the generated C ABI glue (plan §4.3).
//!
//! An extension is a `cdylib` crate. It implements [`ProcessingNode`] for
//! each of its nodes; [`CameraDriver`], [`StageDriver`] or
//! [`IlluminationDriver`] for each of its drivers (ADR-0036); and
//! [`Importer`] or [`Exporter`] for each of its file formats (ADR-0037). It
//! lists them with [`export!`], which makes the C ABI of [`abi`] for them:
//!
//! ```
//! use oscope_ext_sdk::{Descriptor, Image, Outputs, ProcessingNode, serde_json};
//!
//! /// Doubles each sample.
//! struct Double;
//!
//! impl ProcessingNode for Double {
//!     fn descriptor(&self) -> Descriptor {
//!         Descriptor::new("example.double", 1, "example", "Doubles each sample.")
//!             .input("image")
//!             .output("image")
//!     }
//!
//!     fn run(
//!         &self,
//!         _params: &serde_json::Value,
//!         inputs: &[Image<'_>],
//!         outputs: &mut Outputs<'_>,
//!     ) -> Result<(), String> {
//!         let image = inputs[0];
//!         let out = outputs.image(0, image.width(), image.height(), image.channels())?;
//!         for (out, sample) in out.iter_mut().zip(image.data()) {
//!             *out = 2.0 * sample;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! oscope_ext_sdk::export!(Double);
//! ```
//!
//! **Build it with panics unwinding,** Cargo's default: [`export!`] catches
//! a node's panic and turns it into an error, which a library built with
//! `panic = "abort"` can't do. [`run_in_memory`] runs a node without
//! OScope, for its tests.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

pub mod abi;
mod drivers;

pub use drivers::{
    CameraDriver, Driver, Exporter, Extent, Format, IlluminationDriver, ImageDescription, Importer,
    NamedDriver, PixelSize, PlannedTile, Sample, ScanField, ScanWorkflow, StageDriver, camera,
    exporter, illumination, importer, stage, workflow,
};
pub use {oscope_core, oscope_hal, serde_json};

/// A node, as its extension describes it. It is OScope's pipeline's
/// description (ADR-0017), with images for inputs and outputs, on the CPU.
#[derive(Clone, Debug, PartialEq)]
pub struct Descriptor {
    /// Its ID, dotted names: `example.emboss`. The manifest's
    /// `provides.nodes` names it too.
    pub id: String,
    /// Its version, raised when what it gives, or its parameters, change.
    pub version: u32,
    /// Its category, for people.
    pub category: String,
    /// What it does, in a line.
    pub description: String,
    /// Its inputs' names, each an image.
    pub inputs: Vec<String>,
    /// Its outputs' names, each an image.
    pub outputs: Vec<String>,
    /// Its parameters' JSON Schema, in the part OScope checks (ADR-0017).
    pub params: serde_json::Value,
    /// Whether it is quick enough to run on every frame of the live view.
    pub live: bool,
    /// Whether it can run tile by tile, and the pixels around a tile it
    /// reads.
    pub tileable: Option<u32>,
}

impl Descriptor {
    /// A node with no inputs, outputs or parameters yet.
    pub fn new(id: &str, version: u32, category: &str, description: &str) -> Descriptor {
        Descriptor {
            id: id.to_owned(),
            version,
            category: category.to_owned(),
            description: description.to_owned(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: serde_json::json!({ "type": "object", "additionalProperties": false }),
            live: false,
            tileable: None,
        }
    }

    /// With an input.
    pub fn input(mut self, name: &str) -> Self {
        self.inputs.push(name.to_owned());
        self
    }

    /// With an output.
    pub fn output(mut self, name: &str) -> Self {
        self.outputs.push(name.to_owned());
        self
    }

    /// With its parameters' schema.
    pub fn params(mut self, schema: serde_json::Value) -> Self {
        self.params = schema;
        self
    }

    /// Quick enough to run on every frame of the live view.
    pub fn live(mut self) -> Self {
        self.live = true;
        self
    }

    /// Able to run tile by tile, reading `halo` pixels around a tile.
    pub fn tileable(mut self, halo: u32) -> Self {
        self.tileable = Some(halo);
        self
    }

    /// As the host reads it: OScope's pipeline's descriptor, in JSON.
    pub fn to_json(&self) -> serde_json::Value {
        let ports = |names: &[String]| -> Vec<serde_json::Value> {
            let port = |name: &String| serde_json::json!({ "name": name, "type": "image" });
            names.iter().map(port).collect()
        };
        serde_json::json!({
            "id": self.id,
            "version": self.version,
            "category": self.category,
            "description": self.description,
            "inputs": ports(&self.inputs),
            "outputs": ports(&self.outputs),
            "params": self.params,
            "capabilities": {
                "cpu": true,
                "cuda": false,
                "live": self.live,
                "tileable": self.tileable.is_some(),
                "halo": self.tileable.unwrap_or(0),
            },
        })
    }
}

/// An input image, borrowed for a run: `height` rows of `width` pixels,
/// each of `channels` 32-bit floats, interleaved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Image<'a> {
    width: u32,
    height: u32,
    channels: usize,
    data: &'a [f32],
}

impl<'a> Image<'a> {
    /// An image of `data`, which must hold exactly its samples.
    pub fn new(width: u32, height: u32, channels: usize, data: &'a [f32]) -> Result<Self, String> {
        match samples(width, height, channels) {
            Some(len) if len == data.len() => Ok(Image {
                width,
                height,
                channels,
                data,
            }),
            _ => Err(format!(
                "{width} × {height} × {channels} samples, but {} given",
                data.len()
            )),
        }
    }

    /// Pixels across.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Samples per pixel.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The samples, row by row.
    pub fn data(&self) -> &'a [f32] {
        self.data
    }

    /// Sample `channel` of the pixel at (`x`, `y`), the nearest edge pixel's
    /// for a pixel outside the image.
    pub fn clamped(&self, x: i64, y: i64, channel: usize) -> f32 {
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        self.data[(y * self.width as usize + x) * self.channels + channel]
    }
}

/// How many samples an image has, if that fits in memory's addresses.
fn samples(width: u32, height: u32, channels: usize) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(channels)
}

/// Makes an output's buffer: its first sample and its length.
type Make<'a> = dyn FnMut(u32, u32, u32, u32) -> Option<(*mut f32, usize)> + 'a;

/// Where a run makes its outputs.
pub struct Outputs<'a> {
    make: &'a mut Make<'a>,
}

impl<'a> Outputs<'a> {
    /// Makes the image of output `port`, by its place in the descriptor,
    /// `width` × `height` pixels of `channels` samples, all zeros, and gives
    /// its samples to fill. Each output is made once.
    pub fn image(
        &mut self,
        port: usize,
        width: u32,
        height: u32,
        channels: usize,
    ) -> Result<&'a mut [f32], String> {
        let refused = || {
            format!(
                "output {port} can't be made: there is none, it was made already, or it is too large"
            )
        };
        let (Ok(index), Ok(depth)) = (u32::try_from(port), u32::try_from(channels)) else {
            return Err(refused());
        };
        let (data, len) = (self.make)(index, width, height, depth).ok_or_else(refused)?;
        if len == 0 {
            return Ok(&mut []);
        }
        // SAFETY: `make` gives each port's buffer once, `len` samples long,
        // and keeps it, untouched, until the run ends: past 'a.
        Ok(unsafe { std::slice::from_raw_parts_mut(data, len) })
    }
}

/// A node of an extension: what [`export!`] lists.
///
/// The host may run a node on several threads at once, so it is `Sync`.
pub trait ProcessingNode: Send + Sync + 'static {
    /// What it is.
    fn descriptor(&self) -> Descriptor;

    /// Runs it on `inputs`, in the order of its descriptor's inputs, with
    /// `params`, which have passed its schema. It makes each output with
    /// [`Outputs::image`].
    fn run(
        &self,
        params: &serde_json::Value,
        inputs: &[Image<'_>],
        outputs: &mut Outputs<'_>,
    ) -> Result<(), String>;
}

/// An output a node made in [`run_in_memory`].
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedImage {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Samples per pixel.
    pub channels: usize,
    /// The samples, row by row.
    pub data: Vec<f32>,
}

/// Runs `node` without OScope, for its tests: its outputs, in order, or
/// why it failed, or that it didn't make one.
pub fn run_in_memory(
    node: &dyn ProcessingNode,
    params: &serde_json::Value,
    inputs: &[Image<'_>],
) -> Result<Vec<OwnedImage>, String> {
    let mut made: Vec<Option<OwnedImage>> = vec![None; node.descriptor().outputs.len()];
    let mut make = |port: u32, width, height, channels: u32| {
        let slot = made.get_mut(port as usize).filter(|slot| slot.is_none())?;
        let len = samples(width, height, channels as usize)?;
        let channels = channels as usize;
        let data = vec![0.0; len];
        let image = slot.insert(OwnedImage {
            width,
            height,
            channels,
            data,
        });
        Some((image.data.as_mut_ptr(), len))
    };
    node.run(params, inputs, &mut Outputs { make: &mut make })?;
    let made = made.into_iter().enumerate();
    made.map(|(port, image)| image.ok_or_else(|| format!("output {port} wasn't made")))
        .collect()
}

/// Lists an extension's nodes and drivers, and makes the C ABI for them
/// (ADR-0019, ADR-0036): the exported function [`abi::ENTRY`] and the table
/// it gives. Each argument is an expression giving a [`ProcessingNode`], or
/// for drivers a [`NamedDriver`] ([`camera`], [`stage`], [`illumination`]),
/// evaluated once, when the host first asks. Use it once in an extension:
///
/// ```ignore
/// oscope_ext_sdk::export!(Emboss);
/// oscope_ext_sdk::export!(nodes: [Emboss], drivers: [oscope_ext_sdk::camera("example.cam", Cam)]);
/// ```
#[macro_export]
macro_rules! export {
    (nodes: [$($node:expr),* $(,)?], drivers: [$($driver:expr),* $(,)?] $(,)?) => {
        #[doc(hidden)]
        pub struct __OscopeExtension;

        impl $crate::__private::Exported for __OscopeExtension {
            fn nodes() -> &'static [::std::boxed::Box<dyn $crate::ProcessingNode>] {
                static NODES: ::std::sync::OnceLock<
                    ::std::vec::Vec<::std::boxed::Box<dyn $crate::ProcessingNode>>,
                > = ::std::sync::OnceLock::new();
                NODES.get_or_init(|| {
                    ::std::vec![$(::std::boxed::Box::new($node) as ::std::boxed::Box<dyn $crate::ProcessingNode>),*]
                })
            }

            fn drivers() -> &'static [$crate::NamedDriver] {
                static DRIVERS: ::std::sync::OnceLock<::std::vec::Vec<$crate::NamedDriver>> =
                    ::std::sync::OnceLock::new();
                DRIVERS.get_or_init(|| ::std::vec![$($driver),*])
            }
        }

        static __OSCOPE_EXTENSION: $crate::abi::Extension =
            $crate::__private::table::<__OscopeExtension>();

        /// The extension's entry (ADR-0019), made by `oscope_ext_sdk::export!`.
        #[unsafe(no_mangle)]
        pub extern "C" fn oscope_extension() -> *const $crate::abi::Extension {
            &__OSCOPE_EXTENSION
        }
    };
    ($($node:expr),+ $(,)?) => {
        $crate::export!(nodes: [$($node),+], drivers: []);
    };
}

/// What [`export!`] expands to calls. Not part of the SDK's API.
#[doc(hidden)]
pub mod __private {
    use super::*;

    /// An extension's nodes and drivers, made once.
    pub trait Exported: 'static {
        /// The nodes, in the order the host numbers them.
        fn nodes() -> &'static [Box<dyn ProcessingNode>];
        /// The drivers, in the order the host numbers them.
        fn drivers() -> &'static [NamedDriver];
    }

    /// The table of the extension that `N` exports.
    pub const fn table<N: Exported>() -> abi::Extension {
        abi::Extension {
            abi_version: abi::ABI_VERSION,
            describe: describe::<N>,
            run: run::<N>,
            driver_call: driver_call::<N>,
            capture: capture::<N>,
            read: read::<N>,
            write: write::<N>,
        }
    }

    extern "C" fn describe<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
    ) -> abi::Status {
        guard(host, call, |host| {
            let nodes: Vec<_> = N::nodes()
                .iter()
                .map(|node| node.descriptor().to_json())
                .collect();
            let drivers = N::drivers()
                .iter()
                .map(|d| serde_json::json!({ "name": d.name, "kind": d.driver.kind() }));
            let json =
                serde_json::json!({ "nodes": nodes, "drivers": drivers.collect::<Vec<_>>() });
            (host.describe)(call, abi::Str::new(&json.to_string()));
            Ok(())
        })
    }

    /// The driver `index`.
    fn find<N: Exported>(index: u32) -> Result<&'static Driver, String> {
        let found = N::drivers().get(index as usize).map(|named| &named.driver);
        found.ok_or_else(|| "there is no such driver".to_owned())
    }

    extern "C" fn driver_call<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
        driver: u32,
        method: abi::Str,
        params: abi::Str,
    ) -> abi::Status {
        guard(host, call, |host| {
            // SAFETY: the host lends the method's name and its parameters for
            // the call (ADR-0019).
            let (method, params) = unsafe { (text(method)?, text(params)?) };
            let params = match params {
                "" => serde_json::Value::Null,
                params => {
                    serde_json::from_str(params).map_err(|e| format!("the parameters: {e}"))?
                }
            };
            let answer = find::<N>(driver)?.call(method, params)?;
            (host.reply)(call, abi::Str::new(&answer.to_string()));
            Ok(())
        })
    }

    extern "C" fn capture<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
        driver: u32,
        pixels: *mut u8,
        len: usize,
    ) -> abi::Status {
        guard(host, call, |_| {
            let pixels = match len {
                0 => &mut [][..],
                _ if pixels.is_null() => return Err("a frame without pixels".to_owned()),
                // SAFETY: the host lends `len` bytes at `pixels` for the
                // call, and touches them only after it (ADR-0036).
                _ => unsafe { std::slice::from_raw_parts_mut(pixels, len) },
            };
            find::<N>(driver)?.capture(pixels)
        })
    }

    /// Parameters as JSON: none if empty.
    ///
    /// # Safety
    ///
    /// The host lends the text for the call (ADR-0019).
    unsafe fn json(params: abi::Str) -> Result<serde_json::Value, String> {
        // SAFETY: the caller has the host's word that the text is lent for
        // the call.
        match unsafe { text(params) }? {
            "" => Ok(serde_json::Value::Null),
            params => serde_json::from_str(params).map_err(|e| format!("the parameters: {e}")),
        }
    }

    extern "C" fn read<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
        driver: u32,
        params: abi::Str,
        pixels: *mut u8,
        len: usize,
    ) -> abi::Status {
        guard(host, call, |_| {
            // SAFETY: the host lends the parameters for the call (ADR-0019).
            let params = unsafe { json(params) }?;
            let pixels = match len {
                0 => &mut [][..],
                _ if pixels.is_null() => return Err("a plane without pixels".to_owned()),
                // SAFETY: the host lends `len` bytes at `pixels` for the
                // call, and touches them only after it (ADR-0037).
                _ => unsafe { std::slice::from_raw_parts_mut(pixels, len) },
            };
            find::<N>(driver)?.read(params, pixels)
        })
    }

    extern "C" fn write<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
        driver: u32,
        params: abi::Str,
        pixels: *const u8,
        len: usize,
    ) -> abi::Status {
        guard(host, call, |_| {
            // SAFETY: the host lends the parameters for the call (ADR-0019).
            let params = unsafe { json(params) }?;
            let pixels = match len {
                0 => &[][..],
                _ if pixels.is_null() => return Err("a plane without pixels".to_owned()),
                // SAFETY: the host lends `len` bytes at `pixels` for the
                // call, and doesn't change them during it (ADR-0037).
                _ => unsafe { std::slice::from_raw_parts(pixels, len) },
            };
            find::<N>(driver)?.write(params, pixels)
        })
    }

    extern "C" fn run<N: Exported>(
        host: *const abi::Host,
        call: *mut abi::Call,
        node: u32,
        params: abi::Str,
        inputs: *const abi::ImageRef,
        count: u32,
    ) -> abi::Status {
        guard(host, call, |host| {
            let node = N::nodes()
                .get(node as usize)
                .ok_or("there is no such node")?;
            // SAFETY: the host lends the parameters for the call (ADR-0019).
            let params = unsafe { text(params) }?;
            let params =
                serde_json::from_str(params).map_err(|e| format!("the parameters: {e}"))?;
            let inputs = match count {
                0 => &[][..],
                // SAFETY: the host lends `count` images at `inputs` for the
                // call (ADR-0019).
                _ => unsafe { std::slice::from_raw_parts(inputs, count as usize) },
            };
            // SAFETY: each image's samples are lent for the call, as the
            // images are.
            let images: Vec<Image<'_>> = inputs
                .iter()
                .map(|image| unsafe { borrow(image) })
                .collect::<Result<_, _>>()?;
            let mut make = |port, width, height, channels| {
                let made = (host.output)(call, port, width, height, channels);
                let len = samples(made.width, made.height, made.channels as usize)?;
                (!made.data.is_null()).then_some((made.data, len))
            };
            node.run(&params, &images, &mut Outputs { make: &mut make })
        })
    }

    /// Runs `work` with the host's table, and turns its error or its panic
    /// into a status, telling the host why.
    fn guard(
        host: *const abi::Host,
        call: *mut abi::Call,
        work: impl FnOnce(&abi::Host) -> Result<(), String>,
    ) -> abi::Status {
        if host.is_null() {
            return abi::FAILED;
        }
        // SAFETY: the host lends its table for the call (ADR-0019).
        let host = unsafe { &*host };
        let (status, message) = if host.abi_version != abi::ABI_VERSION {
            let message = format!(
                "the host speaks the extension ABI {}, and this extension {}",
                host.abi_version,
                abi::ABI_VERSION
            );
            (abi::FAILED, message)
        } else {
            match panic::catch_unwind(AssertUnwindSafe(|| work(host))) {
                Ok(Ok(())) => return abi::OK,
                Ok(Err(message)) => (abi::FAILED, message),
                Err(panic) => (abi::PANICKED, said(panic.as_ref())),
            }
        };
        (host.fail)(call, abi::Str::new(&message));
        status
    }

    /// What a panic said.
    fn said(panic: &(dyn Any + Send)) -> String {
        let text = panic.downcast_ref::<&str>().map(|text| (*text).to_owned());
        let text = text.or_else(|| panic.downcast_ref::<String>().cloned());
        text.unwrap_or_else(|| "a panic that said nothing".to_owned())
    }

    /// The text `text` points to.
    ///
    /// # Safety
    ///
    /// `text` must point to `len` bytes, alive while the result is.
    unsafe fn text<'a>(text: abi::Str) -> Result<&'a str, String> {
        if text.len == 0 {
            return Ok("");
        }
        // SAFETY: the caller's promise.
        let bytes = unsafe { std::slice::from_raw_parts(text.data, text.len) };
        std::str::from_utf8(bytes).map_err(|_| "text that isn't UTF-8".to_owned())
    }

    /// The image `image` points to.
    ///
    /// # Safety
    ///
    /// `image.data` must point to its samples, alive while the result is.
    unsafe fn borrow<'a>(image: &abi::ImageRef) -> Result<Image<'a>, String> {
        let channels = image.channels as usize;
        let len = samples(image.width, image.height, channels).ok_or("an image too large")?;
        let data = match len {
            0 => &[][..],
            _ if image.data.is_null() => return Err("an image without samples".to_owned()),
            // SAFETY: the caller's promise.
            _ => unsafe { std::slice::from_raw_parts(image.data, len) },
        };
        Image::new(image.width, image.height, channels, data)
    }
}
