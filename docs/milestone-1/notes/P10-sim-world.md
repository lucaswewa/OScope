# P10: Simulated world, optics, H&E tissue

- Status: In review
- Branch / PR: `m1/p10-sim-world` / [#22](https://github.com/lucaswewa/OScope/pull/22)
- Size: L · estimated ~2,200 before it started, from P09's measured densities; the owner chose one pull request · measured **1,961** hand-written lines added (code 1,014; tests 519; examples 421; glue 7), and 2 removed. Docs and `Cargo.lock` aren't counted. **That is 1,061 over L's 900, and 239 under the estimate.**

## Summary

`oscope-sim` now renders what the camera sees, for any stage position, focus and objective ([spec](../phases.md#p10), [ADR-0013](../../adr/0013-the-simulator-clock-camera-mounting-and-3d-samples.md)).

- **The world:**
  - a 75 × 25 mm slide with a coverslip, placed on the stage;
  - in transmitted or reflected light;
  - with the glass's surfaces and edges.
- **Samples are 3D,** made of layers that absorb (optical densities) or reflect, rendered at any pixel size. Each pixel is close to its square's mean, so coarse renderings agree with fine ones averaged down.
- **The optics:**
  - the pixel at the sample;
  - a Gaussian defocus PSF from |dz| and the NA;
  - the depth of field.
- **Image formation:** each layer is blurred by its own defocus. Transmittances multiply (Beer–Lambert), and reflections pass twice through the stain above them.
- **The H&E tissue:**
  - islands with rough edges, holes, and a thickness that varies;
  - nuclei on a jittered grid, in two half-layers of the section;
  - eosin in fibrous stroma and in the cytoplasm around nuclei.

  It is deterministic from a seed, and reports its nuclei as ground truth for P46.
- **A test target** with exact answers, for the renderer's tests.
- **`ex-sim-render`** renders the slide and fields through each objective at five focus positions, with their times, on a page.
- **ADR-0013** also designs the simulation clock (P11) and the camera's mounting (P12).

## What was built

| Where | What |
|---|---|
| `oscope_sim::noise` | **`mix`** and **`hash`** (SplitMix64's finaliser over seed, salt and cell), **`unit`**, **`nth`**; **`value`** noise with Perlin's quintic fade; **`fractal`** noise with the level-of-detail fade; **`coverage`**, a box filter across an edge; **`stripes`**, the exact mean of a periodic pattern over an interval |
| `oscope_sim::optics` | **`Optics`** (NA, magnification, sensor pixel, wavelength, medium): `new` from a catalogue objective, `pixel_size`, `blur(dz)`, `depth_of_field` |
| `oscope_sim::sample` | **`LayerKind`** (absorbing, reflective), **`Layer`** (name, height, thickness, kind), the **`Sample`** trait (`name`, `layers`, `bounds`, `render`), and **`render_pixels`**, a parallel pixel loop for samples |
| `oscope_sim::target` | **`TestTarget`:** absorbing squares 50 µm on a 100 µm pitch at the surface, and reflective bars 20 µm on a 40 µm pitch, 20 µm up, each pixel an exact mean |
| `oscope_sim::tissue` | **`HeTissue`:** `new(seed, bounds)`, `islands`, `nuclei(region)`; **`Nucleus`** (centre, semi-axes, angle, layer, hematoxylin) |
| `oscope_sim::world` | **`Slide`** (`SIZE`, `COVERSLIP`, `new`), **`Field`** (centre, z, size), **`Light`**, and **`World`** (`empty`, `with_slide`, `render`), with a separable Gaussian blur |
| `examples/ex-sim-render` | The overview and 20 fields, their times and sharpness, on a page; `--seed`, `--width`, `--smoke` |
| Docs | ADR-0013 and its index row. P09b marked *Done*. The change log records P10's estimate and the owner's choice |

**The numbers, and where they come from:**

| Quantity | Value | Source |
|---|---|---|
| In-focus blur | σ = 0.21 λ / NA | Zhang, Zerubia and Olivo-Marin (2007) |
| Defocus | a disc of radius \|dz\| tan θ, sin θ = NA / n; its σ is half the radius | geometric optics |
| Depth of field | λ n / NA² + n e / (M NA) | Inoué and Spring, after Berek |
| Stains | hematoxylin (0.650, 0.704, 0.286), eosin (0.072, 0.990, 0.105) in OD | Ruifrok and Johnston (2001) |
| Absorption | transmittance 10^−OD | Beer–Lambert |
| Glass | 96% passed, 4% reflected, per surface, for n = 1.5 | Fresnel, in Hecht's *Optics* |
| Hashing | SplitMix64's finaliser | Steele, Lea and Flood (2014) |
| Noise | value noise with the quintic fade; fractal sums | Perlin (2002); Ebert et al., *Texturing and Modeling* |
| Nuclei | one per cell of a jittered 11 µm grid | jittered sampling, Cook (1986) |
| Slide | 75 × 25 mm, a 24 × 50 mm coverslip | the common sizes |

## How to try it

```sh
cargo run --release -p ex-sim-render
cargo test -p oscope-sim
```

The example's [README](../../../examples/ex-sim-render/README.md) shows what it prints and what the page shows.

## Performance

**Rendering time,** in release, on the owner's PC (i9-12900, 24 threads), from `ex-sim-render`:

| What | Size | Time |
|---|---|---|
| The whole slide at 25 µm | 3000 × 1000 | 202 ms |
| A field at 5×–20× | 768 × 512 | 62–75 ms |
| A field at 50× | 768 × 512 | 75–96 ms |

Defocused fields take a little longer, for their wider blur. A full 20 MP frame would take a few seconds at this rate. §5.20's budget for the simulated camera at full resolution (≥ 14.5 fps with CUDA, ≥ 5 fps on the CPU) is P15's, with cached pre-blurred levels of detail per z bucket.

**In a debug build,** the crate's 15 tests take about 2 s, and `ex-sim-render --smoke` about 7 s.

## Hardware

None was used.

## Design notes and deviations from the plan

- **One pull request, though it is over twice L,** at the owner's choice.
- **The images were looked at, and that found a bug.** At first the stroma's fibres were turned by an angle that varied across the slide, applied to the slide's coordinates. About 45 mm from the slide's corner, a small change of angle moves the pattern a long way, so 5× fields showed concentric rings like fingerprints. The fibres now run along one direction for each seed, bent by a gentle domain warp. That also cut the levels-of-detail error from 5–6% to about 2%, before the fade was changed (below). Breaking the code on purpose confirms the test now catches the old way.
- **The level-of-detail fade was chosen by measurement, not by the usual rule.** Procedural texturing often fades an octave out between four and two pixels a wavelength. Here a 4× coarser rendering was compared with a fine one averaged down, over seeds 5 and 11:

  | Octave fade | 4× | 16× | 64× |
  |---|---|---|---|
  | From 4 to 2 pixels | 2.0% | — | — |
  | From 2 to 1 pixels | 0.9–1.1% | 3.6–4.1% | 18–20% |
  | sin(π f) / (π f), the box filter's response | 1.0–1.2% | 3.6–4.3% | — |
  | None | 0.6–0.7% | 2.8–3.2% | 16–18% |
  | **From 1 to ½ pixel (chosen)** | **0.6–0.7%** | **2.5–2.9%** | **16–18%** |

  Value noise is smooth within each cell of its grid, so wherever its wavelength is longer than a pixel, the value at the pixel's centre is already close to the pixel's mean. Fading such octaves only takes away detail the mean still has. The chosen fade removes only octaves under half a pixel, which would otherwise add speckle at coarse scales. Its effect is within the metric's noise against no fade at all, so no test can fail on it. The fade it replaced, from two pixels, is caught.

  At very coarse scales, nuclei smaller than a pixel are hit or missed by its centre: 16–18% off at 64× (ADR-0013's costs).
- **Three of the tests were wrong at first,** and were corrected:
  - a tolerance of 0 with a strict comparison;
  - a ±dz symmetry measured from the slide, not from the squares' layer 0.05 µm above it;
  - a reflected contrast expected to halve 20 µm out of focus. At 20× the bars blur by σ ≈ 4.6 µm, which keeps about 80% of a 40 µm pattern's contrast. The test now checks the computed values: 0.64 and 0.04 in focus, about 0.62 and 0.06 out of it.
- **The smoke check's focus rule was wrong at first.** It wanted the middle position sharpest. At 50× the sharpest is a half-layer from the middle, where the centred nucleus lies. The check now wants the sharpest within half a depth of field of the middle, and the ends softer.
- **Two tests were added,** after the first mutation run showed them missing: a bar passes what it doesn't reflect, and a reflection passes twice through the stain above it.
- **The tissue's section is two half-layers,** which the spec's "layers with z position" allows. That is what makes focusing through a section show different nuclei at 50×.
- **The camera's mounting, motion blur and the sensor** are P12's. P10's fields are aligned with the stage, still, and noiseless.
- **No new dependencies.** `rayon` was already in the workspace.

## Tests

**15 tests in `oscope-sim`,** all passing:

| File | Tests |
|---|---|
| `tests/optics.rs` | In focus, the blur is diffraction's. It grows with \|dz\|, the same either side, towards \|dz\| tan θ / 2. The pixel sizes and depths of field of Mitutoyo's four objectives |
| `tests/render.rs` | **The test target's exact answers:** <ul><li>a square passes 10^−0.5 of the glass's 0.8493;</li><li>the glass: 1 off the slide, 0.9216 on it, 0.8493 under the coverslip, darker at the coverslip's edge, and an empty world;</li><li>reflected light: 0.64 on a bar and 0.04 between bars in focus, both softened out of focus, and a bar passing 40% in transmitted light;</li><li>a reflection under a stain: 0.04 + 0.5 × 10^−0.6.</li></ul>**The blur:** an edge's 10–90% width grows with \|dz\|, the same at ±dz, and matches 2.563 σ within 5%. **Scale:** a 50 µm square measures 50 µm, ±1 pixel, at 5×, 10× and 20×. **Levels of detail:** the target's coarse rendering equals the fine one averaged down, to 10⁻⁵ |
| `tests/tissue.rs` | The same seed gives the same image, and another seed a different one. A field's half equals the field of its half. A 4× coarser rendering is within 1.5% of the fine one averaged down (0.5–0.7% over five seeds). Nuclei lie where the ground truth says, as dark spots, at 2,000–9,000 a mm², in both layers. The tissue lies in 3–5 islands, covering 5–60% of the coverslip |

**Breaking the code on purpose.** 15 changes, each made alone, and each caught:

| Change | Caught by |
|---|---|
| Diffraction twice as wide | `in_focus_the_blur_is_diffraction_s`, and the blur's growth |
| The defocus disc's radius taken as its spread | `the_blur_grows_with_distance_from_focus_on_either_side` |
| The depth of field without the sensor's term | `pixels_and_depths_of_field` |
| Optical density taken in base e | the square, the reflection under a stain, scale, and blur tests |
| Absorbing layers never blurred | `the_blur_grows_with_distance_from_focus` |
| Reflections not dimmed by the stain above | `a_reflection_passes_twice_through_the_stain_above_it` |
| A mirror that passes everything | the reflection tests |
| One surface for each piece of glass | six tests, the glass among them |
| No margin for the blur | `a_field_s_half_is_the_field_of_its_half` |
| Pixels placed without the field's origin | the square, scale, reflection and blur tests |
| The first fade, gone at two pixels a wavelength | `a_coarse_rendering_is_close_to_a_fine_one_averaged_down` |
| Hard, aliased edges | the same |
| Fibres turned with the slide's coordinates (the old bug) | the same |
| Nuclei reported 3 µm off their centres | `each_nucleus_is_a_dark_spot_where_the_ground_truth_says` |
| Nuclei stained without their hematoxylin | the same |

**The first mutation run** missed two of these: the fade switched off, and hard edges. Measuring why showed the fade itself was poorly chosen (the table above). After the change, hard edges are caught. The fade switched off is replaced in the list by the old fade, which is a real defect; no fade at all measures within the metric's noise.

**The workspace,** with every step CI runs: `cargo fmt`, `clippy -D warnings`, `cargo test` (145 tests), `cargo doc -D warnings`, `cargo deny`, the server's `--version`, and every example's `--smoke`.

- **On Windows:** all 8 steps passed; 77 test suites, 145 tests.
- **Under WSL (Ubuntu 24.04.5):** the same, on a fresh LF clone of the commit, with no file holding a CR.

P10 touches no CUDA path.

## Follow-ups and known gaps

- **P11:** the simulation clock and the stage, as ADR-0013 designs them.
- **P12:** the camera:
  - its mounting, motion blur and rolling shutter;
  - the sensor's Bayer mosaic, noise and quantisation;
  - the modes' measured rates.
- **P13–P14:** more samples (a reflective surface, a cell volume, sub-diffraction structures, imports) as further `Sample`s.
- **P15:** full resolution at the frame rate: cached pre-blurred levels of detail per z bucket, and CUDA.
- **P46:** nucleus detection, checked against `HeTissue::nuclei`.
- **The H&E look** is plausible, not calibrated. Its stain amounts could be tuned against real slides' colour statistics, never committing Tucsen's images.

## Clean room

Written for this phase from the published methods in the table under *What was built*:

- SplitMix64's finaliser (Steele, Lea and Flood, *Fast splittable pseudorandom number generators*, OOPSLA 2014);
- Perlin's quintic fade (*Improving noise*, SIGGRAPH 2002);
- fractal sums of noise (Ebert, Musgrave, Peachey, Perlin and Worley, *Texturing and Modeling: A Procedural Approach*);
- jittered sampling (Cook, *Stochastic sampling in computer graphics*, ACM TOG 1986);
- Gaussian PSF approximations (Zhang, Zerubia and Olivo-Marin, Applied Optics 46(10), 2007);
- the depth of field (Inoué and Spring, *Video Microscopy*, 2nd ed., 1997);
- H&E's stain vectors (Ruifrok and Johnston, Analytical and Quantitative Cytology and Histology 23(4), 2001);
- Fresnel's reflectance (Hecht, *Optics*).

Nothing was copied from `microscope` or any other codebase, and no Tucsen image was used.
