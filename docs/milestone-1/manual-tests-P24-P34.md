# Manual tests: P24–P34

A checklist for trying each phase by hand, from its example. Tick each box as it passes.

## Before you start

- **Where:** run everything from the repository root, in Git Bash, on an up-to-date `main`. In PowerShell, use `curl.exe` rather than `curl`.
- **Release builds:** use `--release` where shown. The simulator renders far more slowly in debug builds, and P29's recording rate needs release.
- **Starting clean:** each example keeps its data in `.oscope/<example>/`. Delete that folder first for a clean start. P24, P25, P26 and P27 also test what a second run finds there.
- **uv** ([docs.astral.sh/uv](https://docs.astral.sh/uv/)) is needed for P26, Checkpoint B (P27), and opening stores in Python or napari (P28, P34). The first run downloads a Python and scikit-image.
- **CUDA:** P29 and P31 use `--features cuda`, which needs the CUDA toolkit and an NVIDIA GPU.
- **Disk:** P29's 10-second recording writes about 6 GB, and ingests it into a 4 GB store. Its 10-minute run keeps up to 100 GB on the scratch disk at once.
- **Ports:** each example that serves has its own, from 8230 to 8238, so none share.
- **Stopping:** stop a server with Ctrl-C.

## P24: extension packages

Run `cargo run -p ex-omext`, then open `http://127.0.0.1:8230/`.

- [ ] The terminal prints the package's SHA-256, then `installed`, and the extension *loaded none, the next start 0.1.0*. Run it twice from a clean start: the SHA-256 is the same both times.
- [ ] The page lists `org.oscope.examples.hello`, with its publisher, what it provides, and the start of its SHA-256; hover for the whole of it.
- [ ] **Start again** restarts the server, and the extension shows *loaded 0.1.0*. **Provided by the extensions loaded** lists `hello.greet` and `hello.greeting`.
- [ ] **Disable**, then **Start again**: it isn't loaded. **Enable**, and **Start again**: it's loaded again.
- [ ] **Install** the same package from `.oscope/ex-omext/scratch/`: *Installed already*, and nothing changes.
- [ ] **A new version:** change `version` in `examples/ex-omext/hello/manifest.toml` to `0.2.0`, then run `cargo run -p oscope-server -- ext pack examples/ex-omext/hello --out .oscope/hello-0.2.0.omext`, and install that file from the page. It shows *loaded 0.1.0* and *the next start loads 0.2.0*. **Start again:** 0.2.0 is loaded, and the `0.1.0` folder is gone from `.oscope/ex-omext/data/extensions/org.oscope.examples.hello/`.
- [ ] **Another API:** set `oscope-api` to `"^2.0"` in the manifest, pack it the same way, and install it: refused, naming the extension API 1.0.0. Then put the manifest back as it was (`git checkout examples/ex-omext/hello/manifest.toml`).
- [ ] **Uninstall**, then **Start again**: the extension is gone from the list.
- [ ] `curl -N http://127.0.0.1:8230/api/v1/extensions/changed` prints an event as you disable or enable it from the page.

## P25: Rust extensions

Run `cargo run --release -p ex-rust-ext`.

- [ ] It says the next start loaded `org.oscope.examples.emboss` 0.1.0, its ABI checked, and describes `example.emboss`: live, tile by tile with a halo of 1 px.
- [ ] It runs the recipe whole and in 256 px tiles, with each run's time, and ends: *both the same as the sample's own code run directly, to the bit*.
- [ ] Through OScope, the node takes within a few milliseconds of its code run directly: the README gives about 30 ms against 29 ms.
- [ ] `.oscope/ex-rust-ext/out/embossed.png` looks like `field.png` embossed, as if lit from its top left.
- [ ] Run it again: *installed already*.

## P26: Python extensions

Run `cargo run --release -p ex-python-node`. It needs uv.

- [ ] It installs the extension with its environment (Python, scikit-image and OScope's SDK), in seconds once uv's cache is warm.
- [ ] Run 1 starts the worker, in about a second. Runs 2 and 3 are warm, a few hundred milliseconds each, **with the same worker process ID** as run 1.
- [ ] A call on an 8 × 8 image takes under a millisecond in all: what the boundary costs.
- [ ] Denoising gains over 10 dB against the clean field: the README gives 21.9 dB noisy and 35.4 dB denoised.
- [ ] `.oscope/ex-python-node/out/denoised.png` is visibly smoother than `noisy.png`, with the nuclei's edges kept.
- [ ] Run it again: *installed already*.

## P27: driver and panel extensions; Checkpoint B

**The camera driver:** run `cargo run -p ex-ext-camera`, then open `http://127.0.0.1:8231/`.

- [ ] The live view shows diagonal stripes moving down and to the right.
- [ ] `curl http://127.0.0.1:8231/api/v1/camera/capabilities` names *OScope's example fake camera*.

**Checkpoint B:** run `cargo run -p ex-checkpoint-b`, then open `http://127.0.0.1:8232/`. It needs uv.

- [ ] The terminal shows the recipe `["smooth","emboss","denoise"]` on the preview with its time, then *5 previews through it*.
- [ ] The batch saves three frames, and the PNG files it lists are in `.oscope/ex-checkpoint-b/data/batch/`.
- [ ] It ends with *checkpoint B: one recipe of stock, Rust and Python nodes, live and as a batch job*.
- [ ] The page's live view shows the slide embossed and smoothed. It updates slowly, as the Python node takes a few hundred milliseconds a frame.

## P28: projects

Run `cargo run -p ex-projects`, then open `http://127.0.0.1:8233/`.

- [ ] Two cards, "H&E section 1" and "H&E section 2", each with its thumbnail, sample, tags and counts.
- [ ] Type in the search box: the cards filter by name, sample and tag. Click a tag: only projects with it show.
- [ ] Click a card: its acquisitions and result, with where each store is. Rename the project; the card shows the new name.
- [ ] **New project** makes an empty card. Delete it from its contents.
- [ ] Stop the example and run it again: it keeps the projects, and the rename.
- [ ] **In Python,** with uv: `uv run --no-project --python 3.13 --with "zarr>=3" --with ome-zarr python examples/ex-projects/open_store.py <a store's path from the page>` prints NGFF 0.5, axes `c, z, y, x`, and three levels.
- [ ] **In napari** (optional, needs a display): the README's command opens the store, with its channels' colours and the pixel size in µm.

## P29: scratch recording and ingest

Run `cargo run --release -p ex-record --features cuda`.

- [ ] It says it renders on the GPU, at 14.5 fps and about 579 MB/s, and reports the scratch disk's free space.
- [ ] Each second's line shows about 576 MB/s, **dropped 0**, and the queue at most 1 or 2 of 16.
- [ ] It ends with about 146 frames, 5.8 GB, 0 dropped. The ingest makes 146 planes of 5472 × 3648 in 5 levels, and deletes the recording from `.oscope/ex-record/scratch/recordings/`.
- [ ] `cargo run --release -p ex-record -- --test-sensor` records the fake camera's stripes at exactly the design rate, with 0 dropped.
- [ ] **The 10-minute run** (optional, long): `cargo run --release -p ex-record --features cuda -- --seconds 600 --keep-gb 100` reports every 10 s with 0 dropped, and keeps at most 100 GB on the disk. Delete the recording afterwards.

## P30: captures, downloads, importers and exporters

Run `cargo run --release -p ex-capture`, then open `http://127.0.0.1:8234/`.

- [ ] It captures a 5472 × 3648 `bayer16` frame, exports it as OME-TIFF (39.9 MB), and reads its first 4 bytes by a range: `"II*\0"`.
- [ ] Open the rendition's link it prints: the frame demosaiced and stretched for display.
- [ ] It imports the rendition (RGB of `u8`) and the download (grey of `u16`).
- [ ] `curl http://127.0.0.1:8234/api/v1/projects/formats` lists the importers (TIFF, PNG, JPEG) and the exporters (OME-TIFF, 16-bit TIFF, PNG).
- [ ] At `http://127.0.0.1:8234/docs`, run the `capture` Thing's `burst` with 3 frames, into the `ex-capture` project (its ID is in the project's link the example prints). The job makes one acquisition of three planes.
- [ ] (Optional, with Python's `tifffile`) the downloaded `.oscope/ex-capture/data/downloads/field.ome.tif` opens as an OME series.

## P31: registration

Run `cargo run --release -p ex-register --features cuda -- --bench`. It serves nothing.

- [ ] Each of the four cases finds its shift within 0.01 px of the expected, with a confidence above 0.5: about 0.99 for the small shifts, 0.73 for the large one everywhere, and 0.97 in the ±16 px window.
- [ ] The GPU's shift matches the CPU's, and is several times faster.
- [ ] The 2048² pair takes under 15 ms on the GPU, §5.20's budget: about 11 ms on the owner's PC.
- [ ] Without `--features cuda`, it says there's no GPU, and the CPU's results are the same.

## P32: calibration

Run `cargo run --release -p ex-calibrate`, then open `http://127.0.0.1:8235/`. It takes about 100 s at real time; add `-- --rate 10` to run the simulator ten times as fast.

- [ ] The objective check says the 10× is plausible, at about 0.971 of its nominal pixel size: the simulator magnifies 3 % more.
- [ ] The table of truth against recovered: the pixel size 0.23301 µm, rotated 30.000°, mirrored, aspect 1, skew 0, and backlash 5.00 µm in x and y, each within P32's tolerances.
- [ ] The page links to the calibration sets, those in use, and the latest objective check. Each opens as JSON.
- [ ] Run it again: new versions of the sets appear beside the old in `.oscope/ex-calibrate/data/calibration/`.

## P33: the calibration wizard

Run `cargo run --release -p ex-wizard`, then open `http://127.0.0.1:8236/`. It takes about 120 s at real time; `-- --rate 10` is faster.

- [ ] Each of the six steps prints its title and its prompt, then ✓ with what it made. The flat field's step moves to clear glass, the colour step loads the colour target, and the stage's step goes back to the tissue.
- [ ] At the end: the geometry (0.23301 µm, turned 30°, mirrored), the stage's backlash (5.00 µm each way), the dark frame, the flat field (about 10 % uneven before), and the colour's ΔE\*ab after the matrix near 0, from about 15.
- [ ] The page links to the wizard, the sets in use, every set, and the API's documentation.
- [ ] **A retry, by hand,** at `http://127.0.0.1:8236/docs`. The walk leaves the stage over the tissue.
  - Start a new wizard with `calibration/wizard_start`, and skip its first three steps with `wizard_skip`.
  - Confirm the flat field with `wizard_confirm`. Over the tissue, its step fails, saying the field isn't blank (`GET /api/v1/calibration/wizard` shows it).
  - `wizard_retry` prompts again. Move the stage to clear glass with `stage/move_to` (x 10000, y 12500, z 5002), confirm again, and the step is done.

## P34: autofocus, auto exposure and white balance; Checkpoint C

**Autofocus:** run `cargo run --release -p ex-autofocus`, then open `http://127.0.0.1:8237/`.

- [ ] The table shows each sample focused within a quarter of the depth of field of the truth: every `÷ d/4` between −1 and 1. The H&E is focused stepped and continuously; the stepped sweeps take 22 frames.
- [ ] The page's curve updates as each run ends: the coarse sweep grey, the fine sweep red, and where the stage was left blue, at the curve's peak. The line above it says where it focused, by which sweep, in how many frames and how long.
- [ ] Clear glass is refused: *the field is empty*. The page then says *No focus*, with why, over a curve with no peak.
- [ ] Auto exposure puts the 99.5th percentile within 5 % of 0.75 of full scale. White balance's gains are within 0.01 of 1, as the simulated light is white.
- [ ] `curl http://127.0.0.1:8237/api/v1/autofocus/defaults` gives Brenner's measure and a depth of field of about 7.9 µm for the 10× in transmitted light.
- [ ] **By hand, on tissue:** move to the first island 15 µm off focus, `curl -X POST -H "Content-Type: application/json" -d '{"x": 35350, "y": 14060, "z": 5017}' http://127.0.0.1:8237/api/v1/stage/move_to`. Then start a run: `curl -X POST -H "Content-Type: application/json" -d '{}' http://127.0.0.1:8237/api/v1/autofocus/run`. The page's curve shows a peak, and `curl http://127.0.0.1:8237/api/v1/stage/position` ends near z 5002.
- [ ] Ask for a run that can't be, such as `-d '{"step_um": -1}'`: it's refused with 400 as it's submitted, saying why.

**Checkpoint C:** run `cargo run --release -p ex-checkpoint-c`, then open `http://127.0.0.1:8238/`.

- [ ] It prints the four steps: the 10× calibrated (plausible, about 0.2400 µm a pixel), the autofocus within a quarter of the depth of field (1.97 µm), the exposure and white balance (gains within 0.01 of 1), and the capture's store.
- [ ] The page links to the calibration, the autofocus and exposure reports, and the projects. Each opens.
- [ ] **In napari** (needs a display and uv): the command it prints opens the capture, with the pixel size in µm.
