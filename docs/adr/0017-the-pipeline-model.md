# ADR-0017: The pipeline model

- Status: Accepted
- Date: 2026-11-13
- Phase: P21

## Context

- **D12 and plan §5.7:** processing is a DAG of typed nodes, as a JSON recipe. It runs per frame (live) or in batch (over datasets, tile by tile for large mosaics). Intermediate results are memoised, so editing one node in M2's graph editor re-runs only what changed.
- **Plan §5.6:** the engine keeps data on the GPU between consecutive CUDA nodes, and copies it only at the boundaries.
- **Nodes come from many places:** OScope's stock nodes (P22–P23, P30–P48), and extensions (ADR-0018 onwards), Rust ones through a C ABI and Python ones through a worker. All of them describe their parameters as a JSON Schema, which the engine must check.
- **Batch work runs as jobs** (ADR-0016): queued, followed, cancelled.
- **Live work runs in the live pool** (ADR-0010), so batch work can't hold up the preview.
- **The `jsonschema` crate** would check parameters, but brings about 30 crates. Each would need the licence and two-week checks, now and at every update.

## Decision

**1. A node** (`Node`) has a descriptor:
- **an ID,** dotted (`test.box_blur`), **a version,** an integer raised when what it gives or its parameters change, **a category** and **a description**;
- **inputs and outputs:** named, typed ports. An input may be optional;
- **its parameters' JSON Schema;**
- **its capabilities:** the CPU and or CUDA; live; tileable; and its halo in pixels.

From parameters its schema has passed, a node makes a **step** (`Step`), once for each plan. The step runs: once, for each tile, or for each frame. When its halo depends on its parameters, as a blur's radius, the step gives it. **Nodes are pure:** a step's outputs depend only on its inputs and parameters.

**2. Port types:** image (32-bit floats, in memory or on the GPU), stack, mask (a byte a pixel), table (named columns, rows of JSON values), geometry (GeoJSON), JSON, and report. Values are immutable and shared.

**3. Parameters** are checked by OScope's own validator, for the part of JSON Schema 2020-12 that nodes use:
- `type`, `enum` and `const`;
- the numeric bounds and `multipleOf`;
- string lengths;
- `items`, `prefixItems` and the array bounds;
- `properties`, `required`, `additionalProperties` and the member counts;
- `allOf`, `anyOf`, `oneOf`, `not`, and `$ref` within the schema.

Annotations are ignored. A node whose schema uses any other keyword, such as `pattern`, is refused when it is registered, so nothing is ignored silently. Rust nodes generate their schemas with `schemars`.

**4. The registry** holds a server's nodes, filled at start-up as `teta-wot`'s Things are. It refuses malformed descriptors:
- IDs and port names that aren't names;
- ports named twice;
- no outputs;
- neither the CPU nor CUDA;
- tileable with an output that isn't a raster.

**5. A recipe** is JSON, format 1:
- `inputs`, named and typed;
- `nodes`, each with an ID, the node, optionally the node's version, and its parameters;
- `edges` from `node.port` to `node.port`, a recipe input being `input.<name>`;
- `outputs`, each a name and the `node.port` it gives.

Unknown fields are refused. A recipe that names a node's version must match the registry's.

**6. A plan** is a recipe checked against the registry, ordered and placed.
- **The checks** report every problem, with where it is:
  - the format;
  - unknown nodes, other versions, parameters the schema refuses;
  - edges from or to ports that aren't there, or between different types;
  - an input fed twice, or a required input not fed;
  - names;
  - cycles, named in the direction data flows;
  - nodes that run only on a GPU, where there is none.
- **The order** is Kahn's topological sort, ties in the recipe's order. Only the steps the outputs need are planned.
- **Placement:**
  - A step whose node has a CUDA step runs on the GPU, when there is one. A node that runs on the CPU too stays there when it would be alone on the GPU, with everything it reads and everything reading it on the CPU.
  - An image crosses between memory and the GPU only where a step reads it on the other side, once for each side.
  - Recipe inputs and outputs are in memory.
- **The halo** a tile reads is the largest sum of the steps' halos along a path from an input to an output.

**7. Running** (`Engine::run`), in the lane's pool:
- **Each step starts as soon as the steps it reads have run,** so independent branches run in parallel. A failure or a cancel starts no more steps.
- **A step's panic** is that step's failure.
- **A value is dropped** once no step will read it again, unless it is an output.

**8. Memoisation:**
- **A step's key** is a hash of its node, version, parameters (members in name order), device, and its inputs' keys.
- **A recipe input's key** is a hash of its content.
- **The cache** keeps steps' outputs in memory, least recently used first out past a byte budget.
- **Not memoised:** outputs on the GPU, as holding GPU memory there would take it from jobs' GPU budget. Live mode never memoises.

**9. The modes:**
- **Whole:** `Engine::run`.
- **Tiled** (`Engine::run_tiled`): the raster inputs cut into tiles, each read with the plan's halo; each tile run as a whole plan; the cores put together. Every step must be tileable. Other inputs go whole to each tile. For nodes that honour their halo, it gives what a whole run does, to the bit.
- **Live** (`Live`):
  - one frame at a time, in the live lane, not memoised;
  - frames don't queue: a frame still waiting when a newer one comes is dropped for it, and one older than the budget when its turn comes is dropped as late;
  - a result that took longer than the budget is still given, and counted;
  - every step must be live.
- **Batch** (`Engine::batch_job`): a compute job over a dataset.
  - Its items are read and written by the caller's functions.
  - Its progress counts items, and tiles within one.
  - It pauses between items. A cancel stops it before its next step, and inside steps that look.

## Consequences

- **Gains:**
  - One model for every kind of processing, from the live view to whole-slide analysis, and for extensions' nodes.
  - Recipes say everything wrong with them at once, and where, for M2's editor.
  - An edit re-runs only the steps after it.
  - Data stays on the GPU between GPU steps.
  - A slow branch doesn't hold up an independent one.
- **Costs:**
  - **Parallel steps' times overlap:** while a step waits on its own threads, rayon may give them the other branch's work.
  - **Steps on the GPU aren't memoised,** so an edit after them re-runs them too, on the GPU.
  - **The validator knows a subset of JSON Schema.** An extension needing more must wait for it to grow.
  - **Hashing a run's inputs costs time:** about 0.6 ms for a 1024 × 768 RGB image, on the batch pool.
  - **In P21, tiled runs read from and write to images in memory.** P28's store and P39's mosaics will feed them tile by tile.
- **To watch:**
  - **A cost model for placement:** measured step and copy times could keep short GPU runs on the CPU.
  - **Memoising GPU outputs,** within a GPU budget of the cache's own.
  - **The server's interface to recipes:** a job kind and a Thing, when P22's live recipes and M2's editor need them.

## Alternatives considered

- **The `jsonschema` crate:** complete, but about 30 crates for small parameter objects. A subset, with unknown keywords refused, keeps the checks strict.
- **Running the plan level by level,** each level's steps in parallel: simpler, but a slow step holds up independent branches at every level.
- **A coordinator thread waiting on a channel:** it blocks a pool thread, and can deadlock a one-thread pool. Rayon's scope spawns each step from its last producer, without blocking.
- **A placement that minimises copies alone:** it would put everything on the CPU, as recipe inputs and outputs are in memory.
- **Cutting each step's inputs to its own halo** in tiled runs: fewer pixels computed, but each step would need tile geometry. Running the whole plan per tile, with the summed halo, works for any tileable node.
- **Dropping over-budget results in live mode:** a recipe always slower than its budget would show nothing. Giving them, counted, lets the live view say so.
