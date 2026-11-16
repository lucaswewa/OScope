# Examples

Each example is a small crate that shows one thing a phase built, through the crates' public API. The phase specs name them ([`phases.md`](../docs/milestone-1/phases.md)), and every phase has one.

## Conventions

- **Where it lives.** `examples/ex-<name>/`, as a crate called `ex-<name>`, such as `ex-core-tour`. The workspace takes every folder in `examples/` as a member, so an example needs no change to the root `Cargo.toml`.
- **Running it.** `cargo run -p ex-<name>` runs it with its defaults, which need no hardware: the simulator, or a fake.
- **Its README** says what it shows, then *How to try it*: the commands, the pages to open, and what you should see.
- **`--smoke`.** Every example takes `--smoke`. It then runs headless for a few seconds, checks what it shows, and exits with 0 only if all went well. The local replays of CI run it ([`scripts/`](../scripts/README.md)). GitHub CI does too while `RUN_EXAMPLES` is `"true"` in its workflows: it is `"false"` for now, to save CI minutes, until the owner turns it back on.
- **Ports.** An example that serves something listens on a port from 8211 to 8219, or from P24 on 8230 to 8249, by default: the ranges kept for manual checks. Its `--smoke` run uses a port from 8220 to 8229, or one the system picks.
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
| [`ex-omext`](ex-omext/README.md) | P24 | Extensions' packages: a dummy extension packed the same way each time, installed through the upload route, refused when it is for another API or could write outside its folder, and listed; on a page, installing, enabling, disabling and uninstalling, each loaded at the next start, and a restart |
| [`ext-rust-node`](ext-rust-node/README.md) | P25 | A sample Rust extension, not a program: an emboss node, built against the SDK alone, with `export!` making its C ABI |
| [`ex-rust-ext`](ex-rust-ext/README.md) | P25 | The sample Rust extension built, packed for this platform, installed, loaded at a start with its ABI checked, and run in a recipe whole and tile by tile, the same to the bit as its own code run directly |
| [`ext-python-node`](ext-python-node/README.md) | P26 | A sample Python extension, not a crate: scikit-image's total-variation denoising as a node, its parameters' schema from its type hints |
| [`ex-python-node`](ex-python-node/README.md) | P26 | The sample Python extension installed, its environment made by uv; its worker started on the first run of a recipe and kept warm, with each run's time; noise removed from a field of the simulated slide, measured |
| [`ext-rust-camera`](ext-rust-camera/README.md) | P27 | A sample Rust extension, not a program: a camera driver streaming a moving test pattern, which passes the HAL's conformance suite |
| [`ex-ext-camera`](ex-ext-camera/README.md) | P27 | The server with an extension's camera instead of the simulator: the sample installed, its driver named by the devices' configuration, its frames in the live view |
| [`ex-checkpoint-b`](ex-checkpoint-b/README.md) | P27 | Checkpoint B: one recipe of a stock node, a Rust extension's and a Python extension's, live on the preview, then as a batch job over the camera's next frames |
| [`ex-projects`](ex-projects/README.md) | P28 | Projects on disk: two projects, each with an acquisition of the simulated slide stored as OME-Zarr, and a result with its provenance; a Gallery-style page of their cards, searched and paged; and how to open the stores in napari or Python |
| [`ex-record`](ex-record/README.md) | P29 | Recording at the design rate, 16 bits at 14.5 fps: the simulated camera's frames, or the test sensor's, written to the scratch disk with how fast and how many were dropped; rolling for long runs; then the recording ingested into a project as OME-Zarr, by a job |
| [`ex-capture`](ex-capture/README.md) | P30 | Captures, downloads and file formats: a frame of the simulated camera captured into a project with its display rendition, exported as OME-TIFF and downloaded whole and by a range, and images imported, all through the API |
| [`ex-register`](ex-register/README.md) | P31 | Registration by phase correlation on fields of the simulated slide: a sub-pixel shift, a whole one and a large one found to a hundredth of a pixel, the large one again in a search window around the shift expected, on the CPU and the GPU; with `--bench`, a 2048² pair against §5.20's 15 ms |
| [`ex-calibrate`](ex-calibrate/README.md) | P32 | Calibration through the API, on a simulated instrument with its camera turned and mirrored, a magnification 3 % off and stage backlash: the objective check, the geometry from a grid of stage moves, and the stage's backlash, soft limits and homing, printed against the truth |
| [`ex-wizard`](ex-wizard/README.md) | P33 | The calibration wizard walked from the command line, on a simulated instrument that vignettes and whose sensor's colours cross: each step's prompt, done on the simulator, confirmed and settled; the geometry, the stage, a dark frame, a flat field and the colour matrix, with ΔE\*ab before and after |
| [`ex-autofocus`](ex-autofocus/README.md) | P34 | Autofocus on the simulator's H&E tissue, reflective chip and fluorescent cells, stepped and continuous, each against the truth in quarters of the depth of field; an empty field refused; auto exposure and white balance, the gains given to the preview; a page plotting the focus curve |
| [`ex-checkpoint-c`](ex-checkpoint-c/README.md) | P34 | Checkpoint C: the 10× calibrated, the slide autofocused, the exposure and white balance set, and a frame captured into a project as OME-Zarr, with napari's command to open it |
| [`ex-plan`](ex-plan/README.md) | P35 | A focus map measured by autofocus over a tilted simulated slide, a plane and a thin-plate spline against the truth; raster, snake, tissue-only, mapped, tiled z-stack, z-scan and multi-position plans over the H&E slide, with their tiles, data and time; a page drawing each plan's tiles and path |
| [`ex-scan`](ex-scan/README.md) | P36 | A snake scan of the tilted simulated H&E slide through the `scan` Thing: planned and previewed, run as a job autofocusing every tenth tile, each tile printed as it arrives, and the acquisition checked, each frame where it was taken; a page drawing the tiles as they arrive |
| [`ext-python-workflow`](ext-python-workflow/README.md) | P36 | A sample Python extension: the scan workflow `example.spiral`, tiles in a square spiral out from a centre |
| [`ex-histo-scan`](ex-histo-scan/README.md) | P37 | A histological scan of the simulated H&E slide: an overview binned 3 × 3, the tissue found in it by its saturation, checked against the simulator's truth, and the tissue scanned, its focus map measured first; a page showing the overview, the tissue and the tiles as they arrive |
| [`ex-live-mosaic`](ex-live-mosaic/README.md) | P38 | A live mosaic of the simulated H&E slide, drawn as the stage stops at four places and as a scan beside them takes its tiles; a page showing the live view beside the mosaic in OpenSeadragon, which grows as the arrow keys move the stage |
| [`ex-stitch`](ex-stitch/README.md) | P39 | A scan of the simulated H&E slide by a stage without encoders, losing steps, stitched as a job: each tile registered with its neighbours and solved, against the simulator's truth; `--bench N` times N tiles of 20 MP; a page showing the mosaic |

`examples/common` is the examples' shared library, `ex-common`, not an example. `ext-rust-node` and `ext-rust-camera` are libraries too: the sample extensions that `ex-rust-ext` and `ex-ext-camera` load. `ext-python-node` is a Python extension, which the workspace leaves out.

P01's example is the server's own command line: [`crates/oscope-server/README.md`](../crates/oscope-server/README.md) says how to try it.
