# Manual tests: P35–P43

A checklist for trying each phase by hand, from its example. Tick each box as it passes.

## Before you start

- **Where:** run everything from the repository root, in Git Bash, on an up-to-date `main`. In PowerShell, use `curl.exe` rather than `curl`.
- **Release builds:** use `--release` where shown. The simulator renders far more slowly in debug builds, and the times in the READMEs are from release builds at real time.
- **Starting clean:** each example keeps its data in `.oscope/<example>/`. Delete that folder first for a clean start.
- **A browser with WebGL** for P41–P43: VTK.js draws their volumes and surfaces with it. Their pages haven't been seen in a browser before this checklist. They have only run under Node, with VTK.js's objects but not its WebGL window.
- **Time:** each example scans at real time, from about 20 s for a z-scan to about two and a half minutes for P37's and Checkpoint D's tissue scans.
- **Ports:** each example that serves has its own, from 8239 to 8249, so none share.
- **Stopping:** stop a server with Ctrl-C.

## P35: scan planners and focus maps

Run `cargo run --release -p ex-plan`, then open `http://127.0.0.1:8239/`. Its focus map takes about 100 s.

- [ ] It describes the 10× on the MIChrome 20: frames of 1313.3 × 875.5 µm, a tenth shared with each neighbour, turned 0.5°, and cells of 1296.4 × 864.2 µm.
- [ ] **The focus map:** 9 points, each found within about half a micrometre of the simulator's truth. Its plane rises about +1.0 µm a mm along x and −0.5 along y, as the slide is tilted.
- [ ] Between the points, the plane is within about 0.1 µm of the truth and the thin-plate spline within about 0.3. Both are under a quarter of the depth of field, 1.97 µm.
- [ ] **The plans' table:**
  - the raster and the snake over the coverslip take the same 1,333 tiles, the snake about half the travel;
  - the snake over the tissue takes 370;
  - the tiled z-stack takes 11 frames a tile.
- [ ] **On the page,** choose each plan in turn: the region in green, each tile's frame, and the stage's path from the dot. The raster's rows all start at the same side; the snake's turn back.
- [ ] The plan by the map colours its frames by z, from about 5027.6 µm in blue to 5033.0 in red. Under the drawing: *The focus map: 9 of 9 points focused*, with its plane's tilt beside the slide's.

## P36: the scan engine and the `scan` Thing

Run `cargo run --release -p ex-scan`, then open `http://127.0.0.1:8240/`.

- [ ] It plans a snake over 1 × 1 mm of the first island: 30 tiles, 5 × 6, about 36 s and 47 MB, autofocusing every tenth tile.
- [ ] Each tile prints as it arrives. Tiles 1, 11 and 21 are focused, and the tiles between keep the last z found.
- [ ] It ends: scanned in about 44 s, and an acquisition of 30 frames, each within about 1.4 µm of its tile, with the plan.
- [ ] **The page** outlines each tile and fills it as its frame is taken, coloured by its z, with a dot where the stage is. The line above it counts the tiles taken and gives the scan's state.
- [ ] The stage ends where it started, at the island's middle: `curl http://127.0.0.1:8240/api/v1/stage/position`.
- [ ] At `http://127.0.0.1:8240/docs`, the `scan` Thing's `plan` previews a scan, its tiles and estimates, without moving the stage.
- [ ] **The Python workflow** (optional, needs uv): `OSCOPE_PYTHON=1 cargo test -p oscope-api --test scan` passes `a_python_workflow_plans_a_spiral_that_the_engine_runs`. It installs the sample `example.spiral`, plans a ring of 9 tiles and runs it. The examples' servers don't load extensions.

## P37: histological scan

Run `cargo run --release -p ex-histo-scan`, then open `http://127.0.0.1:8241/`. It takes about two and a half minutes.

- [ ] **The overview:** 49 frames binned 3 × 3, 160 × 120 pixels of 50 µm, in about 16 s.
- [ ] **The tissue found:** saturation over about 0.23, about 25 mm² in 3 pieces, 1 dropped as dust.
- [ ] 38 tiles over the tissue, where a raster over the region takes 56. Against the simulator's truth, about 99.9 % of the points of tissue fall in a tile.
- [ ] **The scan:** a focus map of 9 points, 0 rejected, missing about 0.3 µm RMS; 38 frames into the project.
- [ ] **The page** shows the overview, the tissue shaded green, and the tiles over it, filled as their frames are taken.

## P38: live stitching

Run `cargo run --release -p ex-live-mosaic`, then open `http://127.0.0.1:8242/`.

- [ ] It prints a mosaic of 3125 × 2084 pixels of 1.92 µm over 6 × 4 mm. Roaming to 4 places draws a few frames and leaves out those blurred on the way. The scan's 6 tiles are each drawn as taken.
- [ ] **The page** shows the live view beside the mosaic, the island drawn where the example roamed and scanned.
- [ ] **Move the stage** with the arrow keys, a quarter of a millimetre a press, or a millimetre with Shift. Once it stops, a frame is drawn where the slide is; frames taken on the way aren't.
- [ ] *Scan 2 × 2 mm here* scans about the stage, each tile drawn as it's taken.
- [ ] *Halt* stops drawing: move, and nothing new appears. *Begin* starts a new mosaic over 10 × 8 mm about the stage. *Clear* empties it.
- [ ] Zoom into the mosaic: finer tiles load, and neighbouring frames line up where they meet.

## P39: post-acquisition stitching

Run `cargo run --release -p ex-stitch`, then open `http://127.0.0.1:8243/`.

- [ ] **The blind stage's scan:** 9 tiles over 3 × 2 mm, where the stage said it was about 5.5 µm RMS off the truth.
- [ ] **The stitch:** 12 pairs kept of 12, missing by about 0.05 px RMS, the tiles about 0.05 µm RMS off the truth.
- [ ] **The page** shows the newest mosaic, 3822 × 2544 pixels, a thumbnail, and its numbers. Zoomed in, the tiles' edges don't show doubled.
- [ ] **The bench:** `cargo run --release -p ex-stitch -- --bench 40` stitches 40 tiles of 20 MP at about 10 a second. The owner's PC gave 9.5–11.9 over nine runs, and plan §5.20 asks for 10. It holds at most 256 MB of parts.

## P40: deep zoom; Checkpoint D

Run `cargo run --release -p ex-deep-zoom`, then open `http://127.0.0.1:8244/`.

- [ ] It stitches a snake over 4 × 3 mm into a raw mosaic of about 5060 × 3372 pixels in 14 levels. Its colour overview and coarse levels are ready about a second after the stitch began.
- [ ] **The page** opens the mosaic in OpenSeadragon, in colour. Zoom from the whole of it down to its pixels: finer tiles load as you go.
- [ ] Move *Black*, *White* and *Gamma*: the view changes, its tiles made afresh. *As made* goes back.
- [ ] **In the browser's developer tools** (Network), each tile's `Server-Timing` says whether it was made or kept, with the server's time. A tile asked for again is kept.
- [ ] **The bench:** `cargo run --release -p ex-deep-zoom -- --bench 200`. A cold tile takes about 5 ms of server time, and a kept one under 1 ms. A full-screen view never seen takes under 150 ms, with and without a hardware job.

**Checkpoint D:** run `cargo run --release -p ex-checkpoint-d`, then open `http://127.0.0.1:8245/`. It takes about two and a half minutes.

- [ ] It prints:
  - the overview's tissue, about 24 mm²;
  - the tissue's scan: 25 tiles, all 25 drawn on the live mosaic as they were taken;
  - the stitch: 40 pairs kept of 40;
  - the mosaic, in 14 deep-zoom levels.
- [ ] The page browses the stitched mosaic in OpenSeadragon, with levels and gamma, as `ex-deep-zoom`'s does.

## P41: volumes

Run `cargo run --release -p ex-volume`, then open `http://127.0.0.1:8246/`.

- [ ] It prints a z-scan of 21 planes through the fluorescent cells at 50× and a confocal, in about 24 s. The raw volume is 1368 × 912 × 21 voxels of 0.192 × 0.192 × 0.5 µm, served from level 1: [684,456,21].
- [ ] **The page** draws the volume as a maximum intensity projection, seen from above the slide and in front of it. The cells are domes on the glass, their membranes bright at their edges, in green.
- [ ] Drag to turn it, and scroll to zoom. *Composited* draws it through its transfer function.
- [ ] *Black* and *White* move the window of intensities. *Depth ×* stretches z up to 8 times.

## P42: depth from focus

Run `cargo run --release -p ex-depth-from-focus`, then open `http://127.0.0.1:8247/`.

- [ ] It prints:
  - the z-scan over the die's dome, in about 20 s;
  - depth from focus in under a second, over 684 × 456 cells, about 73 % sure;
  - the error against the truth, about 0.3 µm RMS and 0.05 µm the median.
- [ ] **The page** draws the heights as a surface coloured by the all-in-focus image: the dome in the middle, the pyramid's terraces left and right, the film's pits above and below, and the ridge's slopes at the corners.
- [ ] *Depth ×* stretches the heights up to 40 times. *Only where sure* leaves out the cells that aren't.
- [ ] **The mesh:** `.oscope/ex-depth-from-focus/data/projects/<project>/results/<heights>/exports/surface.ply`. If you have a mesh viewer, such as MeshLab or Blender, it opens there as the dome's surface.

## P43: deconvolution; Checkpoint E

Run `cargo run --release -p ex-deconvolve`, then open `http://127.0.0.1:8248/`.

- [ ] It prints:
  - the widefield z-scan through the cells, in about 20 s;
  - the deconvolution, about 11 s on the CPU, of 684 × 456 × 21 voxels;
  - the NRMSE against the cells' membranes, the deconvolution's a little under the stack's: about 1.15 against 1.17.
- [ ] **The page** shows the deconvolved volume as a maximum intensity projection. Choose *the stack* to see it before deconvolution: deconvolved, the light gathers towards the cells, most plainly from the side with the depth stretched.
- [ ] *Composited*, then *an isosurface*: the isosurface shows the cells' surfaces. *Level* moves it.
- [ ] *Depth ×* stretches z up to 8 times.
- [ ] **On the GPU** (optional, needs the CUDA toolkit): `cargo run --release -p ex-deconvolve --features oscope-api/cuda` deconvolves on the GPU with the same NRMSE, in about 1.4 s against the CPU's 11.

**Checkpoint E:** run `cargo run --release -p ex-checkpoint-e`, then open `http://127.0.0.1:8249/`. It takes about 85 s.

- [ ] It prints the chip's dome in heights and the cells deconvolved.
- [ ] **The page** holds both examples' pages side by side: the chip's surface and the cells' volume, each with its own controls working.
