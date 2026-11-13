# P13: Samples II: reflective surface, fluorescent cell volume

- Status: In review
- Branch / PR: `m1/p13-sim-3d-samples` / [#26](https://github.com/lucaswewa/OScope/pull/26)
- Size: L · estimated ~1,900 before it started (code ~1,000, tests ~500, `ex-sim-3d` ~450); one pull request, at the owner's standing choice for P12–P23 · measured **2,511** hand-written lines added (code 1,074; tests 815; examples 622), and 71 removed. Docs and `Cargo.lock` aren't counted. **That is 1,611 over L's 900, and about 600 over the estimate:** the tests and the example's depth from focus and page took more than estimated.

## Summary

The simulator has two 3D samples whose truth is known, and fluorescence ([spec](../phases.md#p13), ADR-0013 §2 and §5).

- **A reflective surface:** a silicon chip of 200 µm dies, each with four quadrants: a stepped pyramid, a dome, a ridge, and an aluminium film with etched pits. It is slightly tilted, with a fine waviness and a grain. Its height map is the ground truth for depth from focus: at 50×, depth from focus finds it within 0.03 µm RMS.
- **Fluorescence, in the simulator only:**
  - four excitation lines, and four dyes, each with its colour on the colour sensor;
  - widefield, which keeps all of a plane's light at any focus;
  - a confocal, which passes a share that halves at half its optical section: 0.85 µm through an oil objective of NA 1.4, 4 µm through the instrument's 50×.
- **A culture of cells** in three dyes: nuclei, membranes and organelles. Its true volume, each dye's density anywhere, is the ground truth, and matches what it renders to 1%.
- **A faster blur:** large blurs are four box filters in turn, so their cost no longer grows with the blur. A stack of 39 layers far from focus renders in a fraction of a second.
- **`ex-sim-3d`** steps through both samples in z on a page, beside their truth.

## What was built

| Where | What |
|---|---|
| `oscope_sim::surface` | **`Surface`:** the chip, as reflective slabs 0.5 µm apart over an opaque substrate. `height` (the truth at a point), `height_map` (each pixel's mean height), `die_centre` |
| `oscope_sim::cells` | **`CellCulture`:** cells on a jittered grid, as 13 slices 0.5 µm thick, a layer for each dye in each. `cells(region)` (the `Cell`s, with their `Ellipsoid` nuclei and `Organelle`s), `truth(origin, pixel, width, height, z)` (a plane of the true volume). **`DYES`, `DENSITY`** |
| `oscope_sim::fluorescence` | **`Excitation`** (405, 488, 561, 640 nm). **`Dye`** (DAPI-like, green, orange, far red): `peaks`, `excited_by`, `colour`. **`sensor_response`**, **`optical_section`**, **`sectioning`** |
| `oscope_sim::blur` | **`gaussian`**, of any number of channels: exact below 8 pixels, four box filters from there. **`box_radii`** |
| `oscope_sim::world` | **`Light::Fluorescence { line, confocal }`.** Each light renders only its own layers. Reflections are dimmed by the stains above them only. Empty layers are skipped |
| `oscope_sim::sample` | **`LayerKind::Fluorescent(Dye)`:** a layer that renders its dye's amount |
| `examples/ex-sim-3d` | Both samples' z-series, the surface's height map and depth from focus, the cells' widefield, confocal and truth, on a page with sliders. The truths as raw files. `--seed`, `--smoke` |
| Docs | P12 marked *Done*; P13's estimate in the change log |

## How to try it

```sh
cargo run --release -p ex-sim-3d
cargo test -p oscope-sim
```

Open the address it prints. The example's [README](../../../examples/ex-sim-3d/README.md) shows what it prints and what the page shows.

## Performance

**In release, on the owner's PC,** from `ex-sim-3d`:

| Rendering | Time |
|---|---|
| The surface at 20×, 768 × 512, in reflected light | 225 ms a plane |
| The cells through a 60× oil objective, 640 × 480, one line | about 260 ms an image |

**The surface renders only the slabs near each pixel's height:** a pixel's height without its waviness rules out the others before any noise is computed.

**The cells' cost is their 39 layers,** each blurred by its own distance from focus. At 60× with NA 1.4, a slice 6 µm from focus is blurred by σ = 100 pixels, and the field grows by 3σ on each side. The box filters keep that affordable. P15 caches levels of detail.

## Hardware

None. The instrument has no fluorescence (plan, risk 13).

## Design notes and deviations from the plan

- **The surface is slabs of one height map.** ADR-0013 renders thin layers, each blurred by its own distance from focus. A surface whose height varies is cut into reflective slabs 0.5 µm apart. Each pixel's reflectance goes to the two slabs either side of its height, in proportion to how near each is, so its blur follows its height between slabs.
  - A blur that varies pixel by pixel would be exact, and much slower.
  - Measured, the slabs are close enough: on a flat terrace, depth from focus finds the height within 0.004 µm, though the slabs are 0.5 µm apart.
- **Reflections are dimmed only by the stains above them, as ADR-0013 says.** P10 also dimmed them by reflective layers above. A surface's slabs would then dim each other where a pixel's light is shared between two: aluminium halfway between slabs would reflect 35% less. No earlier test changed.
- **A fluorescent layer holds one dye,** and renders its amount: micrometres of dye at unit density along the light's path.
  - The world blurs the amount at the dye's own emission wavelength, then colours it by how well the line excites the dye and what the sensor records of its light.
  - Each light renders only its own layers: dyes in fluorescence; stains and reflections otherwise. A dye the line excites less than a millionth is left out.
- **The models of fluorescence:**
  - **Excitation:** a Gaussian of 25 nm around each dye's published excitation peak. Real spectra fall more slowly on their blue side.
  - **The sensor:** Bayer channels as Gaussians peaking at 610, 535 and 460 nm, with red cut off from 650 nm by the infrared filter. So the far-red dye is dim on the colour camera.
  - **The confocal:**
    - its optical section is the wave-optical width 0.88 λ / (n − √(n² − NA²)), in quadrature with a one-Airy-unit pinhole's geometric width √2 n PH / NA;
    - a plane dz from focus passes 1 / (1 + (2 dz / section)²). Its tails fall as 1 / dz², as a pinhole's share of a wide blur does;
    - its lateral blur is widefield's. At one Airy unit the two are close.
- **The cells' level of detail:** at pixels larger than they are, the membrane is widened to a pixel and each organelle to a pixel across, with its density lowered to keep its amount of dye.
  - The membrane's factor is the exact ratio of the two skins' volumes. Scaling by thickness alone lost 12% at 1 µm pixels.
  - Organelles under a pixel fall on pixel centres by chance, so they are kept on average. Over 96 µm at 1 µm pixels, against 0.1 µm:
    - nuclei and membranes come within 0.6%;
    - organelles within 1%, and within 4% at 2 µm.
- **The cells in the example are seen through a simulated 60× oil objective of NA 1.4,** which the instrument doesn't have. Through its dry 50× (NA 0.55) the confocal's section is 4 µm, as thick as the cells, and shows little. Fluorescence is the simulator's only, so the example shows it as confocal microscopes are used. The tests use the instrument's 50×.
- **"Its ground-truth volume exportable":** `CellCulture::truth` gives any plane of it on any grid. The example writes 13 planes as a raw file, with its shape in `results.json`. P41's volume store will hold volumes properly.
- **What building it found:**
  - **The grain must have fine detail.** At first the surface's grain was mostly 8–16 µm across, longer than a 2 µm tile. Depth from focus missed by 1–3.5 µm RMS, even on flat terraces. With most of the grain at 0.75–1.5 µm, it came within 0.03 µm.
  - **A focus measure of gradients is misled by bright edges.** Beside the aluminium film, a dim pit's tiles were sharpest far from focus, where the film's blurred edge crossed them. The modified Laplacian (Nayar and Nakagawa, 1994) responds to fine detail rather than blurred edges. The tests and the example use it; P34 and P42 choose their own.
  - **The membrane's density factor,** above.
  - **Three wrong expectations in my tests,** each corrected:
    - the confocal's share at 1.5 sections is exactly a tenth;
    - four boxes' 2D peak is 6% off a Gaussian's, compounded over two axes, so the test compares the 1D profile;
    - a fifth of the tries for organelles land in the cytoplasm, so the tries were doubled to 120.

## Tests

**76 tests in `oscope-sim`,** 18 of them new, all passing:

| File | Tests |
|---|---|
| `tests/surface.rs` (4) | **The sharpest z matches the height map:** depth from focus over 1 µm tiles, against the truth's mean in each. Measured at 50×: dome 0.024 µm RMS (worst 0.053); ridge 0.021 (0.062); terrace 0.004 (0.006); film 0.011 (0.016). At 20×: dome 0.028 (0.059); ridge 0.019 (0.043). The limits are 0.05 RMS and 0.15 worst. The height map's features, each across its edge, and the tilt. Metal shines, silicon less, the chip is opaque. The same seed gives the same surface |
| `tests/fluorescence.rs` (6) | Each dye's colour under its line, exactly; cross-excitation under 0.1%. Each dye blurs at its own wavelength. **Widefield keeps a sheet's light at every distance from focus, and the confocal passes exactly its share.** The optical section through the 50× and through an oil objective. **A confocal cuts the light from out-of-focus cells:** focused 8 µm above them, widefield keeps over 70% of their light, the confocal under 10%. Each light sees only its own layers |
| `tests/cells.rs` (4) | **The truth holds what the slices render:** each dye's amount within 1%. A coarse slice holds as much dye as a fine one. Cells hold their nuclei and organelles, and keep apart. The same seed gives the same culture, and half a field is the field of its half |
| `tests/blur.rs` (4) | The cascade's widths have the Gaussian's variance, within 2%. Both blurs keep the light and spread it as asked. The cascade's profile is within 5% of a Gaussian's peak. An even image stays even |

**Breaking the code on purpose:** 16 changes, each made alone. 14 were caught the first time.

- **One was missed:** blurring a dye at white light's wavelength instead of its own. `each_dye_blurs_at_its_own_wavelength` was added for it.
- **One didn't build** as first written. Rewritten so it built, it was caught.

| Change | Caught by |
|---|---|
| Slabs not interpolated | `the_sharpest_z_matches_the_height_map` |
| Reflections dimmed by the reflective layers above | `the_sharpest_z_matches_the_height_map` |
| No confocal sectioning | the sheet and the cells' confocal tests |
| The pinhole's geometric term dropped | `the_optical_section_follows_the_aperture` |
| Every line exciting every dye | `each_dye_shines_in_its_colour_under_its_line` |
| No infrared-cut filter | `each_dye_shines_in_its_colour_under_its_line` |
| A dye blurred at white light's wavelength | `each_dye_blurs_at_its_own_wavelength` (added) |
| Weakly excited dyes left out | `each_dye_shines_in_its_colour_under_its_line` |
| The cascade's widths for the wrong variance | all three blur tests |
| Organelles let into the nucleus | `cells_hold_their_nuclei_and_organelles_and_keep_apart` |
| The widened membrane's dye not kept | `a_coarse_slice_holds_as_much_dye_as_a_fine_one` |
| The widened organelles' dye not kept | `a_coarse_slice_holds_as_much_dye_as_a_fine_one` |
| The truth's membrane filled in | `the_truth_holds_what_the_slices_render` |
| The chip transparent | `metal_shines_and_the_chip_is_opaque` |
| No tilt along x | `the_height_map_has_its_features` |
| The pits not etched | `the_height_map_has_its_features` |

**The workspace,** with every step CI runs:

- `cargo fmt`;
- `clippy -D warnings`;
- `cargo test`;
- `cargo doc -D warnings`;
- `cargo deny`;
- the server's `--version`;
- every example's `--smoke`, `ex-sim-3d`'s among them.

The results of `scripts/ci-local.sh` on Windows and `scripts/ci-clone.sh` under WSL are below.

P13 touches no CUDA path.

## Follow-ups and known gaps

- **P14:** the sub-diffraction structures, as further fluorescent samples; imported images; loading and unloading slides.
- **P15:** speed, with cached levels of detail and CUDA. The cell volume's 39 layers are its heaviest case.
- **P16:** the illumination Thing's fluorescence lines and confocal switch, through `SimCamera::set_light`, which already renders any `Light`.
- **P34 and P42:** depth from focus, against `Surface::height_map`. **P41 and P43:** volumes and deconvolution, against `CellCulture::truth`.
- **The fluorescence models are simple:** Gaussian spectra, a modelled sensor response, a confocal whose lateral blur is widefield's, no photobleaching and no autofluorescence. They are enough to test sectioning and colour separation, not to match a real instrument.

## Clean room

Written for this phase from:

- plan §5.10 and ADR-0013;
- **the optical section:** Wilhelm, Gröbler, Gluch and Heinz, *Confocal laser scanning microscopy: principles* (Carl Zeiss); Cole, Jinadasa and Brown, *Measuring and interpreting point spread functions to determine confocal microscope resolution and ensure quality control*, Nature Protocols, 2011;
- **the box filters:** Wells, *Efficient synthesis of Gaussian filters by cascaded uniform filters*, IEEE PAMI, 1986; Kovesi, *Fast almost-Gaussian filtering*, DICTA, 2010;
- **the focus measure in the tests and the example:** Nayar and Nakagawa, *Shape from focus*, IEEE PAMI, 1994;
- **reflectances, from Fresnel's equation at normal incidence** (Hecht, *Optics*): silicon from Green's refractive indices (*Solar Energy Materials and Solar Cells*, 2008), aluminium from Rakić's (*Applied Optics*, 1995);
- the dyes' published excitation and emission peaks.

Nothing was copied from `microscope` or any other codebase.
