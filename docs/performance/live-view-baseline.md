# The live view: its architecture and a baseline benchmark

- **What:** `ex-live-view` (P17's example), measured step by step from the simulated camera's rendering to the frame drawn in the page, before any optimisation. It is the baseline later optimisations are measured against.
- **When:** 2026-11-20, on the branch `perf/live-view-baseline`, from `main` at `5bfba25` (W4.3).
- **Set-up:** the simulator on the MIChrome 20's full 5472 × 3648 sensor. The page always shows 1000 × 667 of the whole sensor, at JPEG quality 80.
- **Two builds:** `cargo run --release -p ex-live-view`, and the same with `--features cuda`.
- **The machine:** the owner's laptop, an MSI Stealth GS66 12UGS. An i7-12700H, 14 cores and 20 threads; 16 GB; an RTX 3070 Ti Laptop GPU with 8 GB, driver 621.24. Windows 11 Pro, on mains power. §5.20's budgets are for this laptop.
- **The browser:** headless Chrome 157, which draws WebGL on the RTX 3070 Ti through ANGLE and Direct3D 11.
- **The raw results** are in [`data/`](data/README.md).

## Summary

| Stage still, means | CPU build | CUDA build |
|---|---:|---:|
| Frames a second the page draws | 5.0–5.3 | 5.9–6.0 |
| The camera's own rate, as set (mode `full`, 16 bits) | 7.24 | 7.24 |
| Frames a second the simulated camera manages | 5.7–6.0 | 5.9–6.0 |
| The camera's rendering, per frame | 158–166 ms | 159–161 ms |
| The preview, raw frame to JPEG | 184–194 ms | 128–135 ms |
| The same preview with nothing else running | 29 ms | 13.5 ms |
| Server latency: exposure end to message ready | 467–491 ms | 361–372 ms |
| **End to end: exposure end to frame drawn** | **476–501 ms** | **371–382 ms** |
| The page's share: transit, decoding, drawing | 10–11 ms | 9 ms |

| Jogging x at 500 µm/s, means | CPU build | CUDA build |
|---|---:|---:|
| Frames a second the page draws | 0.42–0.43 | 0.40–0.43 |
| The camera's rendering, per frame | 2.27–2.30 s | 2.00–2.08 s |
| of which new tiles of the slide | 1.16–1.34 s | 1.14–1.16 s |
| of which the motion-blurred exposure | 1.02–1.08 s | 0.91–0.98 s |
| **End to end** | **2.5 s** | **2.5–2.7 s** |

The ranges are two runs of each build. What the numbers say:

1. **The simulated camera is the main cost, and it always renders on the CPU.**
   - With the stage still, a frame takes ~160 ms, of which 155–162 ms is the per-pixel exposure on all 20 threads. That is longer than the camera's 138 ms interval, so it skips about one exposure in five.
   - With the stage moving, it takes over 2 s. New tiles of the slide take half of that, and an exposure that samples the slide at up to 32 places per pixel for motion blur takes the other half.
   - `--features cuda` doesn't move the camera to the GPU: only the preview's steps. The camera's GPU path (P15) renders a still frame in 17 ms rather than 143 ms, but nothing in the server turns it on.
2. **The preview is fast alone and slow beside the camera.** It takes 29 ms on the CPU and 13.5 ms on the GPU with nothing else running, but 6.7 and 9.5 times that in the live view. The camera's 20 threads compete with the live pool's 5 for 20 hardware threads.
3. **Within the camera** (§2.6), **the sensor's noise is over half of a still exposure:** 68–80 ms of 130–146 ms. **The procedural H&E section is 91% of each tile's cost.** Focus, the objective and the light path decide how often every tile must be rendered again.
4. **The wire and the page are small:** transit 1–2 ms, decoding 6 ms, drawing 2–3 ms. Nothing there needs work yet.
5. **On this laptop, §5.20's preview budget is met on the CPU and is marginal on the GPU:** 49–53 ms against 60, and 29.8–35.3 ms against 30, at 1920 px (§4.6).

§5 ranks what to optimise. The first two items: render the camera on the GPU in the server, and give the camera's rendering threads of its own, so it can't crowd out the preview.

## 1. How a frame gets from the simulator to the page

`ex-live-view` serves the simulated microscope and a page that shows the `preview` Thing's stream (P17, [notes](../milestone-1/notes/P17-preview.md), ADR-0014). This section follows one frame through it, with the threads each step runs on.

```text
 oscope-sim                     oscope-acquire              oscope-api + teta-wot              browser
 ─────────────────────────      ────────────────────        ───────────────────────────        ───────────────────
 "simulated camera" thread      the camera's sink           one task per WebSocket             main thread
   wait for exposure end  ──►     wait for the stage's  ──►   take the newest frame   ──►        onmessage
   (sim clock, rolling)           position, seal the          wait for the ack (≤ 2 ahead)       Blob → createImageBitmap
   render: tiles, motion,         frame, publish on the       spawn_blocking:                    (decoded off the main thread)
   pixels (rayon, 20 thr.)        frame bus (latest-wins      Preview::render in the             WebGL: texImage2D, draw
   pack u16 into a pooled         slot per subscriber)        live pool (rayon, 5 thr.)          ack the next message ──┐
   40 MB slab                                                 header + JPEG → one-slot sink                            │
                                                              writer task → WebSocket ──► TCP loopback                 │
                                                                     ▲                                                 │
                                                                     └──────────────── {"ack": n + 1} ─────────────────┘
```

### 1.1 The simulated camera (`oscope-sim`, `camera.rs`)

- **The camera's own thread,** `simulated camera`, runs free at the mode's rate: `full` in 16 bits is the MIChrome 20's 7.24 fps, one frame every 138 ms.
- **Each frame is exposed on the simulation clock,** which runs in real time once the stage has homed. It has a rolling shutter: each of the 3648 rows starts 16.4 µs after the one above (60 ms of readout over the frame), and is exposed for 15 ms. So the frame exists 75 ms after its first row started, and the thread sleeps until then.
- **Then it renders the frame** (`Inner::render`). §2 describes the models:
  1. **The slide's tiles** (`camera.field`): rendered images of the slide, from a cache.
  2. **The stage's places** (`camera.motion`) during each row's exposure: one when it is still, and up to 64 while it moves.
  3. **Each pixel** (`camera.expose`): 20 million of them, sampled from the tiles and put through the sensor's model. Rows run in parallel on rayon's global pool, all 20 threads.
- **The counts are packed** (`camera.pack`) into a slab from the frame pool: 8 slabs of 40 MB on the heap. 16-bit samples are aligned high. This loop runs on the camera's thread alone.
- **If a frame takes longer than the interval,** the frames whose exposures ended meanwhile are skipped and counted. The next frame then starts late (`camera.wake_late`).
- **The GPU can render frames** (`SimCamera::use_gpu`, P15), but nothing in the server calls it. `ex-live-view`'s `cuda` feature turns on `oscope-api/cuda`, which reaches the preview's steps and not `oscope-sim/cuda`. **In both builds measured here, the camera renders on the CPU.**

### 1.2 Acquisition and the frame bus (`oscope-acquire`)

- **The camera's sink** (`Acquisition::sink`), on the camera's thread:
  - It waits, up to 50 ms, for the stage to report a position after the exposure's midpoint, and interpolates it (`acquisition.position_wait`).
  - It adds the objective and the lights.
  - It seals the frame and publishes it (`acquisition.publish`).
- **The frame bus** gives every subscriber a clone of the same `Frame`, an `Arc` with no copy. Each preview session subscribes latest-wins: one slot, where a new frame replaces one not yet taken.

### 1.3 The preview session (`oscope-api`'s `preview` Thing, on `teta-wot`)

- **Each WebSocket** on `/api/v1/preview/stream` is a `teta-wot` binary stream, with a tokio task running `PreviewThing::stream`. In a loop:
  1. It sleeps until its next turn (`max_fps`, 30 by default), then takes the newest frame from its slot. `stream.frame_age` is that frame's age then, from the end of the camera's packing.
  2. If the page hasn't acknowledged, it waits (`stream.ack_wait`). It keeps at most 2 messages unshown, and then takes the newest frame.
  3. It hands the frame to tokio's blocking pool (`spawn_blocking`, `stream.spawn`), where `Preview::render` runs in the compute layer's **live pool**: rayon, a quarter of the threads, here 5 of 20.
  4. It writes ADR-0014's 76-byte header and the JPEG into one message (`stream.message`) and puts it in `teta-wot`'s one-slot sink (`stream.send`). A writer task sends the slot's message as a binary WebSocket frame, through axum and tungstenite, over TCP.
- **The server's latency,** `stream.latency`, runs from the frame's exposure end to the message being ready: both on the camera's clock, in the header.

### 1.4 The preview chain (`oscope-acquire::preview`)

For 1000 × 667 of the whole 5472 × 3648 sensor:

| # | Step | What it does here | Size after it |
|---|---|---|---|
| 1 | ROI and size | The whole frame, fitted into 1000 × 667 | |
| 2 | `to_f32` | The mosaic's 16-bit samples to floats from 0 to 1 | 5472 × 3648 × 1, 80 MB |
| 3 | `demosaic` | P08's 2 × 2 superpixel, since the output is under half the frame each way: it bins as it goes | 2736 × 1824 × 3, 60 MB |
| 4 | `bin` | The largest whole factor that keeps the image at least 1000 × 667: 2 (`downsample`, a box) | 1368 × 912 × 3, 15 MB |
| 5 | `black` | The frame's black level taken off | |
| 6 | `correct` | White balance: skipped, with the look's gains of 1 | |
| 7 | `to_display` | The display transform, sRGB's curve, to 8 bits | 1368 × 912 × 3, 3.7 MB |
| 8 | `resize` | A box filter over the factor of 1.37 left | 1000 × 667 × 3, 2.0 MB |
| 9 | `jpeg` | OScope's baseline encoder (ITU-T T.81), quality 80, 4:2:0, a restart interval for each row of 16 × 16 units, encoded in parallel | 110 KB still, 86 KB jogging |

- **On the CPU,** every step runs in the live pool.
- **With `--features cuda`,** steps 2–7 run on the GPU through P08c's CUDA twins: the 40 MB mosaic is uploaded from the heap, which isn't pinned memory, and the 3.7 MB of 8-bit RGB is downloaded. Steps 8 and 9 stay on the CPU.

### 1.5 The page (`examples/ex-live-view/page`)

- **The WebSocket** delivers each message as an `ArrayBuffer`. The page reads the header, wraps the JPEG in a `Blob`, and decodes it with `createImageBitmap`, which Chrome does off the main thread.
- **WebGL 1** draws it: `texImage2D` uploads the bitmap as a texture, and one textured quad fits it to the 1000 × 667 canvas.
- **Then the page acknowledges** the next message it's ready for, so the server never has more than two unshown messages in flight.

### 1.6 Key technologies

| Layer | Technologies |
|---|---|
| The simulator | Rust; rayon's global pool, a row per task; a procedural H&E section from seeded hashes; an LRU tile cache; a simulation clock with a rolling shutter; CUDA (cudarc, NVRTC kernels) available but not used by the server |
| Frames | Pooled slabs (`FramePool`, heap memory); sealed, `Arc`-shared `Frame`s; the frame bus, with lossless and latest-wins subscribers (ADR-0008) |
| The server | tokio; axum through `teta-wot`'s W3C WoT Things; a binary WebSocket stream with a one-slot sink |
| Compute | `oscope-compute`: two rayon pools, live and batch, so batch work can't hold up the preview; CUDA through cudarc, with a CPU twin for every kernel (ADR-0010) |
| Imaging | Superpixel and bilinear demosaicing, box binning, a display transform, and OScope's own JPEG encoder, parallel across restart intervals |
| The wire | ADR-0014: a 76-byte header with the frame's times on the camera's clock, then the JPEG; acknowledgements for flow control |
| The browser | WebSocket, `Blob`, `createImageBitmap`, WebGL 1 |

### 1.7 Threads, and what shares the processor

- **The camera's rendering** uses rayon's global pool: one thread per hardware thread, 20 here.
- **The preview** uses the live pool's 5 threads, called from a tokio blocking thread.
- **tokio's workers** (20) run the sessions, the HTTP server and the WebSocket writers.
- **Chrome,** on the same laptop, decodes and draws.

The i7-12700H has 6 performance cores with two threads each, and 8 efficiency cores: 20 threads in all. The camera alone keeps them busy: its frames take longer than its interval, so it starts the next as soon as one is done. The preview's 5 threads compete with its 20 for the same cores.

## 2. How the simulated camera makes its image

The camera doesn't hold an image of the slide. It renders the slide on demand from a procedural model, through models of the objective, the light and the sensor (ADR-0013). This section follows that chain for the live view's set-up, then gives what each model costs (§2.6).

```text
 the sample (HeTissue)          the world (World::render)            the tile cache              the exposure                  the sensor
 ────────────────────           ─────────────────────────            ──────────────              ────────────                  ──────────
 per pixel of each layer:  ──►  each layer: OD → 10^−OD,       ──►  256-px tiles of the   ──►  each pixel: mounting,   ──►  electrons: Poisson
 islands, holes, thickness,     blurred by the objective's          slide's light, linear       Bayer channel, the tiles      shot noise, read noise,
 nuclei on an 11 µm grid,       PSF at its distance from focus;     RGB at the sample's         sampled at the stage's        dark current; gain;
 chromatin, cytoplasm,          layers multiplied; the glass's      pixel, keyed by objective,  places during its row's       12-bit ADC, black 125,
 stroma: optical densities      surfaces and edges                  light and depth bucket      exposure; vignetting          aligned high in 16 bits
```

### 2.1 The sample: the slide, the section and its cells

- **The world** holds one slide, 75 × 25 mm, with its corner at the stage's origin, under a 50 × 24 mm coverslip. Its surface is in focus at z = 5000 µm, where `ex-live-view` puts the stage. The configuration loads `H&E tissue` (`SlideLoader`), which is `HeTissue::new(seed, coverslip)`.
- **The section** (`tissue.rs`): 4 µm thick, in 3–5 elliptical islands with rough fractal edges, holes on a 300 µm grid, and a thickness that varies with fractal noise and thins towards the edges. It has **two layers,** its lower and upper halves, 2 µm each at 1 and 3 µm above the slide.
- **The cells:**
  - **Nuclei** lie on a jittered 11 µm grid, at most one in each square, at a density from 25% to 85% that varies over 250 µm. Each is an ellipse with a semi-major axis of 2.8–4.6 µm, at a random angle, in one of the two layers. Its hematoxylin varies from 0.75 to 1.25, textured by a chromatin noise of 1.6 µm.
  - **Cytoplasm** is a ring 2.5 µm around each nucleus, stained by eosin.
  - **The stroma** between the cells is fibres: six octaves of noise stretched along one direction and warped.
- **Stains become colour by Beer–Lambert:** each stain's optical density per µm, times its RGB vector (Ruifrok and Johnston, 2001), times the layer's thickness.
- **No cell is stored.** Every choice is a hash of the seed, a salt and a grid cell, so any region renders the same, in any order, on any thread. The cost is that **each pixel of each layer recomputes its neighbourhood:**
  - the 9 nearest nucleus squares, each a hash and a value noise;
  - the 9 nearest hole squares;
  - the distance to each island;
  - and up to 3 + 3 + 6 + 2 octaves of value noise, each 4 hashes.
- **Each pixel is the mean over its square:** octaves finer than half a pixel are replaced by their mean. So coarser pixels are a little cheaper, and a coarse rendering matches a fine one averaged down.
- **The other sample with cells,** `CellCulture`, is cells in 3D labelled with three fluorescent dyes, for fluorescence and confocal stacks. It renders through the same world and tiles, in fluorescent light; it isn't in the live view.

### 2.2 The objective: optics, blur and depth of field

- **The objective** fitted (`SimMount::fit`) becomes the camera's `Optics`: its NA and magnification, and the sensor's 2.4 µm pixel. The pixel at the sample is 2.4 µm over the magnification.
- **The point spread function is a Gaussian** (`Optics::blur`): σ = √((0.21 λ/NA)² + (|dz| tan θ / 2)²), with sin θ = NA, in green light of 0.55 µm. In focus that is diffraction's best Gaussian fit (Zhang, Zerubia and Olivo-Marin, 2007); out of focus, geometric optics' cone of light.
- **Each layer is blurred** at its own distance from focus (`blur.rs`):
  - below 8 pixels, by a kernel 6σ wide;
  - up to 64 pixels, by four box filters (Kovesi, 2010);
  - beyond that, by rendering coarser and enlarging.
  - Each tile renders a margin of 3σ + 1 pixels around it for the blur: **defocus makes every tile larger.**
- **Depth of field** sets the depth buckets: a tile renders at the middle of a bucket a sixteenth of the depth of field deep. **A focus change of more than one bucket re-renders every tile.**

| Objective | Pixel at the sample | Field of view | σ in focus | Depth of field | A depth bucket |
|---|---:|---:|---:|---:|---:|
| 5× / 0.14 | 0.48 µm | 2.63 × 1.75 mm | 1.7 px | 29.8 µm | 1.9 µm |
| **10× / 0.28,** the live view's | 0.24 µm | 1.31 × 0.88 mm | 1.7 px | 7.9 µm | 0.49 µm |
| 20× / 0.42 | 0.12 µm | 0.66 × 0.44 mm | 2.3 px | 3.4 µm | 0.21 µm |
| 50× / 0.55 | 0.048 µm | 0.26 × 0.18 mm | 4.4 px | 1.9 µm | 0.12 µm |

A frame always needs about 330–345 tiles, since tiles are counted in pixels: a higher magnification covers less slide with the same tiles.

### 2.3 The illumination

- **The lights** (`SimLights`) give the camera a light path and an intensity: `transmitted` at 0.6 in the live view.
  - **The path** (transmitted, reflected or fluorescence) chooses which layers the world renders, and is part of each tile's key: changing it re-renders every tile.
  - **The intensity** only scales the electrons per pixel during the exposure, so it costs nothing.
- **The glass:** the slide's and coverslip's four surfaces each pass 96% (Fresnel's 4% lost), and their edges darken a 30 µm band.
- **Vignetting** (P33, `1 / (1 + s r²)²`) and **the sensor's colour response** (a 3 × 3 matrix on each tile) are off by default.
- **Fluorescence** has dyes, excitation lines and a confocal's sectioning; the live view doesn't use it.

### 2.4 From the tiles to each pixel: the exposure

- **The tiles:** for each frame, the camera asks the cache for the tiles covering wherever the sensor's footprint was during the frame, with a margin.
  - Each tile is 256 pixels square, plus one overlapping row and column, of linear RGB floats at the sample's pixel: 792 KB.
  - The cache keeps 2 GB, least recently used first: 2709 tiles. A still frame needs 330–345 of them, 261 MB.
- **Motion:** for each row, the stage's true positions during its 15 ms exposure. That is one place when still, and one per pixel moved, up to 64, when moving. At 500 µm/s the stage moves 7.5 µm, 31 pixels, during each row's exposure, so each pixel is sampled at 32 places.
- **Each pixel:**
  - its centre through the mounting (rotation, mirroring, offset) onto the stage;
  - its RGGB channel;
  - the tiles sampled bilinearly at each place, and averaged;
  - the vignetting;
  - then the sensor.

### 2.5 The sensor and its noise

- **Electrons:** the light times 800 e⁻/ms at full intensity, times the exposure, so about 6,000 e⁻ through the glass at 0.6 and 15 ms, of a 15,000 e⁻ full well. Dark current adds 0.5 e⁻/s.
- **Noise** (`SensorModel::count`): Poisson shot noise, which is a rounded normal above 30 e⁻, as here; and Gaussian read noise of 2.5 e⁻. Each pixel draws **two Box–Muller normals,** each with a logarithm, a square root and a cosine, from SplitMix64 seeded by the frame and the row, so that any thread draws the same noise.
- **The ADC:** the gain, a 12-bit ADC with a black level of 125, aligned high in 16 bits: 2000 black, 65,520 full.

### 2.6 What each model costs

[`crates/oscope-sim/tests/models.rs`](../../crates/oscope-sim/tests/models.rs), ignored by default, measures them. It renders the full sensor over the section with the live view's models, then changes one model at a time. It uses a stepped clock, with nothing else running.

**Frames,** in ms, on 20 threads:

| Variant | Cold frame | Its tiles | Warm frame | its tiles | motion | exposure | packing |
|---|---:|---:|---:|---:|---:|---:|---:|
| **As the live view** | 4458 | 345 | 132.6 | 0.1 | 2.6 | 129.8 | 4.6 |
| The sensor without noise | 4467 | 345 | **64.0** | 0.1 | 2.6 | **61.3** | 4.2 |
| No slide | **149** | 330 | 137.0 | 0.1 | 2.6 | 134.2 | 5.3 |
| 5× | 4477 | 345 | 137.6 | 0.1 | 2.5 | 134.9 | 4.2 |
| 20× | 5040 | 330 | 138.5 | 0.1 | 2.7 | 135.6 | 4.2 |
| 50× | **7708** | 330 | 140.8 | 0.1 | 2.6 | 138.1 | 4.7 |
| 10 µm out of focus | **6340** | 345 | 142.5 | 0.1 | 2.7 | 139.6 | 4.3 |
| 50 µm out of focus | **12142** | 345 | 140.9 | 0.1 | 2.8 | 138.0 | 4.3 |
| The light at full intensity | 4875 | 345 | 144.7 | 0.1 | 2.7 | 141.9 | 4.5 |
| Vignetting of 0.3 | 4829 | 345 | 144.8 | 0.1 | 2.7 | 141.8 | 4.3 |
| Jogging at 200 µm/s | 4961 | 345 | **506.7** | 0.1 ¹ | 12.7 | **492.8** | 4.5 |
| Jogging at 500 µm/s | 4878 | 345 | **1220.4** | **204.2** ² | 27.3 | **980.3** | 4.2 |

¹ 6 new tiles a frame on average, in a column every two frames or so: the median frame has none. ² 15 new tiles a frame. On the stepped clock the frames are 138 ms of stage travel apart; in the live view a 2 s frame lets the stage move 1 mm, so each frame needs far more new tiles (§4.2).

These are the second of two runs. The first agreed within about 10% with two exceptions. Its cold frames were slower, 4.8–16 s. Its warm frames at 50× and 50 µm out of focus took ~257 ms, and so did their packing, which doesn't depend on the optics. So that was the laptop, not the models. Warm frames drift up by ~9% through a run, as the laptop warms. The tile and pixel timings below repeated within 2%.

**One tile on one thread,** in ms:

| Objective, focus | The two layers | Their blur | The whole tile |
|---|---:|---:|---:|
| 5×, in focus | 115.2 | 9.8 | 123.0 |
| **10×, in focus** | **111.3** | **9.5** | **121.5** |
| 20×, in focus | 114.9 | 12.0 | 137.2 |
| 50×, in focus | 161.8 | 32.8 | 198.1 |
| 10×, 10 µm out of focus | 132.2 | 31.7 | 163.9 |
| 10×, 50 µm out of focus | 300.3 | 21.6 | 320.1 |

**One pixel's exposure on one thread,** in ns:

| The mounting | One sample of the tiles | The vignetting | The sensor's counts, with noise | without |
|---:|---:|---:|---:|---:|
| 3.6 | 18.4 | 1.8 | 44.1 | 6.2 |

**What that says:**

- **The sample decides the cold frame:** 149 ms with no slide, and 4.5–5.0 s with the section.
  - The section's two layers are 91% of a tile's time, and the blur 8%.
  - Each layer pixel costs about 0.75 µs, mostly hashing. Each one recomputes its neighbourhood of nuclei, holes and noise, and **both layers recompute all of it,** though only the nuclei differ between them.
  - Once the tiles are cached, the sample costs nothing: a warm frame over the section takes what one over no slide does.
- **The sensor's noise is over half of a warm frame's exposure:** 68–80 ms of 130–146 ms, in the two runs. Per pixel, the counts take 44 ns with noise and 6 ns without: that is the two Box–Muller normals. One sample of the tiles takes 18 ns.
- **The objective costs through the blur.** Per tile, 5× and 10× cost the same, 20× is 13% dearer for its wider kernel, and 50× 63% dearer, with a 27-tap kernel and a 14-pixel margin. Once the tiles are cached, the objective costs nothing per frame, but at a higher magnification each µm of stage travel crosses more tiles.
- **The focus costs through the blur's margin:** 10 µm out of focus a tile is 35% dearer, and 50 µm out 2.6 times. A z move of more than half a µm at 10× changes the depth bucket, and then **every tile is rendered again.** So focusing at 10× costs a cold frame each time it crosses a bucket.
- **The illumination costs nothing per frame:** the intensity only scales the signal, and vignetting is computed for every pixel whether it is on or not. Differences of up to 9% between these variants are within a run's drift.
- **Motion multiplies the exposure:** each place is another sample of the tiles for every pixel.
  - 200 µm/s means 14 places, and an exposure of 490 ms.
  - 500 µm/s means 32 places, and 980 ms.
  - It also adds new tiles: about 122 a second at 500 µm/s. Each is 121.5 ms of one thread's time, or ~14 ms on 20 threads when 15 come at once.

## 3. The benchmark

### 3.1 What changed in the example

- **The full sensor:** `ex-live-view` now simulates the MIChrome 20's 5472 × 3648 sensor rather than the scaled-down 1368 × 912. The owner had made this change already.
- **A fixed view:** the page always asks for 1000 × 667 of the whole sensor, at quality 80, and draws it on a 1000 × 667 canvas. Zooming is gone.
- **Timing is on,** and the server's step times are served each second as `/bench/timings.json`.
- **The page** measures its own steps and shows every step's times in a table under the image. **Copy the timings** gives them all as JSON.
- **`--smoke`** asks for 1000 × 667, and checks that the timings are served.

### 3.2 How each step is timed

- [`oscope_core::timing`](../../crates/oscope-core/src/timing.rs) keeps, for each named step, how long it took over the last 30 s.
  - It is off unless a program turns it on, and then a step costs one atomic load. `ex-live-view` turns it on.
  - With it on, the GPU's steps wait for the GPU to finish each step (`synchronize`), so that each step's time is its own. Without that, a kernel's time would show up in the next step that waits.

| Step | Where | What it times |
|---|---|---|
| `camera.wake_late` | `oscope-sim` `camera.rs` | How late the camera's thread starts a frame after its exposure ended |
| `camera.render` | | The whole rendering: |
| `camera.field` | | the tiles, rendered when not cached |
| `camera.motion` | | the stage's places during each row |
| `camera.expose`, `camera.expose_gpu` | | every pixel, on the CPU or the GPU |
| `camera.pack` | | The counts packed into the frame's slab |
| `acquisition.position_wait` | `oscope-acquire` `acquisition.rs` | The wait for the stage's position |
| `acquisition.publish` | | Sealing and publishing the frame |
| `stream.frame_age` | `oscope-api` `preview.rs` | The frame's age, from the end of packing, when the session takes it |
| `stream.ack_wait` | | The wait for the page's acknowledgement |
| `stream.spawn` | | The wait for a blocking thread |
| `preview.render` | `oscope-acquire` `preview/mod.rs` | The whole chain, and each of its steps: `preview.to_f32`, `demosaic`, `bin`, `black`, `correct`, `to_display` on the CPU, or `preview.gpu.*` on the GPU; `resize`; `jpeg` |
| `stream.message`, `stream.send` | `oscope-api` `preview.rs` | The message made, and put in the one-slot sink |
| `stream.latency` | | Exposure end to message ready, from the header |
| `page.transit` | the page | Message ready to its arrival in the page |
| `page.decode` | | `createImageBitmap`, with the `Blob` |
| `page.draw` | | `texImage2D` and the draw |
| `page.next_frame` | | From the draw to the next animation frame |
| `page.end_to_end` | | Exposure end to the frame drawn |
| `page.interval` | | Between frames drawn: the frame rate |

**The page and the server's clock:**

- The header's times are on the camera's clock, the simulation's, which started at 0.
- `/bench/timings.json` gives the offset from that clock to the UNIX epoch, taken each second.
- The page adds it, and compares the result with `performance.timeOrigin + performance.now()`. Both clocks are this laptop's, so the offset is good to about a millisecond.
- The check: in every run with the stage still, the steps' means add up to the measured `stream.latency` and `page.end_to_end` within 2 ms.

### 3.3 The runs

- **The harness** is [`examples/ex-live-view/bench/bench.mjs`](../../examples/ex-live-view/bench/bench.mjs). It opens the page in headless Chrome with Playwright, then records three windows of 30 s:
  - **start:** from the page's first frame, with the camera's tiles rendered for the first time;
  - **still:** the stage at rest;
  - **jog:** jogging x + at 500 µm/s, from x 40.0 mm to 55.7 mm.
- **Each run** starts the example from a fresh data root, so it has no saved settings. Each build ran twice.
- **Other measurements:**
  - the preview with nothing beside it: `the_live_view_s_preview`, added to `oscope-acquire`'s ignored benchmarks;
  - P17's `the_preview_budget`, at 1920 px;
  - `ex-sim-bench`, the camera alone;
  - `oscope-sim`'s new `models` benchmark (§2.6).

### 3.4 Running it again

```sh
cargo run --release -p ex-live-view                    # or with --features cuda
node examples/ex-live-view/bench/bench.mjs --label cpu # in another terminal; needs `npm ci` in web/
cargo test --release -p oscope-acquire --features cuda --test preview -- --ignored --nocapture --test-threads=1
cargo test --release -p oscope-sim --test models -- --ignored --nocapture --test-threads=1
cargo run --release -p ex-sim-bench --features cuda
```

## 4. Results

### 4.1 The stage still

Run 1 of each build, in ms: the per-second rate, mean, median and 95th percentile. Run 2 is in [`data/`](data/README.md), within 5%.

| Step | CPU /s | mean | p50 | p95 | CUDA /s | mean | p50 | p95 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `camera.wake_late` | 5.7 | 68.7 | 70.0 | 130.3 | 6.0 | 68.8 | 69.5 | 128.6 |
| `camera.render` | 5.7 | 165.7 | 165.1 | 179.1 | 6.0 | 159.0 | 158.6 | 162.9 |
| &nbsp;&nbsp;`camera.field` | 5.7 | 0.1 | 0.1 | 0.2 | 6.0 | 0.1 | 0.1 | 0.1 |
| &nbsp;&nbsp;`camera.motion` | 5.7 | 3.5 | 3.5 | 3.9 | 6.0 | 3.5 | 3.5 | 3.8 |
| &nbsp;&nbsp;`camera.expose` | 5.7 | 161.9 | 161.3 | 175.3 | 6.0 | 155.3 | 154.9 | 159.3 |
| `camera.pack` | 5.7 | 6.7 | 6.1 | 10.8 | 6.0 | 5.2 | 5.1 | 6.6 |
| `acquisition.position_wait` | 5.7 | 0.0 | 0.0 | 0.0 | 6.0 | 0.0 | 0.0 | 0.0 |
| `acquisition.publish` | 5.7 | 0.0 | 0.0 | 0.0 | 6.0 | 0.0 | 0.0 | 0.0 |
| `stream.frame_age` | 5.0 | 54.6 | 39.9 | 168.4 | 6.0 | 0.1 | 0.1 | 0.1 |
| `stream.ack_wait` | 5.0 | 0.0 | 0.0 | 0.0 | 6.0 | 0.0 | 0.0 | 0.0 |
| `stream.spawn` | 5.0 | 0.1 | 0.0 | 0.1 | 6.0 | 0.0 | 0.1 | 0.1 |
| `preview.render` | 5.0 | 194.3 | 185.3 | 300.8 | 6.0 | 127.9 | 137.0 | 162.7 |
| &nbsp;&nbsp;`to_f32` | 5.0 | 38.5 | 38.9 | 74.9 | 6.0 | 28.4 | 19.8 | 50.2 |
| &nbsp;&nbsp;`demosaic` | 5.0 | 39.4 | 34.6 | 80.7 | 6.0 | 8.4 | 5.3 | 15.9 |
| &nbsp;&nbsp;`bin` | 5.0 | 30.1 | 26.6 | 62.4 | 6.0 | 4.1 | 2.5 | 6.3 |
| &nbsp;&nbsp;`black` | 5.0 | 6.6 | 1.4 | 35.8 | 6.0 | 1.3 | 0.8 | 2.2 |
| &nbsp;&nbsp;`to_display` | 5.0 | 29.7 | 19.2 | 88.8 | 6.0 | 6.5 | 6.2 | 11.8 |
| &nbsp;&nbsp;`resize` | 5.0 | 23.0 | 15.6 | 67.1 | 6.0 | 45.8 | 45.7 | 76.8 |
| &nbsp;&nbsp;`jpeg` | 5.0 | 14.4 | 10.7 | 38.3 | 6.0 | 31.4 | 31.2 | 50.4 |
| `stream.message` | 5.0 | 0.0 | 0.0 | 0.0 | 6.0 | 0.0 | 0.0 | 0.0 |
| `stream.send` | 5.0 | 0.0 | 0.0 | 0.0 | 6.0 | 0.0 | 0.0 | 0.0 |
| **`stream.latency`** | 5.0 | **490.7** | 477.3 | 629.1 | 6.0 | **361.0** | 367.7 | 441.7 |
| `page.transit` | 5.0 | 2.0 | 1.2 | 3.5 | 6.0 | 1.5 | 1.0 | 2.5 |
| `page.decode` | 5.0 | 6.3 | 6.3 | 7.5 | 6.0 | 5.6 | 5.8 | 6.3 |
| `page.draw` | 5.0 | 2.7 | 2.4 | 4.1 | 6.0 | 2.1 | 2.1 | 2.4 |
| `page.next_frame` | 5.0 | 1.3 | 0.6 | 3.6 | 6.0 | 1.0 | 0.5 | 5.1 |
| **`page.end_to_end`** | 5.0 | **501.3** | 488.4 | 639.7 | 6.0 | **371.4** | 377.1 | 451.4 |
| `page.interval` | 5.0 | 199.4 | 192.4 | 329.2 | 6.0 | 167.0 | 166.8 | 224.5 |

In the CUDA build, the preview's steps 2–7 are `preview.gpu.*`, and `to_f32` includes the upload. The tile cache held its 330 tiles throughout, and no session dropped a message.

### 4.2 Jogging at 500 µm/s

Run 1, in ms. Each window has only 12–13 frames, so its p95 is close to its maximum.

| Step | CPU mean | p50 | p95 | CUDA mean | p50 | p95 |
|---|---:|---:|---:|---:|---:|---:|
| `camera.render` | 2270.2 | 1498.2 | 4072.6 | 2078.7 | 1485.7 | 3793.7 |
| &nbsp;&nbsp;`camera.field` | 1159.5 | 413.6 | 2914.2 | 1158.6 | 631.4 | 2709.7 |
| &nbsp;&nbsp;`camera.motion` | 25.5 | 25.3 | 26.0 | 26.9 | 27.3 | 27.6 |
| &nbsp;&nbsp;`camera.expose` | 1084.8 | 1078.4 | 1139.1 | 983.7 | 1049.0 | 1090.4 |
| `camera.pack` | 5.1 | 5.0 | 6.1 | 4.9 | 4.7 | 5.7 |
| `preview.render` | 161.1 | 136.4 | 226.1 | 127.1 | 133.9 | 170.1 |
| `stream.latency` | 2353.6 | 1736.1 | 4177.4 | 2144.4 | 1639.4 | 3999.6 |
| `page.end_to_end` | 2508.0 | 1877.5 | 4186.3 | 2454.7 | 1928.6 | 4008.7 |
| `page.interval` | 2262.8 | 1569.1 | 3947.7 | 2253.4 | 1701.1 | 3772.8 |

- **The tiles feed on themselves.** In 30 s of jogging the camera rendered about 2800 new tiles, ~5 ms each on 20 threads, and the cache reached its 2 GB budget and began to evict. A frame that takes 2 s lets the stage move 1 mm, most of a field of view, so the next frame needs ~250 new tiles. Slow frames make the next one slower.
- **The exposure is 6.7 times a still one,** from sampling 32 places per pixel.
- **The preview and the page are unchanged.** They only see one frame every 2.3 s.

### 4.3 The start

The first 30 s after the page connects include the first frame, whose 330 tiles take **4.2 s.** Its server latency was 4.3 s. After that the cache holds every tile, and the frames are as in §4.1.

### 4.4 Where the latency goes

The stage still, run 1's means, in ms:

| Part | CPU build | CUDA build |
|---|---:|---:|
| The camera starts late, behind on the frame before | 68.7 | 68.8 |
| The camera renders | 165.7 | 159.0 |
| It packs the counts | 6.7 | 5.2 |
| The frame waits in the session's slot (acquisition takes ~0) | 54.6 | 0.1 |
| The preview | 194.3 | 127.9 |
| The message, and the hand-over to the WebSocket | 0.1 | 0.1 |
| **Server latency** (measured: 490.7 and 361.0) | **490.1** | **361.1** |
| Transit, decoding, drawing | 11.0 | 9.2 |
| **End to end** (measured: 501.3 and 371.4) | **501.1** | **370.3** |

- **The camera is half of the CPU build's latency and two thirds of the CUDA build's.** That includes its late start, which is queueing: its rendering takes longer than its interval.
- **The CPU build's preview is slower than the camera** (194 against 172 ms a frame), so each frame also waits in the session's slot. In the CUDA build the preview keeps up, and the session takes each frame as it is published.

### 4.5 What limits the frame rate

- **The camera:** its rendering and packing take 164–172 ms a frame, against the 138 ms its mode allows. So it manages 5.7–6.0 fps of its 7.24, skipping about one exposure in five.
- **In the CPU build, the preview:** at 184–194 ms it can't keep up with the camera, so the page draws 5.0–5.3 fps, and frames are replaced in the slot before the session takes them.
- **In the CUDA build,** the page draws every frame the camera makes: 5.9–6.0 fps.
- **Jogging,** the camera alone: 0.4 fps.
- **The camera's mode caps it all at 7.24 fps** (`full`, 16 bits). Mode `bin2`, 8-bit frames, or the `design` mode at 14.5 fps would raise that cap. They would only help once the camera renders faster.

### 4.6 The preview with nothing beside it

`the_live_view_s_preview`, on a synthetic 20 MP mosaic with the same request: the median of 15 previews, in ms, beside the live view's run 1 medians.

| Step | CPU, alone | CPU, live | GPU, alone | GPU, live |
|---|---:|---:|---:|---:|
| `to_f32` (GPU: with the 40 MB upload) | 5.1 | 38.9 | 4.5 | 19.8 |
| `demosaic` | 5.4 | 34.6 | 0.6 | 5.3 |
| `bin` | 3.9 | 26.6 | 0.3 | 2.5 |
| `black` | – ¹ | 1.4 | – ¹ | 0.8 |
| `to_display` (GPU: with the 3.7 MB download) | 3.8 | 19.2 | 1.9 | 6.2 |
| `resize` | 3.5 | 15.6 | 3.8 | 45.7 |
| `jpeg` | 1.9 | 10.7 | 2.0 | 31.2 |
| **`preview.render`** | **28.9** | **185.3** | **13.5** | **137.0** |

¹ The synthetic frame has no black level, so the step doesn't run.

- **Every step is 4–8 times slower beside the camera,** since the camera's 20 threads leave the live pool's 5 waiting for cores.
- **The CUDA build's resize and JPEG suffer most:** 45.7 and 31.2 ms, against 15.6 and 10.7 in the CPU build, and 3.8 and 2.0 alone. While the GPU works, the live pool's other 4 threads have nothing to do and go to sleep. Once the GPU is done, they wait to be scheduled again behind the camera's threads. That is my reading of it rather than something measured; giving the camera threads of its own (§5) would test it.

**§5.20's preview budget on this laptop,** P17's `the_preview_budget`: a 20 MP frame to a JPEG at 1920 × 1280, quality 95, the median of 15.

| Path | This laptop, 4 runs | Budget | P17, on the i9-12900 PC |
|---|---|---|---|
| CPU | 49.4, 51.0, 51.2, 53.0 ms | ≤ 60 ms | 34.2 ms |
| GPU | 29.8, 29.9, 30.4, 35.3 ms | ≤ 30 ms | 22.0 ms |

The CPU path meets its budget. **The GPU path is at its budget, and over it in 2 runs of 4.** The test fails when it is over, and it is ignored by default.

### 4.7 The simulated camera alone

`ex-sim-bench --features cuda`: the camera alone in mode `design`, with 1 ms exposures, on a stepped clock; the median of 8 frames.

| Backend | Cold | Still | Jog 0.2 mm/s | Jog 1 mm/s |
|---|---:|---:|---:|---:|
| CPU | 4577 ms | 143 ms, 7.0 fps | 171 ms | 481 ms |
| GPU | 4536 ms | **17 ms, 57.5 fps** | 19 ms | 236 ms |

- **On the GPU, a still frame is 8 times faster.** §5.20 asks for 14.5 fps on the GPU and 5 on the CPU, and both are met.
- **Cold frames take as long on the GPU** as on the CPU, because the tiles are rendered on the CPU either way. Jogging at 1 mm/s, the tiles are most of the GPU's 236 ms.
- **Its jogging numbers understate the live view's.** Its exposures are 1 ms rather than 15, so there are fewer places to sample. And its frames are one interval of travel apart, not 2 s.

## 5. What to optimise

In order of what each would do for the live view.

1. **Render the simulated camera on the GPU in the server.**
   - `oscope-api`'s `cuda` feature should turn on `oscope-sim/cuda`, and the simulator should call `SimCamera::use_gpu` when there is a GPU, as `ex-sim-bench` and `ex-record` do.
   - A still frame would take ~17 ms rather than ~160, under the 138 ms interval, so the camera would reach its 7.24 fps.
   - The CPU would be free for the preview, which takes 13.5–29 ms alone.
   - My estimate for the stage still: ~60–80 ms end to end, from 371–501. The camera would no longer start late; it would render in ~20 ms and pack in 5; the preview would take 15–30; the page 10.
   - The GPU's 8 GB has room: 512 tile slots are 405 MB.
2. **Give the camera's rendering threads of its own,** for the CPU build and for the tiles in the CUDA build.
   - Today it uses rayon's global pool, all 20 threads, beside the live pool's 5. It could take the batch pool, or the threads less the live pool's, and a lower priority.
   - The preview would come back towards its 29 ms alone: about 150 ms less latency in the CPU build, for a slower camera.
   - It would also test §4.6's reading of the CUDA build's slow resize and JPEG.
3. **Make the slide's tiles cheaper,** for jogging, focusing, and every change of objective or light. Each tile is 121.5 ms of one thread at 10×, 91% of it the section's two layers.
   - **Both layers in one pass:** each layer pixel recomputes the islands, holes, thickness, stroma and warp, though only the nuclei differ between the layers. That could nearly halve a tile.
   - **Each nucleus once per tile:** today each is evaluated for every pixel of its 9 neighbouring squares, in both layers.
   - **Keep tiles off the frame's path:** render ahead of the motion in the background, and sample a coarser level until the fine tile is ready. These are the plan's "cached, pre-blurred levels of detail" for the simulator's 20 MP risk (§12).
   - **Cache the layers before their blur,** so that a focus change costs only the blur: 9.6 of 121.5 ms at 10×. Today every half µm of z at 10× re-renders every tile.
   - **Or render tiles on the GPU,** the section as a kernel, which would also help the GPU camera while jogging.
4. **Make motion blur cheaper.**
   - Each place is another 18 ns sample per pixel: 32 places at 500 µm/s with 15 ms exposures, 970–980 ms a frame on 20 threads.
   - The tiles could be blurred along the motion once per row, as a box filter or a running sum along x for a move in x, and then sampled once. Or the places could be capped, with each sample widened.
5. **Make the sensor's noise cheaper:** 68–80 ms of a still frame's 130–146 ms exposure.
   - Above 30 e⁻ both shot and read noise are Gaussian, so one normal of variance `mean + n σ_read²` could replace the two.
   - One Box–Muller transform gives two normals, so half the draws could be saved.
   - A faster normal generator, such as a ziggurat or a table, would cut more.
   - Each draw must stay seeded by frame and row, and the sensor's tests must still hold.
6. **Fuse the CPU preview's first steps:** from the 16-bit mosaic to 8-bit RGB at 1368 × 912 in one pass. That would skip 80 + 60 + 15 MB of float images per frame, which matter most when 20 other threads share the memory. Steps 2–7 are 18.3 of the preview's 29 ms alone.
7. **For the GPU preview:**
   - **A pinned frame pool:** the 40 MB upload takes 4.5 ms alone but 20–28 ms in the live view. From pageable memory the driver copies through a staging buffer on the CPU, where pinned memory would be a DMA.
   - **The resize on the GPU,** leaving only the JPEG on the CPU.
   - Once the camera renders on the GPU, the frame is already there. Using it would need the frame bus to carry device frames: a design change.
8. **Smaller items:**
   - the packing, 5–7 ms on the camera's own thread, could be written by the exposure straight into the slab;
   - `Mounting::to_stage` takes a sine and cosine for every pixel, 3.6 ns × 20 M;
   - the 50 ms position wait is never reached.
9. **The wire and the page,** about 10 ms in all, need nothing yet.

## 6. Caveats

- **One laptop, at an uncontrolled temperature.** Cold frames of the same tiles took 4.2–6.7 s from one run to another. Still frames repeat within 5%; jogging and cold frames, within 10–20%.
- **The browser shares the laptop** with the server, as a browser on the instrument's PC would.
- **Headless Chrome:** its compositor may pace frames differently from a visible window, which affects `page.next_frame`. Decoding and drawing should be the same.
- **Transit and end to end** depend on the clock offset, good to about a millisecond.
- **Timing on:** the GPU's steps wait for the GPU, a few µs each, and the CPU's cost one `Instant` each.
- **The jogging windows** have 12–13 frames each.
- **The models benchmark** runs on a stepped clock with nothing beside it, so its frames don't contend with a preview. Its jogging frames are one interval of travel apart, so they need fewer new tiles than the live view's.
