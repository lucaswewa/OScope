# P14: Samples III: sub-diffraction structures, imports, load and unload

- Status: In review
- Branch / PR: `m1/p14-sim-samples-imports` / [#27](https://github.com/lucaswewa/OScope/pull/27)
- Size: M · estimated ~1,700 before it started (code ~900, tests ~550, `ex-sim-samples` ~250); one pull request, at the owner's standing choice for P12–P23 · measured **2,246** hand-written lines added (code 1,058; tests 861; examples 327), and 2 removed. Docs and `Cargo.lock` aren't counted. **That is 1,646 over M's 600, and about 550 over the estimate,** most of it in the tests.

## Summary

The simulator has its super-resolution sample, takes your own images and stacks as samples, and can load and unload slides from a catalogue its configuration lists ([spec](../phases.md#p14); D23, D24).

- **Sub-diffraction structures:** filaments 25 nm across, and hollow tubules like mitochondria. Their fine-scale truth shows each filament's width and each tubule's hollow; through the 50× they blur exactly as the optics' transfer function predicts.
- **Imports:** a PNG, JPEG or TIFF image, or a multi-page TIFF stack, at its µm per pixel and z step, as brightfield, reflected light or fluorescence. They land where their corner and scale put them, to 0.05 µm.
- **A slide loader:** load a sample in place of any other, unload it, leaving the stage empty: the camera then sees only the light and the sensor's noise.
- **A sample catalogue:** the five built-in samples, each with ground truth, and the images and stacks a configuration adds, without.
- **`oscope-imaging` reads image files,** beside the PNG and TIFF it already writes.
- **`ex-sim-samples`** lists, loads, renders and unloads every sample, and imports an image and a stack through a configuration, or your own image.

## What was built

| Where | What |
|---|---|
| `oscope_sim::structures` | **`Structures`:** filaments and tubules in a thin layer, in two dyes. `filaments(region)`, `tubules(region)` (the `Filament`s and `Tubule`s), `truth(origin, pixel, width, height)` (the exact amounts, at any pixel) |
| `oscope_sim::import` | **`Imported`:** an image or stack as a sample: `new` (from pages), `read` (from a file), `centred_on`. **`ImportLight`:** transmitted, reflected, or fluorescent in a dye |
| `oscope_sim::catalogue` | **`SampleCatalogue`:** `builtin`, `from_toml` (`[[samples]]`), `get`. **`SampleEntry`:** `has_ground_truth`, `build`. **`SampleKind`**, **`Imaging`**. **`SlideLoader`:** `load`, `unload`, `loaded` |
| `oscope_sim::fluorescence` | `Dye` reads from a configuration: `dapi`, `green`, `orange`, `far-red` |
| `oscope_imaging::color` | **`read_image`:** PNG, JPEG and TIFF (uncompressed, LZW, deflate, JPEG), every page, as linear RGB |
| `examples/ex-sim-samples` | The catalogue, every sample loaded and rendered at 5× and 50×, two imports through a configuration, the empty stage; `--image`, `--um-per-pixel`, `--smoke` |
| Dependencies | `zune-jpeg` 0.5.15 and `zune-core` 0.5.3 (JPEG); `tiff`'s `lzw`, `deflate` and `jpeg` features, which add `weezl` 0.1.12; `jpeg-encoder` 0.6.0, for a test only |
| Docs | P13 marked *Done*; P14's estimate in the change log |

## How to try it

```sh
cargo run --release -p ex-sim-samples
cargo run --release -p ex-sim-samples -- --image path/to/your.png --um-per-pixel 0.25
cargo test -p oscope-sim -p oscope-imaging
```

The example's [README](../../../examples/ex-sim-samples/README.md) shows what it prints and what its images show.

## Performance

**In release, on the owner's PC,** from `ex-sim-samples`: two 768 × 512 fields of each sample, at 5× and 50×:

| Sample | Both fields |
|---|---|
| H&E tissue | 216 ms |
| Test target | 138 ms |
| Reflective surface | 443 ms |
| Fluorescent cells (three lines each) | 945 ms |
| Sub-diffraction structures (two lines each) | 170 ms |
| Your image | 56 ms |
| The stack | 67 ms |

## Design notes and deviations from the plan

- **"A sample catalogue in the configuration":** the server keeps its `[devices]` table as it is until P16's device Things read it. P14 defines what the simulator's part says, `[[devices.simulator.samples]]` entries, and parses it (`SampleCatalogue::from_toml`); P16 hands it that table. Until then the example writes and reads its own configuration.
- **What a configuration can say:**
  - each entry's name, kind, seed, file, scale, z step, light and dye;
  - unknown keys and kinds are refused;
  - an entry with a built-in's name replaces it;
  - an import's file is relative to the configuration's folder.

  **Ground truth** is recorded by kind: every procedural sample has it, an import doesn't.
- **Imports:**
  - **Placement:** centred on the coverslip, each page a layer, a stack's pages a z step apart from the slide's surface.
  - **What the values mean:**
    - brightfield values are transmittances, rendered as optical densities, black clamped to 4;
    - reflected values are reflectances;
    - fluorescence values are amounts of dye, full brightness standing for 0.2.
  - **Rendering:** each pixel is the exact mean of the image pixels under it, so a coarse field is a fine one averaged down (ADR-0013). At high magnification an import's pixels show as blocks: nothing is interpolated.
- **Reading files:**
  - The decoders sit in `oscope-imaging`, beside its encoders; `oscope-sim`, a layer above, uses them (ADR-0005).
  - **PNG and JPEG are taken as sRGB,** the web's encoding, and decoded to linear light. **TIFF is taken as linear data,** as scientific stacks are.
- **The loader:**
  - it builds a sample before touching the stage, so a sample that can't be built (a missing file) leaves the stage as it was;
  - the focus belongs to the world, not the slide, so loading doesn't move it.
- **The structures** lie in one thin layer on the glass, as a cell imaged by super-resolution microscopy is thin.
  - **Filaments** are persistent random walks: 0.5 µm steps, each turning a little, a persistence length of about 45 µm.
  - **Tubules** are hollow rods with round ends, 0.24–0.32 µm across, with a 30 nm wall.
- **Their level of detail:** at pixels wider than a structure, its body becomes a tent a pixel either side, and each of its ends a cone a pixel in radius, each holding the dye of the part it stands for.
  - At first the tent simply surrounded each structure, ends included. That gave every structure round ends a pixel across: 3% too much dye for filaments at 1 µm pixels, and 28% for short tubules.
  - Since the fix, a 1 µm rendering holds within 1.5% of a 5 nm one's dye.
- **`jpeg-encoder`'s licence, found while choosing a test encoder:**
  - from 0.6.1, its licence is "(MIT OR Apache-2.0) AND IJG", and IJG isn't in `deny.toml`'s list, which only the owner extends;
  - 0.6.0 is MIT OR Apache-2.0. P14 pins it, `=0.6.0`, for its JPEG test;
  - **P17, which the owner chose `jpeg-encoder` for, will pin 0.6.0 too,** unless the owner allows IJG.
- **What building it found:**
  - **The end-cap bias,** above.
  - **The MTF measure's own bias:** on a 128-pixel field, the Hann window's spectrum, 0.33 cycles a µm wide, mixed lower frequencies in. The measured transfer came out 10–60% high: 0.905 against 0.824 at 0.5 cycles a µm, 0.073 against 0.045 at 2. The renderer itself matched a blur done in the test to 0.0004. On 256 pixels the measure comes within 0.009 of the prediction.
  - **Two mistakes in my tests and example,** both corrected:
    - a helper took its edge levels from the wrong columns;
    - the example listed its own image by a path that the catalogue then joined to its folder a second time.

## Tests

**88 tests in `oscope-sim`, 12 of them new, and 5 new in `oscope-imaging`,** all passing:

| File | Tests |
|---|---|
| `oscope-sim/tests/structures.rs` (4) | **Sub-diffraction detail is blurred as the optics predict:** in focus at 50×, the image's spectrum over the truth's, in rings at 0.5, 1, 1.5 and 2 cycles a µm, against exp(−2π² σ² f²) for σ = 0.21 λ / NA: within 0.015. The truth shows what the optics can't: a filament over 25 nm only, a tubule's hollow; the 50× shows the tubule as one blur. A coarse rendering holds as much dye as a fine one. The same seed gives the same structures |
| `oscope-sim/tests/import.rs` (5) | **An image lands at its corner and scale:** a rectangle's edges within 0.05 µm. A coarse field is a fine one averaged down. An import's values mean what its light says. A stack's pages are its z step apart: focused on each page's depth, its square is the sharpest. Bad imports are refused |
| `oscope-sim/tests/loader.rs` (3) | The catalogue lists its samples and their ground truth, and refuses what a configuration can't say. Loading puts a sample on the stage, a failed load leaves it, unloading empties it. **Unloading leaves only background and noise:** the camera's frame then matches the sensor model's mean and variance for unhindered light |
| `oscope-imaging/tests/read.rs` (5) | An 8-bit PNG is read as linear light; a 16-bit grey PNG fills all three channels; every page of a TIFF stack, uncompressed, LZW and deflate; a JPEG within its compression's error; other files are refused, with their names |

**Breaking the code on purpose:** 15 changes, each made alone.

- 14 were caught the first time.
- **One was missed:** a failed load that emptied the stage. The loader test checked what the loader said was loaded, not what was on the stage. It now checks both, and catches it.

| Change | Caught by |
|---|---|
| A widened structure without its cross-section's area | the coarse-rendering and truth tests |
| No round ends when widened | `a_coarse_rendering_holds_as_much_dye_as_a_fine_one` |
| Widened bodies running past their ends | `a_coarse_rendering_holds_as_much_dye_as_a_fine_one` |
| Tubules not hollow | the coarse-rendering and truth tests |
| Filaments twice as wide | `the_truth_shows_what_the_optics_cannot` |
| Imports averaged with equal weights | the averaging and meaning tests |
| An import's corner ignored | the landing, meaning and stack tests |
| A brightfield import's values taken as densities | `an_import_s_values_mean_what_its_light_says` |
| A stack's pages all at the slide | `a_stack_s_pages_are_its_z_step_apart` |
| 8-bit PNGs not decoded from sRGB | `an_8_bit_png_is_read_as_linear_light` |
| 16-bit TIFFs taken as sRGB | `every_page_of_a_tiff_stack_is_read_as_linear_data` |
| Unloading leaving the slide | both loader tests |
| A failed load emptying the stage | `loading_puts_a_sample_on_the_stage_and_unloading_takes_it_off` (once strengthened) |
| Imports said to have ground truth | `the_catalogue_lists_its_samples_and_their_truth` |
| A configured sample not replacing a built-in | `the_catalogue_lists_its_samples_and_their_truth` |

**The new crates:** `cargo deny` passes, with no new duplicate versions. Each crate is MIT, Apache-2.0 or Zlib, released at least two weeks before:

- `zune-jpeg` 0.5.15, March 2026;
- `zune-core` 0.5.3, August 2026;
- `weezl` 0.1.12, November 2025;
- `jpeg-encoder` 0.6.0, October 2023.

**The workspace,** with every step CI runs: REPLAYS

P14 touches no CUDA path.

## Follow-ups and known gaps

- **P16:**
  - the `simulation` Thing: the catalogue, choosing, loading, unloading and importing samples;
  - the server's `[devices.simulator]` handed to `SampleCatalogue`;
  - uploads of images and stacks (`POST /api/v1/upload/…`).
- **P17:** `jpeg-encoder` pinned at 0.6.0 for its licence, unless the owner allows IJG.
- **P15:** a pyramid for large imports. Each pixel is averaged from the image's own, so a field over a large import at a coarse scale costs as many image pixels as it covers.
- **P41 and P43:** deconvolution against `Structures::truth`.
- **Imports are taken as they are:** sRGB for PNG and JPEG, linear for TIFF. There is no setting for a linear PNG or a TIFF that isn't, nor for colour profiles.

## Clean room

Written for this phase from:

- plan §5.10 and ADR-0013;
- **sRGB's decoding:** IEC 61966-2-1;
- **the filaments' paths:** the worm-like chain's persistent random walk (Kratky and Porod, 1949);
- **the structures' sizes:** microtubules' 25 nm, and mitochondria's 0.2–0.5 µm width, from cell-biology textbooks;
- **the MTF check:** a Gaussian's transfer function, exp(−2π² σ² f²); the Hann window; the discrete Fourier transform;
- the formats' own specifications, through the `png`, `tiff` and `zune-jpeg` crates.

Nothing was copied from `microscope` or any other codebase.
