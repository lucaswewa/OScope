# Examples

Each example is a small crate that shows one thing a phase built, through the crates' public API. The phase specs name them ([`phases.md`](../docs/milestone-1/phases.md)), and every phase has one.

## Conventions

- **Where it lives.** `examples/ex-<name>/`, as a crate called `ex-<name>`, such as `ex-core-tour`. The workspace takes every folder in `examples/` as a member, so an example needs no change to the root `Cargo.toml`.
- **Running it.** `cargo run -p ex-<name>` runs it with its defaults, which need no hardware: the simulator, or a fake.
- **Its README** says what it shows, then *How to try it*: the commands, the pages to open, and what you should see.
- **`--smoke`.** Every example takes `--smoke`. It then runs headless for a few seconds, checks what it shows, and exits with 0 only if all went well. The local replays of CI run it ([`scripts/`](../scripts/README.md)). GitHub CI does too while `RUN_EXAMPLES` is `"true"` in its workflows: it is `"false"` for now, to save CI minutes, until the owner turns it back on.
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
| [`ex-sim-stage`](ex-sim-stage/README.md) | P11 | The simulated grblHAL stage on a 10× clock: homing, moves and a jog, each planned against measured, trigger pulses, and its reported position plotted against time |
| [`ex-sim-camera`](ex-sim-camera/README.md) | P12 | The simulated MIChrome 20 streaming over the H&E slide during a jog: each mode's rate, drops and jitter, and recorded positions against the truth |
| [`ex-sim-3d`](ex-sim-3d/README.md) | P13 | The simulator's 3D samples stepped through in z on a page: a reflective chip with its true height map and depth from focus against it, and fluorescent cells in widefield and confocal beside their true volume |
| [`ex-sim-samples`](ex-sim-samples/README.md) | P14 | The simulator's sample catalogue: each sample loaded and rendered at 5× and 50× in its own light, an image and a stack imported through a configuration, and the empty stage once unloaded |
| [`ex-sim-bench`](ex-sim-bench/README.md) | P15 | The simulated camera at full resolution against §5.20's budgets: a cold frame, then warm frames still and jogging, on the CPU and, with `cuda`, the GPU |
| [`ex-http-walkthrough`](ex-http-walkthrough/README.md) | P16 | The instrument on the control plane: the simulated microscope's camera, stage, illumination, objective and simulation Things, with a walkthrough in `curl` and a control page with the stage's position and buttons |
| [`ex-live-view`](ex-live-view/README.md) | P17 | The live preview: the simulated camera's frames as viewport-sized JPEGs through the `preview` Thing's binary WebSocket, drawn with WebGL, zoomed by asking for a region of the sensor, with fps, latency and jog buttons |
| [`ex-live-overlays`](ex-live-overlays/README.md) | P18 | The live view's overlays from statistics of the raw frames: a crosshair, an ROI to draw with its statistics, a line profile, histograms with percentiles, the focus value, false colour and the saturation mask |
| [`ex-sidecar`](ex-sidecar/README.md) | P19 | A small supervisor, as the desktop app will be: it runs the server by the sidecar contract, reads its JSON ready line, starts it again when it exits with code 75, and stops it by closing its stdin |
| [`ex-shm-reader`](ex-shm-reader/README.md) | P19 | A second process reading the server's shared-memory preview ring, the desktop app's fast path: the rate previews come at, and any torn reads |
| [`ex-jobs`](ex-jobs/README.md) | P20 | The job service: stage tours as hardware jobs, one at a time and holding the hardware lock, and crunching as compute jobs sharing a GPU budget, on a page with the queue and progress live, cancel, pause, resume and reorder |
| [`ex-pipeline`](ex-pipeline/README.md) | P21 | A recipe of typed nodes run on a field of the simulated slide from the command line: its graph, each step's time, a second run from the memo cache, an edit that re-runs only what follows it, a tiled run the same to the bit, and a batch over more fields as a compute job |
| [`ex-live-recipe`](ex-live-recipe/README.md) | P22 | Live recipes of stock nodes on the live view: a choice of recipes, each checked against its latency budget before it runs; a flat field and a dark frame captured from the camera and applied over the region shown |
| [`ex-filters`](ex-filters/README.md) | P23 | The general image-processing toolkit on a field of the simulated slide, compared on a page: the Gaussian, median and bilateral filters, the unsharp mask, CLAHE, thresholds, morphology, the distance transform and labelled nuclei, each with its time on the CPU and the GPU |

`examples/common` is the examples' shared library, `ex-common`, not an example.

P01's example is the server's own command line: [`crates/oscope-server/README.md`](../crates/oscope-server/README.md) says how to try it.
