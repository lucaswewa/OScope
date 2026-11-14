# P25: Rust extension ABI and SDK

- Status: Done
- Branch / PR: `m1/p25-rust-ext` / [#39](https://github.com/lucaswewa/OScope/pull/39)
- Size: L · estimated ~2,300 before it started (the C ABI's tables and views, the safe `ProcessingNode` trait, the `export!` macro and its glue, with panics caught, ~450; the loader with its version check, the host's table and the nodes it gives the pipeline, loaded at each start and offered to live recipes, ~450; tests ~550; the sample `ext-rust-node` and `ex-rust-ext` ~450; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,247** hand-written lines added (code 1,078; tests 735; examples 434, of which the sample 142), and 31 removed. **That is 1,347 over L's 900, and about 2 % under the estimate.**

## Summary

Third parties write processing nodes in Rust, and OScope loads them, checks them and runs them in its pipeline ([spec](../phases.md#p25); plan §5.8; D9; [ADR-0019](../../adr/0019-the-rust-extension-abi.md)).

- **`oscope-ext-sdk`:**
  - **the C ABI, version 1:** the extension's and the host's tables, images and text as `#[repr(C)]` views, and statuses;
  - **the safe API:** `ProcessingNode`, `Descriptor`, `Image` and `Outputs`;
  - **`export!`,** which makes the C ABI for an extension's nodes, catching their panics;
  - **`run_in_memory`,** to test a node without OScope.
- **`oscope-ext`:** `Library`, which loads a library with `libloading`, checks its ABI version and its description, and makes its nodes the pipeline's. Each start loads the Rust extensions it loads, and checks their nodes against their manifests and the stock nodes.
- **`oscope-api`:** the `preview` Thing offers the loaded extensions' nodes to live recipes.
- **`ext-rust-node`:** a sample extension, an emboss node, written as a third party would: against the SDK alone, with no `unsafe`.
- **`ex-rust-ext`:** builds the sample, packs it, installs it, loads it at a start, and runs it in a recipe, against the sample's own code.

## What was built

| Where | What |
|---|---|
| `oscope-ext-sdk` `abi.rs` | `ABI_VERSION` 1, `ENTRY` (`oscope_extension`), `Entry`; `Extension` and `Host`; `ImageRef`, `ImageMut`, `Str`, `Call`; `OK`, `FAILED`, `PANICKED` |
| `oscope-ext-sdk` `lib.rs` | `ProcessingNode`; `Descriptor` and `to_json`, as the pipeline reads it; `Image` (a borrowed view, with `clamped` for edges); `Outputs::image`; `run_in_memory` and `OwnedImage`; `export!`, and the glue it calls: the table, the panic guard, the host's version checked |
| `oscope-ext` `native.rs` | `Library::load` and `from_entry`: the version, then the description (images only, the CPU, the registry's checks); the host's table, catching its own panics; `MAX_SAMPLES`; the pipeline's node and step for an extension's node |
| `oscope-ext` `installed.rs` | Each start loads Rust extensions' libraries; a library's nodes must be its manifest's; no extension may provide a stock node's ID; a library that doesn't load is reported now and for the next start; `Extensions::nodes` |
| `oscope-ext` | `ExtError::Library` |
| `oscope-api` `preview.rs` | The live recipes' registry gets the loaded extensions' nodes |
| `examples/ext-rust-node` | The sample: `Emboss`, `emboss`, `export!(Emboss)`, its test, its manifest |
| `examples/ex-rust-ext` | Finding the built library and packing it for this platform (`lib.rs`); the walk-through (`main.rs`); the server test |

## How to try it

```sh
cargo run -p ex-rust-ext
```

It finds the sample's library, packs it, installs it, loads it as a start does, and runs `example.emboss` on a field of the simulated slide, whole and tile by tile. Both runs are the same, to the bit, as the sample's own code run directly. It writes `field.png` and `embossed.png` in `.oscope/ex-rust-ext/out/`. The [README](../../../examples/ex-rust-ext/README.md) says how to install the package into a server and use the node in a live recipe.

## Performance

No budget of plan §5.20 is P25's. On the owner's PC (release; `ex-rust-ext`; a 1024 × 768 RGB field; three runs):

| | Time |
|---|---|
| `example.emboss` through the ABI and the pipeline, whole | 29.7–30.0 ms |
| The sample's own code, called directly | 28.5–29.1 ms |
| The same, in 12 tiles of 256 px with its 1 px halo | 37.0 ms |
| Loading the library at a start, with its checks | 27.9 ms the first time, 0.4 ms after |

The boundary and the pipeline cost about a millisecond. The node itself is the sample's plain loop, unoptimised and on one thread.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in close to the estimate, which allowed a fifth more than P24's densities after P24's overrun.
- **An extension's crate needs no `unsafe`.** `export!`'s expansion carries `#[unsafe(no_mangle)]`, but lints don't fire inside another crate's macro, so the sample builds under the workspace's `forbid(unsafe_code)`. The `unsafe` the glue needs is in `oscope-ext-sdk`, as CLAUDE.md allows.
- **ABI 1 carries images only,** with no optional inputs, on the CPU. Masks, tables and GPU nodes need a new version.
- **The host owns every buffer:** it lends the inputs, and allocates the outputs, which the extension fills in place. Nothing is copied, and nothing is freed across the boundary.
- **Libraries are never unloaded,** not even on an in-process restart: a Rust library can't be torn down safely.
- **A library's nodes must be exactly the ones its manifest provides,** checked when it loads. Otherwise the registry built from manifests (ADR-0018) wouldn't match what runs.
- **No extension may provide a stock node's ID.** P24's load plan now counts the stock nodes as taken.
- **The sample is an example crate,** `ext-rust-node`: a library, not an `ex-` program. It is skipped by the examples' smoke runs, which run only crates with a `src/main.rs`.

**Other choices:**

- **The entry function's name,** `oscope_extension`, is the same in every version. The table's first field tells the version, which the host reads before anything else.
- **Statuses are plain `u32`s,** not a Rust `enum`: a value the other side doesn't know would be undefined behaviour in an `enum`.
- **Parameters cross as JSON text,** checked by the pipeline against the node's schema before the run. The SDK parses them once per run.
- **Each side catches its own panics.** The SDK turns a node's panic into `PANICKED`, with its message. The host's functions catch theirs, so a bug in the host can't unwind into the extension either.
- **The SDK checks the host's ABI version too,** and refuses to run under another.
- **An output** may hold up to 2³⁰ samples, and each is made once. A node that doesn't make one of its outputs fails.
- **Loading a library** that fails (not a library, another ABI, a description refused, its nodes not its manifest's) leaves the extension not loaded, with why, and its names out of the registry. Its state says so for the next start too, until another version is installed.
- **`libloading` 0.9.0** was already in the lock, through `cudarc`. 0.9.1 came out the day this phase started.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-ext-sdk` | `sdk` (5) | **The ABI's layouts:** every struct's size, alignment and offsets, the constants and the entry's name, pinned. **Descriptors** read as the pipeline's. **Images** hold exactly their samples, and repeat their edges. **`run_in_memory`:** two outputs made; an output made twice, one that doesn't exist, and a failure, each said. **The glue:** another host's version refused, the description given, a panic caught as `PANICKED` with its message, a node out of range refused |
| `oscope-ext` | `native` (4) | **An extension made with `export!`,** in this process: its five nodes described; one run in a recipe gives twice its input, and the node's schema is the pipeline's to check. **A panic, a failure, a missing output and an output made twice** each become the step's error, and the extension still works after. **Refused:** another ABI version, a mask port, a schema the registry can't check, and an entry giving no table. **At a start:** a package whose library isn't a library isn't loaded, says why now and for the next start, and leaves the registry without its names; a package providing `filter.gaussian` is refused for the stock node |
| `ext-rust-node` | its unit test | The node, run in memory: a flat image becomes grey, and each column beside a step stands out by 0.75 × strength, worked by hand |
| `ex-rust-ext` | `server` (2) | **The real library,** packed and installed: a start loads it, the `extensions` Thing lists it and its node, the `preview` Thing offers the node to live recipes, and a live recipe with it passes its check. **A manifest providing another node** than the library has: not loaded, saying both |
| `ex-rust-ext` | `--smoke` | The walk-through on a small field: the library found, packed, installed and loaded; the recipe whole and tile by tile, the same to the bit as the sample's code |

**Where they ran:** on Windows, and on Linux under WSL, where the library is a `.so`; the three suites and `ex-rust-ext --smoke` 5 times out of 5 each with their binaries pinned to two cores.

**Mutation checks** (each broke the code on purpose, then the tests were run): all eleven caught.

| Broken on purpose | Caught by |
|---|---|
| The SDK letting a node's panic go | `the_glue_refuses_another_host_and_catches_panics` and the `native` suite: their processes abort, as Rust ends a process whose panic reaches an `extern "C"` function, *panic in a function that cannot unwind* |
| The SDK not checking the host's version | `the_glue_refuses_another_host_and_catches_panics` |
| The host not checking the library's version | `another_abi_version_and_a_description_the_pipeline_cannot_take_are_refused` |
| Ports other than images let through | the same |
| Nodes not checked as the registry checks them | the same |
| An output made twice | `a_panic_a_failure_and_a_missing_output_become_errors` |
| A missing output let through | the same |
| A library's nodes not checked against its manifest | `a_library_whose_nodes_are_not_its_manifest_s_does_not_load` |
| Stock nodes' IDs free for extensions | `a_library_that_does_not_load_says_why_and_stock_nodes_are_taken` |
| A failed library forgotten for the next start | the same |
| The live recipes' registry without the extensions' nodes | `a_start_loads_the_sample_and_live_recipes_can_use_its_node` |

## Follow-ups and known gaps

- **Python nodes** (P26), and the other kinds of contribution in their phases (P27, P30, P36, P47), with the ABI's later versions.
- **ABI 2,** if extensions need masks, tables, optional inputs or the GPU.
- **A crash in an extension** still ends the server. The service manager starts it again (ADR-0015).

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The ABI** follows the C ABI's usual pattern of a versioned table of functions, written for OScope.
- **Embossing** is a textbook operation: a 3 × 3 kernel of differences across a diagonal, added to a mid grey.
