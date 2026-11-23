//! Nodes: what each declares, and the steps they make from their parameters.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "cuda")]
use oscope_compute::cuda::{Cuda, DeviceImage};
use oscope_compute::{Compute, ComputeError, Image, Lane};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::value::{Mask, PortType, Value, Values};

/// Where a step runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    /// The CPU, in the run's pool.
    Cpu,
    /// The GPU, with CUDA.
    Cuda,
}

/// A node's input or output, or a recipe's: a name and a type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Port {
    /// Its name: letters, digits and `_`.
    pub name: String,
    /// Its values' type.
    #[serde(rename = "type")]
    pub kind: PortType,
    /// For a node's input: whether it may be left without an edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// What a node can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// It runs on the CPU.
    pub cpu: bool,
    /// It runs on the GPU, with CUDA.
    pub cuda: bool,
    /// It is quick enough to run on every frame of the live view.
    pub live: bool,
    /// It can run tile by tile: each output pixel depends only on the input
    /// pixels within its halo, and its outputs are rasters of its raster
    /// inputs' size.
    pub tileable: bool,
    /// The pixels around a tile it reads, unless its step says
    /// ([`Step::halo`]).
    pub halo: u32,
}

/// What a node is (ADR-0017), as the registry lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Descriptor {
    /// Its ID, dotted: `test.box_blur`.
    pub id: String,
    /// Its version, raised when what it gives, or its parameters, change.
    pub version: u32,
    /// Its category, for people: `filter`.
    pub category: String,
    /// What it does, in a line.
    pub description: String,
    /// Its inputs.
    pub inputs: Vec<Port>,
    /// Its outputs.
    pub outputs: Vec<Port>,
    /// Its parameters' JSON Schema, in the part [`crate::schema`] knows.
    pub params: serde_json::Value,
    /// What it can do.
    pub capabilities: Capabilities,
}

impl Descriptor {
    /// A node with no ports, no parameters and no capabilities yet.
    pub fn new(id: &str, version: u32, category: &str, description: &str) -> Descriptor {
        Descriptor {
            id: id.to_owned(),
            version,
            category: category.to_owned(),
            description: description.to_owned(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: serde_json::json!({ "type": "object", "additionalProperties": false }),
            capabilities: Capabilities::default(),
        }
    }

    fn port(name: &str, kind: PortType, optional: bool) -> Port {
        Port {
            name: name.to_owned(),
            kind,
            optional,
        }
    }

    /// With an input.
    pub fn input(mut self, name: &str, kind: PortType) -> Self {
        self.inputs.push(Self::port(name, kind, false));
        self
    }

    /// With an input that may be left without an edge.
    pub fn optional_input(mut self, name: &str, kind: PortType) -> Self {
        self.inputs.push(Self::port(name, kind, true));
        self
    }

    /// With an output.
    pub fn output(mut self, name: &str, kind: PortType) -> Self {
        self.outputs.push(Self::port(name, kind, false));
        self
    }

    /// With the parameters `P`, their schema generated from it.
    pub fn params<P: JsonSchema>(mut self) -> Self {
        self.params = schemars::schema_for!(P).to_value();
        self
    }

    /// With these capabilities.
    pub fn capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// A kind of node, as the registry holds it (ADR-0017).
///
/// Nodes are pure: a step's outputs depend only on its inputs and its
/// parameters, so the engine may give a memoised result instead of running
/// it.
pub trait Node: Send + Sync + 'static {
    /// What it is.
    fn descriptor(&self) -> &Descriptor;

    /// Its step for `params`, which have passed its schema: once for each
    /// plan, so the work parameters need is done once.
    fn prepare(&self, params: &serde_json::Value) -> Result<Box<dyn Step>, String>;
}

/// A node with its parameters, ready to run: once, for each tile, or for
/// each frame, perhaps on several threads at once.
pub trait Step: Send + Sync {
    /// Runs it, its inputs on `ctx.device()`, by port; its outputs by port,
    /// on the same device or in memory.
    fn run(&self, ctx: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError>;

    /// The pixels around a tile it reads, when its parameters set them: the
    /// descriptor's otherwise.
    fn halo(&self) -> Option<u32> {
        None
    }

    /// What its outputs depend on beyond its parameters and inputs, such as
    /// a file's content, as part of its memo key.
    fn key(&self) -> Option<u64> {
        None
    }
}

impl<F> Step for F
where
    F: Fn(&StepCtx<'_>, &Inputs<'_>) -> Result<Values, NodeError> + Send + Sync,
{
    fn run(&self, ctx: &StepCtx<'_>, inputs: &Inputs<'_>) -> Result<Values, NodeError> {
        self(ctx, inputs)
    }
}

/// A node of `descriptor`, whose steps `make` makes from its parameters, `P`.
pub fn node<P, S>(
    descriptor: Descriptor,
    make: impl Fn(P) -> S + Send + Sync + 'static,
) -> Arc<dyn Node>
where
    P: DeserializeOwned,
    S: Step + 'static,
{
    try_node(descriptor, move |params: P| Ok(make(params)))
}

/// A node of `descriptor`, whose steps `make` makes from its parameters,
/// `P`, or refuses them, for what their schema can't say.
pub fn try_node<P, S>(
    descriptor: Descriptor,
    make: impl Fn(P) -> Result<S, String> + Send + Sync + 'static,
) -> Arc<dyn Node>
where
    P: DeserializeOwned,
    S: Step + 'static,
{
    struct Made<F> {
        descriptor: Descriptor,
        make: F,
    }
    impl<F> Node for Made<F>
    where
        F: Fn(&serde_json::Value) -> Result<Box<dyn Step>, String> + Send + Sync + 'static,
    {
        fn descriptor(&self) -> &Descriptor {
            &self.descriptor
        }
        fn prepare(&self, params: &serde_json::Value) -> Result<Box<dyn Step>, String> {
            (self.make)(params)
        }
    }
    Arc::new(Made {
        descriptor,
        make: move |params: &serde_json::Value| {
            let params = P::deserialize(params).map_err(|error| error.to_string())?;
            Ok(Box::new(make(params)?) as Box<dyn Step>)
        },
    })
}

/// Why a step gave no outputs.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NodeError {
    /// It failed, for this reason.
    #[error("{0}")]
    Failed(String),
    /// It stopped, as the run was cancelled.
    #[error("cancelled")]
    Cancelled,
}

impl From<String> for NodeError {
    fn from(reason: String) -> Self {
        NodeError::Failed(reason)
    }
}

impl From<&str> for NodeError {
    fn from(reason: &str) -> Self {
        NodeError::Failed(reason.to_owned())
    }
}

impl From<oscope_imaging::color::ColorError> for NodeError {
    fn from(error: oscope_imaging::color::ColorError) -> Self {
        NodeError::Failed(error.to_string())
    }
}

impl From<ComputeError> for NodeError {
    fn from(error: ComputeError) -> Self {
        NodeError::Failed(error.to_string())
    }
}

/// What a running step may use.
pub struct StepCtx<'a> {
    pub(crate) compute: &'a Compute,
    pub(crate) device: Device,
    pub(crate) lane: Lane,
    pub(crate) cancel: &'a AtomicBool,
}

impl StepCtx<'_> {
    /// The compute layer: its kernels and pools. The step already runs in
    /// its lane's pool.
    pub fn compute(&self) -> &Compute {
        self.compute
    }

    /// Where it runs: its images are there.
    pub fn device(&self) -> Device {
        self.device
    }

    /// Its lane: live or batch.
    pub fn lane(&self) -> Lane {
        self.lane
    }

    /// The GPU, for a step running there.
    #[cfg(feature = "cuda")]
    pub fn cuda(&self) -> Result<&Cuda, NodeError> {
        self.compute.cuda().ok_or_else(|| "there is no GPU".into())
    }

    /// Whether the run was cancelled: a long step should stop, with
    /// [`NodeError::Cancelled`].
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

/// A step's inputs, by port.
pub struct Inputs<'a> {
    pub(crate) ports: &'a [Port],
    pub(crate) values: &'a [Option<Value>],
}

impl<'a> Inputs<'a> {
    /// The value on input `port`: `None` for an optional input without an
    /// edge.
    pub fn get(&self, port: &str) -> Option<&'a Value> {
        let index = self.ports.iter().position(|p| p.name == port)?;
        self.values.get(index)?.as_ref()
    }

    fn need<T>(
        &self,
        port: &str,
        what: &str,
        pick: impl Fn(&'a Value) -> Option<T>,
    ) -> Result<T, NodeError> {
        let value = self
            .get(port)
            .ok_or_else(|| format!("no value on input `{port}`"))?;
        pick(value).ok_or_else(|| format!("input `{port}` isn't {what}").into())
    }

    /// The image in memory on input `port`.
    pub fn image(&self, port: &str) -> Result<&'a Image, NodeError> {
        self.need(port, "an image in memory", Value::as_image)
    }

    /// The image on the GPU on input `port`.
    #[cfg(feature = "cuda")]
    pub fn device_image(&self, port: &str) -> Result<&'a DeviceImage, NodeError> {
        self.need(port, "an image on the GPU", |value| match value {
            Value::DeviceImage(image) => Some(&**image),
            _ => None,
        })
    }

    /// The stack on input `port`.
    pub fn stack(&self, port: &str) -> Result<&'a [Image], NodeError> {
        self.need(port, "a stack", Value::as_stack)
    }

    /// The mask on input `port`.
    pub fn mask(&self, port: &str) -> Result<&'a Mask, NodeError> {
        self.need(port, "a mask", Value::as_mask)
    }

    /// The JSON on input `port`.
    pub fn json(&self, port: &str) -> Result<&'a serde_json::Value, NodeError> {
        self.need(port, "JSON", Value::as_json)
    }
}
