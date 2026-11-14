# ADR-0019: The Rust extension ABI

- Status: Accepted; extended by [ADR-0036](0036-extension-drivers-and-panels.md) and [ADR-0037](0037-importers-exporters-and-the-abi-s-third-version.md)
- Date: 2026-11-14
- Phase: P25

## Context

- **D9 and plan §5.8:** third parties write processing nodes in Rust. OScope loads them in its own process, as native libraries (`cdylib`), and runs them as fast as its own nodes. Extensions are trusted code (ADR-0018).
- **Rust has no stable ABI.** A library built with another compiler, or another version of a crate, can't share Rust types with the server. Only the C ABI is stable.
- **Authors should write safe Rust,** with no `unsafe` of their own. CLAUDE.md allows unsafe code in `oscope-ext` and `oscope-ext-sdk` (the generated glue), not in an extension's crate.
- **Since Rust 1.81, a panic that unwinds out of an `extern "C"` function aborts the process.** A node's panic must be caught before it reaches the boundary.
- **The pipeline (ADR-0017)** runs a node's step from several threads at once, tile by tile or frame by frame, and checks its descriptor and parameters itself.

## Decision

**1. The boundary is C.** `oscope-ext-sdk`'s `abi` module defines it, version **1**:

- **`oscope_extension`,** the only exported function, gives a pointer to the extension's table. The table lives as long as the library.
- **The extension's table** (`Extension`): `abi_version: u32`, then `describe` and `run`.
- **The host's table** (`Host`), lent to every call: `abi_version: u32`, then `describe` (takes the description), `output` (makes an output's image) and `fail` (takes why a call failed).
- **Images** (`ImageRef`, `ImageMut`): a pointer to `f32` samples and `width`, `height` and `channels` as `u32`. Pixels are interleaved, and rows follow each other without gaps, as OScope's `Image`.
- **Text** (`Str`): a pointer to UTF-8 bytes and a length, not NUL-terminated.
- **A call's context** (`Call`): an opaque pointer that the host passes in, and the extension passes back to the host's functions.
- **Statuses:** `OK` 0, `FAILED` 1, `PANICKED` 2.

Every table starts with its version, so that a host reads it whatever the version. **The host loads only a library of its own version.** Any change to these layouts is a new version. The layouts are pinned by a test: sizes, alignments and offsets on x86_64 (ADR-0003).

**2. Ownership: the host owns every buffer.** Inputs are the host's images, lent for the call. Outputs are allocated by the host through `output`, filled by the extension, and kept by the host. Text goes both ways only as borrowed `Str`: the receiver copies what it keeps. Nothing is freed across the boundary, so the two sides' allocators never meet. An output may hold at most 2³⁰ samples. Each is made once.

**3. Calls:**
- **`describe`** gives the extension's nodes as JSON, through `Host::describe`. The format is the pipeline's descriptor (ADR-0017). In ABI 1, every port is an image and none is optional, and nodes run on the CPU alone.
- **`run`** runs node *n*, by its place in the description, with its parameters as JSON text. The host has already checked them against the node's schema. The node's inputs come in its descriptor's order, and it makes every output through `Host::output`. **It may be called from several threads at once:** nodes are `Send + Sync`.

**4. Errors and panics.** A function that fails tells the host why through `Host::fail`, then returns `FAILED`. The SDK catches a node's panic, says where through `fail`, and returns `PANICKED`. The host's functions catch their own panics. So no panic crosses the boundary either way. The host turns both statuses into the step's error: *it panicked: …*. **Extensions must be built with `panic = "unwind"`,** Cargo's default. A library that aborts on panic, or that crashes, ends the server, which its service manager starts again (ADR-0015).

**5. The SDK** (`oscope-ext-sdk`, depending on `serde_json` alone):
- **`ProcessingNode`:** `descriptor()` and `run(params, inputs, outputs)`, in safe Rust. `Image` is a borrowed view; `Outputs::image` makes an output and lends its samples.
- **`export!(node, …)`** expands, in the extension's crate, into the table and `oscope_extension`. The glue it calls, with its `unsafe` code, is in the SDK. The expansion's own `#[unsafe(no_mangle)]` comes from an external macro, so an extension's crate builds under `#![forbid(unsafe_code)]`: the sample takes the workspace's lints.
- **`run_in_memory`** runs a node without OScope, for its author's tests.
- **The SDK checks the host's version** too, and refuses to run under another.

**6. Loading** (`oscope-ext`, with `libloading`):
- At each start, `Extensions::open` loads each Rust extension it loads (ADR-0018): its library for this platform. It checks the ABI version first, then the description.
- **The library's nodes must be exactly its manifest's `provides.nodes`.** Each must pass the pipeline registry's checks, and no stock node may have its ID: an extension providing a stock node's ID isn't loaded at all.
- **An extension whose library fails these checks isn't loaded,** and says why, now and for the next start, until another version is installed.
- **Libraries are never unloaded.** A Rust library's statics and thread-locals can't be torn down safely while the process runs.
- The loaded nodes join the `preview` Thing's registry, for live recipes, beside the stock nodes.

## Consequences

- **Authors write safe Rust** against a small SDK, and test their nodes without OScope.
- **A library built by any Rust compiler loads,** as long as it speaks ABI 1: the boundary has only C types.
- **A node runs as fast as a stock node,** but for parsing its parameters' JSON once per run. Its inputs are lent, and its outputs are filled where the host keeps them: nothing is copied.
- **A misbehaving extension can still crash the server:** a segfault, or an abort. That is the price of running in the process (D9). Python extensions run in a worker instead (ADR-0020).
- **ABI 1 carries images only.** Masks, tables, optional inputs and GPU nodes need a new version, with the host loading both while extensions move.
- **Other kinds** (drivers, panels, importers, exporters, workflows, reports) add their own functions to a later version, with their phases.

## Alternatives considered

- **Rust types across the boundary,** with the same compiler pinned for extensions and server. Fragile: any update to either would break every extension, without an error.
- **`abi_stable` or `stabby`.** They carry Rust types safely, but are large, and they would put their types into every extension's API. A dozen C functions and structs are enough for images.
- **The extension allocating its outputs, and the host freeing them through the extension.** That needs a free function for every buffer, and gets the allocator wrong if a call is missed. The host allocating keeps one owner.
- **Passing parameters as a C struct.** Each node's parameters differ. JSON is what the recipe has already, and what the schema checks.
- **Loading libraries in a worker process,** as Python's (ADR-0020). Images would cross a process boundary, and the reason for native nodes, their speed, would be lost.
- **Unloading a library when an extension is uninstalled.** Unsafe for Rust libraries, and unneeded: uninstalling waits for the next start (ADR-0018).
