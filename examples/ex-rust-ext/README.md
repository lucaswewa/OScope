# ex-rust-ext

This branch runs the standalone example in the Linux backend container. CUDA is
preferred where recipe nodes support it, with CPU fallback. Server integration
is deferred. See the [R05 procedure](../../docs/phases/r05-storage-processing.md#manual-test).

A Rust extension through OScope (P25, [ADR-0019](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0019-the-rust-extension-abi.md)): the sample [`ext-rust-node`](../ext-rust-node/README.md) built, packed, installed, loaded at a start with its ABI checked, and run in a recipe.

## How to try it

```sh
cargo run -p ex-rust-ext
```

Cargo builds the sample's library along with the example. The example then:

1. **finds the Linux library,** `libext_rust_node.so`, beside it in `target/`;
2. **packs it for this platform** into `.oscope/ex-rust-ext/org.oscope.examples.emboss-0.1.0.omext`, and prints its SHA-256;
3. **installs the package,** and starts again as the server does, which **loads the library**: its ABI version checked, its nodes described and checked against the manifest's `provides`;
4. **runs a recipe** of `example.emboss`, strength 3, on a 1024 × 768 field of the simulated H&E slide: whole, then in 256 px tiles, with each run's time;
5. **runs the sample's own code directly,** timed, and checks both runs against it, sample for sample: the C ABI changes nothing;
6. **writes** `field.png` and `embossed.png` in `.oscope/ex-rust-ext/out/`.

```text
the next start loaded org.oscope.examples.emboss 0.1.0, its ABI checked, in 21.6 ms
  example.emboss: Embosses an image, as if lit from its top left.
    live: true, tile by tile with a halo of 1 px
...
both the same as the sample's own code run directly, to the bit
```

Running it again says *installed already*: installing the same package again changes nothing. With `--release`, on the owner's PC, the node takes about 30 ms through OScope, against 29 ms for its code run directly: the boundary costs about a millisecond.

**In the server:** install the package into a server's data root and start the server again. Two ways: the upload route, as in [`ex-omext`](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/examples/ex-omext/README.md), or that example's page. The `extensions` Thing then lists the extension loaded, and the `preview` Thing lists `example.emboss` among the nodes that live recipes may use (`GET /api/v1/preview/nodes`).

**`--smoke`** does the same on a 256 × 192 field in 64 px tiles, in a temporary folder.

The example's test, `tests/server.rs`, installs the package, starts a server with the fake devices, and checks that `example.emboss` is loaded, offered to live recipes, and accepted in one.
