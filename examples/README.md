# Examples

Each example is a small crate that shows one thing a phase built, through the crates' public API. The phase specs name them ([`phases.md`](../docs/milestone-1/phases.md)), and every phase has one.

## Conventions

- **Where it lives.** `examples/ex-<name>/`, as a crate called `ex-<name>`, such as `ex-core-tour`. The workspace takes every folder in `examples/` as a member, so an example needs no change to the root `Cargo.toml`.
- **Running it.** `cargo run -p ex-<name>` runs it with its defaults, which need no hardware: the simulator, or a fake.
- **Its README** says what it shows, then *How to try it*: the commands, the pages to open, and what you should see.
- **`--smoke`.** Every example takes `--smoke`. It then runs headless for a few seconds, checks what it shows, and exits with 0 only if all went well. CI runs it.
- **Ports.** An example that serves something listens on a port from 8211 to 8219 by default, the range kept for manual checks. Its `--smoke` run uses a port from 8220 to 8229, or one the system picks.
- **Data.** An example writes under a data root of its own, `.oscope/ex-<name>/` by default, which Git ignores. Its `--smoke` run uses a temporary folder.
- **Pages.** A page an example serves is plain HTML and JavaScript, in the example's `page/` folder. [`examples/common`](common/README.md) serves it next to the API, at the same origin, and runs the `--smoke` checks.
- **Vendored files.** JavaScript libraries go in `examples/vendor/`, each with its licence. That folder isn't a crate, so it must be excluded from the workspace when it arrives (`exclude = ["examples/vendor"]` in the root `Cargo.toml`).
- **Dependencies.** An example may depend on any OScope crate. No OScope crate may depend on an example; the layering test ([ADR-0005](../docs/adr/0005-layered-crates-and-their-dependency-rules.md)) checks.

## The examples

| Example | Phase | What it shows |
|---|---|---|
| [`ex-core-tour`](ex-core-tour/README.md) | P02 | Units, an affine map fitted to noisy point pairs, and the clock |
| [`ex-frame-bus`](ex-frame-bus/README.md) | P03 | A 20 MP source at 14.5 fps through the frame pool and bus, with lossless and latest-wins subscribers |
| [`ex-server-hello`](ex-server-hello/README.md) | P04 | The server's `system` Thing, and a page beside it that shows its properties and Thing Description |
| [`ex-logs`](ex-logs/README.md) | P05 | The logs' live tail with filters, a button that writes a note at each level, and the daily files |
| [`ex-compute-info`](ex-compute-info/README.md) | P06 | The compute backends, the processor and its live and batch pools, and microbenchmarks of the first kernels at 20 MP |
| [`ex-cuda-check`](ex-cuda-check/README.md) | P07 | The GPU, the CUDA twins against their CPU references with their times, and pinned against ordinary uploads (feature `cuda`) |
| [`ex-demosaic`](ex-demosaic/README.md) | P08–P08c | Each demosaicing method on a 20 MP test chart, on the CPU and the GPU against §5.20's budgets, with its PSNR, on a page comparing their images |
| [`ex-objectives`](ex-objectives/README.md) | P09 | The objective catalogue, with each objective's pixel size, field of view and resolution on the MIChrome 20, and the manual objective mount and lights with their capability reports |
| [`ex-hal-fakes`](ex-hal-fakes/README.md) | P09b | The scripted fakes' capability reports and behaviour, and the conformance suite passing on each and catching a fake scripted to fail |
| [`ex-sim-render`](ex-sim-render/README.md) | P10 | The simulated H&E slide: an overview, and fields through each objective at five focus positions, with their rendering times, on a page |

`examples/common` is the examples' shared library, `ex-common`, not an example.

P01's example is the server's own command line: [`crates/oscope-server/README.md`](../crates/oscope-server/README.md) says how to try it.
