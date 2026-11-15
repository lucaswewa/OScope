//! Rust extensions' libraries: loaded, checked, and their nodes made into
//! the pipeline's (ADR-0019).

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_ext_sdk::abi;
use oscope_pipeline::{
    Descriptor, Inputs, Node, NodeError, PortType, Registry, Step, StepCtx, Value, Values,
};

use crate::ExtError;

/// The most samples an output may have: 2³⁰, 4 GiB of floats.
pub const MAX_SAMPLES: usize = 1 << 30;

/// The host's table, which every call borrows.
static HOST: abi::Host = abi::Host {
    abi_version: abi::ABI_VERSION,
    describe,
    output,
    fail,
    reply,
};

/// What the host keeps for a call to an extension.
#[derive(Default)]
struct Call {
    description: Option<String>,
    outputs: Vec<Option<Image>>,
    failure: Option<String>,
    answer: Option<String>,
}

/// A driver an extension describes (ADR-0036).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DriverInfo {
    /// Its name, which the configuration names it by.
    pub name: String,
    /// Its kind: `camera`, `stage` or `illumination`.
    pub kind: String,
}

/// What an extension describes.
#[derive(serde::Deserialize)]
struct Description {
    nodes: Vec<Descriptor>,
    drivers: Vec<DriverInfo>,
}

/// A Rust extension's library, loaded: its table, and the nodes and drivers
/// it describes. A library is never unloaded: a Rust library's statics and
/// thread-locals can't be torn down safely while the process runs.
pub struct Library {
    table: &'static abi::Extension,
    descriptors: Vec<Descriptor>,
    drivers: Vec<DriverInfo>,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<_> = self.descriptors.iter().map(|d| &d.id).collect();
        f.debug_struct("Library").field("nodes", &ids).finish()
    }
}

impl Library {
    /// Loads the library at `path`, checks its ABI version, and reads what
    /// nodes it has.
    pub fn load(path: &Path) -> Result<Arc<Library>, ExtError> {
        let failed = |why: String| ExtError::Library {
            path: path.to_owned(),
            why,
        };
        // SAFETY: loading a library runs its initialisers: extensions are
        // trusted code (ADR-0018).
        let library = unsafe { libloading::Library::new(path) };
        let library = Box::leak(Box::new(library.map_err(|e| failed(e.to_string()))?));
        // SAFETY: the entry has this type in every version of the ABI.
        let entry = unsafe { library.get::<abi::Entry>(abi::ENTRY) };
        let entry = *entry.map_err(|e| failed(format!("it has no `oscope_extension`: {e}")))?;
        Library::from_entry(entry).map_err(failed)
    }

    /// The library whose entry is `entry`, already in this process: what
    /// [`load`](Self::load) does once it has the entry.
    pub fn from_entry(entry: abi::Entry) -> Result<Arc<Library>, String> {
        let table = entry();
        if table.is_null() {
            return Err("its entry gave no table".to_owned());
        }
        // SAFETY: every version's table starts with its version, a `u32`.
        let version = unsafe { *table.cast::<u32>() };
        if version != abi::ABI_VERSION {
            return Err(format!(
                "it was built for the extension ABI {version}, and this OScope speaks {}",
                abi::ABI_VERSION
            ));
        }
        // SAFETY: a table of this version, which lives as long as the
        // library, which is never unloaded.
        let table: &'static abi::Extension = unsafe { &*table };
        let described = call(0, |host, call| (table.describe)(host, call))?;
        let json = described.description.ok_or("it described nothing")?;
        let Description {
            nodes: descriptors,
            drivers,
        } = serde_json::from_str(&json).map_err(|e| format!("its description: {e}"))?;
        check(&descriptors, "in ABI 3")?;
        let kinds = [
            "camera",
            "stage",
            "illumination",
            "importer",
            "exporter",
            "workflow",
        ];
        if let Some(driver) = drivers.iter().find(|d| !kinds.contains(&d.kind.as_str())) {
            return Err(format!(
                "driver `{}`: no driver is a {}",
                driver.name, driver.kind
            ));
        }
        let library = Arc::new(Library {
            table,
            descriptors,
            drivers,
        });
        registered(&library.nodes())?;
        Ok(library)
    }

    /// What its nodes are, in its order.
    pub fn descriptors(&self) -> &[Descriptor] {
        &self.descriptors
    }

    /// Its drivers, in its order.
    pub fn drivers(&self) -> &[DriverInfo] {
        &self.drivers
    }

    /// Calls its driver `driver`'s method `method` with `params`: its answer.
    pub fn driver_call(
        &self,
        driver: u32,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (method, params) = (abi::Str::new(method), params.to_string());
        let table = self.table;
        let made = call(0, |host, call| {
            (table.driver_call)(host, call, driver, method, abi::Str::new(&params))
        })?;
        let answer = made.answer.ok_or("it gave no answer")?;
        serde_json::from_str(&answer).map_err(|e| format!("its answer: {e}"))
    }

    /// Captures its camera `driver`'s next frame into `pixels`.
    pub fn capture(&self, driver: u32, pixels: &mut [u8]) -> Result<(), String> {
        let table = self.table;
        let (data, len) = (pixels.as_mut_ptr(), pixels.len());
        call(0, |host, call| {
            (table.capture)(host, call, driver, data, len)
        })
        .map(drop)
    }

    /// Reads into `pixels` what `params` ask of its importer `driver`
    /// (ADR-0037).
    pub fn read(
        &self,
        driver: u32,
        params: &serde_json::Value,
        pixels: &mut [u8],
    ) -> Result<(), String> {
        let (table, params) = (self.table, params.to_string());
        let (data, len) = (pixels.as_mut_ptr(), pixels.len());
        call(0, |host, call| {
            (table.read)(host, call, driver, abi::Str::new(&params), data, len)
        })
        .map(drop)
    }

    /// Gives its exporter `driver` the `pixels`, as `params` say (ADR-0037).
    pub fn write(
        &self,
        driver: u32,
        params: &serde_json::Value,
        pixels: &[u8],
    ) -> Result<(), String> {
        let (table, params) = (self.table, params.to_string());
        let (data, len) = (pixels.as_ptr(), pixels.len());
        call(0, |host, call| {
            (table.write)(host, call, driver, abi::Str::new(&params), data, len)
        })
        .map(drop)
    }

    /// Its nodes, for the pipeline's registry.
    pub fn nodes(self: &Arc<Self>) -> Vec<Arc<dyn Node>> {
        let nodes = self.descriptors.iter().zip(0..).map(|(descriptor, index)| {
            let node = NativeNode {
                library: Arc::clone(self),
                index,
                descriptor: descriptor.clone(),
            };
            Arc::new(node) as Arc<dyn Node>
        });
        nodes.collect()
    }
}

/// Checks what extensions' nodes may be so far, `rule` saying where:
/// every port an image, none optional, and the CPU alone.
pub(crate) fn check(descriptors: &[Descriptor], rule: &str) -> Result<(), String> {
    for descriptor in descriptors {
        let id = &descriptor.id;
        let mut ports = descriptor.inputs.iter().chain(&descriptor.outputs);
        if !ports.all(|port| port.kind == PortType::Image && !port.optional) {
            return Err(format!(
                "node `{id}`: {rule}, every port is an image, and none is optional"
            ));
        }
        if !descriptor.capabilities.cpu || descriptor.capabilities.cuda {
            return Err(format!("node `{id}`: {rule}, nodes run on the CPU"));
        }
    }
    Ok(())
}

/// Checks `nodes` as the pipeline's registry checks every node (ADR-0017).
pub(crate) fn registered(nodes: &[Arc<dyn Node>]) -> Result<(), String> {
    let mut registry = Registry::new();
    nodes
        .iter()
        .try_for_each(|node| registry.register(Arc::clone(node)))
}

/// Calls the extension: `work`, given the host's table and a new call
/// expecting `outputs` outputs. The call afterwards, or why it failed.
fn call(
    outputs: usize,
    work: impl FnOnce(&abi::Host, *mut abi::Call) -> abi::Status,
) -> Result<Call, String> {
    let mut call = Call {
        outputs: (0..outputs).map(|_| None).collect(),
        ..Call::default()
    };
    let status = work(&HOST, (&raw mut call).cast());
    let failure = call.failure.take();
    match status {
        abi::OK => Ok(call),
        abi::FAILED => Err(failure.unwrap_or_else(|| "it failed, and didn't say why".to_owned())),
        abi::PANICKED => Err(format!("it panicked: {}", failure.unwrap_or_default())),
        status => Err(format!("it gave the unknown status {status}")),
    }
}

/// Does `work` on the call `call`, catching its panics: no panic may cross
/// back into the extension.
///
/// # Safety
///
/// `call` must be the pointer the host lent for a call in progress.
unsafe fn with_call<T>(call: *mut abi::Call, work: impl FnOnce(&mut Call) -> T) -> Option<T> {
    if call.is_null() {
        return None;
    }
    // SAFETY: the caller's promise: the `Call` the host made for this call,
    // which nothing else touches while the extension runs.
    let call = unsafe { &mut *call.cast::<Call>() };
    panic::catch_unwind(AssertUnwindSafe(|| work(call))).ok()
}

/// A copy of the text `text`.
///
/// # Safety
///
/// `text` must point to `len` bytes, alive for the call.
unsafe fn copy(text: abi::Str) -> String {
    if text.len == 0 || text.data.is_null() {
        return String::new();
    }
    // SAFETY: the caller's promise.
    let bytes = unsafe { std::slice::from_raw_parts(text.data, text.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

extern "C" fn describe(call: *mut abi::Call, json: abi::Str) {
    // SAFETY: the extension passes back the call and the text the host and
    // it lent for the call (ADR-0019).
    unsafe { with_call(call, |call| call.description = Some(copy(json))) };
}

extern "C" fn fail(call: *mut abi::Call, message: abi::Str) {
    // SAFETY: as in `describe`.
    unsafe { with_call(call, |call| call.failure = Some(copy(message))) };
}

extern "C" fn reply(call: *mut abi::Call, json: abi::Str) {
    // SAFETY: as in `describe`.
    unsafe { with_call(call, |call| call.answer = Some(copy(json))) };
}

extern "C" fn output(
    call: *mut abi::Call,
    port: u32,
    width: u32,
    height: u32,
    channels: u32,
) -> abi::ImageMut {
    let make = |call: &mut Call| {
        let slot = call
            .outputs
            .get_mut(port as usize)
            .filter(|slot| slot.is_none())?;
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(channels as usize)?;
        if channels == 0 || len > MAX_SAMPLES {
            return None;
        }
        let image = slot.insert(Image::zeros(Shape::new(width, height, channels as usize)));
        Some(image.data_mut().as_mut_ptr())
    };
    // SAFETY: as in `describe`.
    let data = unsafe { with_call(call, make) }.flatten();
    abi::ImageMut {
        data: data.unwrap_or(std::ptr::null_mut()),
        width,
        height,
        channels,
    }
}

/// A node of a Rust extension, as the pipeline's registry holds it.
struct NativeNode {
    library: Arc<Library>,
    index: u32,
    descriptor: Descriptor,
}

impl Node for NativeNode {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    fn prepare(&self, params: &serde_json::Value) -> Result<Box<dyn Step>, String> {
        let names =
            |ports: &[oscope_pipeline::Port]| ports.iter().map(|p| p.name.clone()).collect();
        Ok(Box::new(NativeStep {
            library: Arc::clone(&self.library),
            index: self.index,
            params: params.to_string(),
            inputs: names(&self.descriptor.inputs),
            outputs: names(&self.descriptor.outputs),
        }))
    }
}

/// A node of a Rust extension with its parameters, which it passes as JSON
/// text.
struct NativeStep {
    library: Arc<Library>,
    index: u32,
    params: String,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

impl Step for NativeStep {
    fn run(&self, _: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        let images = self.inputs.iter().map(|port| inputs.image(port));
        let images = images.collect::<Result<Vec<&Image>, _>>()?;
        let views = images.iter().map(|image| {
            let channels = u32::try_from(image.channels()).map_err(|_| "too many channels")?;
            Ok(abi::ImageRef {
                data: image.data().as_ptr(),
                width: image.width(),
                height: image.height(),
                channels,
            })
        });
        let views = views.collect::<Result<Vec<_>, NodeError>>()?;
        let table = self.library.table;
        let (params, count) = (abi::Str::new(&self.params), views.len() as u32);
        let ran = call(self.outputs.len(), |host, call| {
            (table.run)(host, call, self.index, params, views.as_ptr(), count)
        })?;
        let made = self.outputs.iter().zip(ran.outputs).map(|(port, image)| {
            let image = image.ok_or_else(|| format!("it made no output `{port}`"))?;
            Ok((port.clone(), Value::image(image)))
        });
        made.collect()
    }
}
