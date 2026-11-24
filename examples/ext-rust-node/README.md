# ext-rust-node

A sample Rust extension for OScope (P25, [ADR-0019](../../docs/adr/0019-the-rust-extension-abi.md)): one processing node, `example.emboss`, which embosses an image as if lit from its top left. It is what a third party writes: a library crate built against [`oscope-ext-sdk`](../../crates/oscope-ext-sdk) alone.

It isn't a program. [`ex-rust-ext`](../ex-rust-ext/README.md) builds it, packs it, installs it, and runs it in a recipe.

## The node

Each sample becomes 0.5 plus `strength` times a weighted sum of its 3 × 3 neighbourhood over 4, with the edges repeated. The weights run across the diagonal, from −2 at the top left to +2 at the bottom right, and sum to 0. So a flat image becomes a flat grey, and an edge stands out light or dark.

- **Input** `image`, **output** `image`, of the same size and channels.
- **Parameter** `strength`, from 0 to 10, 1 by default.
- **It runs live** (quick enough for every frame), and **tile by tile** with a halo of 1 px.

## Its panel

[`panels/example.emboss.json`](panels/example.emboss.json) describes a panel for the app to draw (W8.5, [ADR-0036](../../docs/adr/0036-extension-drivers-and-panels.md)): a form of `strength`, submitted to `example.emboss` on the live view's newest frame, its answer shown as an image. It is data, an asset of the package, which the manifest lists. In the app: Settings, *Extensions*, then the panel.

## How it is made

[`src/lib.rs`](src/lib.rs):

1. **`Emboss` implements `ProcessingNode`:** its `descriptor` (ID, version, ports, parameters' schema, capabilities) and its `run`.
2. **`run` makes its output** with `outputs.image(…)`: OScope allocates it, and the node fills it.
3. **`oscope_ext_sdk::export!(Emboss)`** makes the C ABI: the function `oscope_extension`, which OScope looks for in the library, and the table behind it. The crate needs no `unsafe` of its own.
4. **Its test runs the node without OScope,** with `oscope_ext_sdk::run_in_memory`.

[`Cargo.toml`](Cargo.toml) builds it as a `cdylib`, the library OScope loads. It is an `rlib` as well, so that its test and `ex-rust-ext` can call it directly. **Keep `panic = "unwind"`,** Cargo's default: the SDK turns a panic in a node into an error, which it can't do once panics abort.

## Packing it by hand

[`manifest.toml`](manifest.toml) names a library for each platform. To make a package for both:

1. build the library on each platform: `cargo build --release -p ext-rust-node`;
2. copy `ext_rust_node.dll` (Windows) and `libext_rust_node.so` (Linux) into a folder's `lib/`, beside a copy of `manifest.toml`;
3. pack the folder: `cargo run -p oscope-server -- ext pack <folder>`.

A package installs only where it has a library for the platform. `ex-rust-ext` packs the library it built, for this platform alone.
