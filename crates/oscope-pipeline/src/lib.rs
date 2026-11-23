//! The processing pipeline: recipes of typed nodes, run live or in batch
//! (D12, plan §5.7).
//!
//! Layer L3 of ADR-0005. ADR-0017 records the design:
//!
//! - **A node** ([`Node`]) declares its ID, version and category, its typed
//!   ports, its parameters as a JSON Schema, and what it can do: the CPU
//!   and CUDA, live, tile by tile with a halo ([`Descriptor`]). From its
//!   parameters it makes a [`Step`], which runs.
//! - **The registry** ([`Registry`]) holds the nodes a server has.
//! - **A recipe** ([`Recipe`]) is versioned JSON: nodes, the edges between
//!   their ports, and the recipe's own inputs and outputs. A [`Plan`] is a
//!   recipe checked against the registry, for types, cycles, missing inputs
//!   and parameters, with its steps ordered and placed on the CPU or the
//!   GPU.
//! - **The engine** ([`Engine`]) runs plans: each step once the steps it
//!   reads have run, so independent branches run in parallel, and steps'
//!   results memoised by a hash of their inputs and parameters. It runs
//!   them whole, tile by tile with halos ([`Engine::run_tiled`]), on every
//!   frame within a latency budget ([`Live`]), or over a dataset as a
//!   compute job ([`Engine::batch_job`]).
//! - **[`test_nodes`]** are trivial nodes, for tests and examples.
//!
//! ```
//! use std::sync::Arc;
//!
//! use oscope_compute::{Compute, ComputeConfig, Image, Shape};
//! use oscope_pipeline::{Engine, Recipe, RunOptions, Value, Values, test_nodes};
//!
//! let compute = Arc::new(Compute::new(&ComputeConfig::default())?);
//! let engine = Engine::new(test_nodes::registry(), compute, 64 << 20);
//! let recipe = Recipe::from_json(
//!     r#"{
//!         "format": 1,
//!         "inputs": [{ "name": "image", "type": "image" }],
//!         "nodes": [{ "id": "blur", "node": "test.box_blur", "params": { "radius": 2 } }],
//!         "edges": [{ "from": "input.image", "to": "blur.image" }],
//!         "outputs": [{ "name": "blurred", "from": "blur.image" }]
//!     }"#,
//! )?;
//! let plan = engine.plan(&recipe)?;
//! let inputs = Values::from([("image".to_owned(), Value::image(Image::zeros(Shape::new(64, 48, 3))))]);
//! let run = engine.run(&plan, &inputs, &RunOptions::default())?;
//! assert_eq!(run.outputs["blurred"].as_image().map(Image::width), Some(64));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod batch;
pub mod library;
mod live;
mod memo;
mod node;
mod plan;
mod recipe;
mod registry;
mod run;
pub mod schema;
pub mod stock;
pub mod test_nodes;
mod tile;
mod value;

pub use batch::{Batch, ReadItem, WriteItem};
pub use library::{Kept, Library, LibraryError, Listed, check};
pub use live::{Live, LiveResult, LiveStats};
pub use memo::MemoStats;
pub use node::{
    Capabilities, Descriptor, Device, Inputs, Node, NodeError, Port, Step, StepCtx, node, try_node,
};
pub use plan::Plan;
pub use recipe::{Edge, FORMAT, INPUT, NodeUse, Problem, Recipe, RecipeError, RecipeOutput};
pub use registry::Registry;
pub use run::{Engine, Report, Run, RunError, RunOptions, StepReport};
pub use tile::{Tile, tiles};
pub use value::{Mask, PortType, Table, Value, Values, outputs};
