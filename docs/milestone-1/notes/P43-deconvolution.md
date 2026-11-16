# P43: Deconvolution and volume rendering

- Status: In review
- Branch / PR: `m1/p43-deconvolution` / [#58](https://github.com/lucaswewa/OScope/pull/58)
- Size: L · estimated ~3,000 before it started (in `oscope-volume`, the Born–Wolf widefield PSF with its Bessel function and integral ~150, Richardson–Lucy on rustfft's 3D FFTs with mirrored padding, total-variation regularisation and its iterations ~280; in `oscope-compute`, its CUDA twin on cuFFT with its kernels ~230; the pipeline's nodes ~150; in `oscope-workflows`, the deconvolution job, from a volume acquisition to a result volume, ~200; tests ~750; `ex-deconvolve` and its VTK.js page with MIP, composite, isosurface and a before-and-after toggle ~500; `ex-checkpoint-e` ~200; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice · measured **2,733** hand-written lines added (code 1,081: `oscope-volume` 566, `oscope-workflows` 235, `oscope-compute` 183, `oscope-pipeline` 93, `oscope-api` 4; tests 823; examples 760; glue 69), and 12 removed. **That is 1,833 over L's 900, and 9 % under the estimate.** VTK.js, vendored in P41, isn't counted.

## Summary

A volume deconvolved, by Richardson–Lucy and Born and Wolf's PSF, and volumes drawn in VTK.js ([spec](../phases.md#p43); plan §5.16, D24).

- **The PSF** is Born and Wolf's for a widefield microscope, from the NA, the emission's wavelength, the medium's index and the voxels.
- **Richardson–Lucy** runs by FFT over the volume padded by mirroring, with total variation if asked: on the CPU with rustfft, or on the GPU with cuFFT, within 2 × 10⁻⁶ of the CPU's largest voxel.
- **The pipeline** has the nodes `volume.psf` and `volume.deconvolve`.
- **The job `volume.deconvolve`** takes a volume acquisition to a volume result, in floats. A raw stack is deconvolved on its 2 × 2 cells, and any stack's background is taken off first.
- **`ex-deconvolve`** scans the simulated fluorescent cells in widefield, deconvolves the stack, and draws either with VTK.js: as a projection, composited, or as an isosurface.
- **Checkpoint E, `ex-checkpoint-e`,** serves P42's chip in heights and P43's cells deconvolved from one server, side by side.
- **On the simulator,** the deconvolution comes nearer the cells' truth than the stack, if only a little: an NRMSE of 1.149 against the stack's 1.171. Widefield at NA 0.55 resolves about 3 µm in depth, the membranes are 0.3 µm thick, and the simulator blurs by a Gaussian, not by Born and Wolf's PSF.
- **`ex-depth-from-focus`** now places its truth where the stage was, as the scan's first frame records it.
- **P42 is marked Done.**

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p43); plan §5.16, D24).

### What it does

1. **Its input is a volume** (P41): N planes of W × H voxels of dx × dy × dz µm, as for depth from focus (P42). A raw volume is deconvolved on its 2 × 2 cells' means, at their size.
2. **The point spread function is Born and Wolf's** for a widefield microscope, scalar and free of aberration (Born and Wolf, 1999, §8.8). At lateral distance r and axial distance z from focus, in the optical coordinates v = (2π/λ) NA r and u = (2π/λ)(NA²/n) z, its intensity is

   I(u, v) = |2 ∫₀¹ J₀(vρ) e^(−iuρ²/2) ρ dρ|²,

   which is 1 at focus, (2J₁(v)/v)² in the focal plane (Airy's pattern, its first zero at 0.61 λ/NA) and (sin(u/4)/(u/4))² on the axis.
   - **λ** is the dye's emission, 0.519 µm by default, the simulated green dye's. **NA** is the objective's, and **n** the medium's, 1 for the dry Mitutoyo objectives.
   - **J₀** is from the polynomial approximations of Abramowitz and Stegun (1964, 9.4.1 for x ≤ 3 and 9.4.3 above), within 5 × 10⁻⁸.
   - **The integral** is Simpson's rule over ρ, with intervals enough for the phase and the Bessel function to turn a sixteenth of a cycle at most.
   - **Sampled** at the voxels' centres on a grid about the PSF's centre, of odd size. Axially it spans the volume's depth, so that each plane's light reaches every other. Laterally it spans the widest blur within that depth, the geometric cone's radius plus three Airy radii, at most the volume's width.
   - **Normalised** to sum to 1.
3. **Richardson–Lucy** (Richardson, 1972; Lucy, 1974): from u₀ = d, the data clamped at 0, each iteration is

   uₖ₊₁ = uₖ · (h̃ ⊛ (d / (h ⊛ uₖ))),

   h̃ being the PSF mirrored, ⊛ a convolution by FFT. Where h ⊛ uₖ is under 10⁻¹² of the data's largest value, the ratio is 0.
   - **The FFTs** are rustfft's on the CPU, 3D as 1D transforms along each axis, in 32-bit floats.
   - **Edges:** each axis is padded by mirroring the volume by the PSF's half-size, at most its own length, then to the next size whose prime factors are 2, 3, 5 and 7. The convolution is circular over the padded volume, and the estimate is cropped back.
   - **Energy:** without regularisation, the estimate's sum over the padded volume stays the data's at every iteration, as the PSF sums to 1.
   - **Iterations:** `iterations`, 20 by default.
4. **Total variation, if asked** (Dey et al., 2006): uₖ₊₁ = uₖ · cₖ / (1 − λ_TV div(∇uₖ / |∇uₖ|)), cₖ being the factor above.
   - λ_TV is `tv`, 0 by default, which turns it off; 0.002 is typical.
   - The gradient takes forward differences and the divergence backward ones, with |∇u| = √(|∇u|² + 10⁻¹²).
   - The denominator is held at 0.5 or more, so that a strong curvature can't flip an update's sign.
5. **On CUDA,** the same: cuFFT's 3D complex transforms in 32-bit floats, and kernels for the spectra's products, the ratio, the update and the total variation. The FFTs round differently from rustfft's, so the twin isn't to the bit: its estimate after 10 iterations is within 10⁻³ of the CPU's largest voxel.
6. **The error against a truth,** for the tests and the example: the NRMSE of each volume standardised to zero mean and unit variance over the voxels compared, √(2(1 − ρ)), ρ being Pearson's correlation. It doesn't depend on the volumes' units or offsets.
7. **The pipeline's nodes:**
   - **`volume.psf`:** the PSF as a stack, from `na`, `wavelength_um`, `medium`, the voxel `voxel_um` and the volume's size;
   - **`volume.deconvolve`:** a stack and a PSF → the deconvolved stack. Parameters `iterations` and `tv`; on the CPU, or the GPU through the compute layer.
8. **The job `volume.deconvolve`,** a compute job: {`project`, `acquisition`, `options`}.
   - **Options:** `na`, `wavelength_um`, `medium`, `iterations` and `tv`.
   - **Its result:** a volume of the project, `float32`, of the deconvolved voxels and their voxel size, naming the acquisition and the options in its provenance.
   - **Progress** counts iterations, and it is cancelled between them. It runs on the GPU when there is one.
   - **Refused:** an acquisition that isn't a volume.
9. **Applied** to the simulated fluorescent cells, seen in widefield, and to the sub-diffraction structures.
10. **`ex-deconvolve`** (port 8248):
    - **a widefield z-scan** of the simulated cells at 50×, 21 planes 0.5 µm apart about their focus;
    - **the job,** then the NRMSE of the stack and of its deconvolution against the cells' true membranes, both printed;
    - **the page** draws either with VTK.js, as a maximum intensity projection, composited through a transfer function, or as an isosurface (marching cubes; Lorensen and Cline, 1987), with a toggle between before and after.
11. **Checkpoint E, `ex-checkpoint-e`** (port 8249): P42's heights of the reflective chip as a surface, and P43's deconvolved cells as a volume, both in VTK.js, from one server, on one page.

### What the tests show

- **The PSF:**
  - symmetric in z and about the axis, at its largest at the centre;
  - summing to 1;
  - in focus, Airy's pattern, its first zero at 0.61 λ/NA within a voxel;
  - on the axis, (sin(u/4)/(u/4))² within 10⁻³;
  - J₀ against its tabulated values within 10⁻⁷.
- **Richardson–Lucy,** on a synthetic object blurred by the Born–Wolf PSF, with noise:
  - the NRMSE against the object falls from the data's through iterations 1, 5, 10 and 20;
  - without regularisation, the estimate's sum over the padded volume is the data's within 10⁻⁵ at every iteration;
  - on noisy data, total variation leaves an NRMSE no worse than without it.
- **CPU and CUDA,** with `OSCOPE_CUDA=1`: the estimates within 10⁻³ of the largest voxel after 10 iterations.
- **The nodes** give what the library gives.
- **The job,** over a widefield z-stack of the simulator's cells and of its sub-diffraction structures: its result recorded with its provenance, nearer the truth than the stack by the NRMSE; refusals; a cancel.
- **The example's smoke:** the deconvolution nearer the truth than the stack, by the NRMSE.

### Sources

- Born, M. and Wolf, E. (1999). *Principles of Optics*, 7th edition, §8.8. Cambridge University Press.
- Abramowitz, M. and Stegun, I. A. (1964). *Handbook of Mathematical Functions*, §9.4. National Bureau of Standards.
- Richardson, W. H. (1972). Bayesian-based iterative method of image restoration. *Journal of the Optical Society of America* 62(1), 55–59.
- Lucy, L. B. (1974). An iterative technique for the rectification of observed distributions. *The Astronomical Journal* 79(6), 745–754.
- Dey, N., Blanc-Féraud, L., Zimmer, C., Roux, P., Kam, Z., Olivo-Marin, J.-C. and Zerubia, J. (2006). Richardson–Lucy algorithm with total variation regularization for 3D confocal microscope deconvolution. *Microscopy Research and Technique* 69(4), 260–266.
- Lorensen, W. E. and Cline, H. E. (1987). Marching cubes: a high resolution 3D surface construction algorithm. *Computer Graphics* 21(4), 163–169.
- Gibson, S. F. and Lanni, F. (1991). Experimental test of an analytical model of aberration in an oil-immersion objective lens used in three-dimensional light microscopy. *Journal of the Optical Society of America A* 8(10), 1601–1613: the PSF left for a follow-up.

## What was built

- **`oscope-volume`:**
  - `psf`: `bessel_j0`, `born_wolf`, `Optics` and `psf`;
  - `deconvolve`: `Volume`, `DeconvolveOptions`, `Padded`, `richardson_lucy`, `blur`, `deconvolve` and `nrmse`;
  - its dev builds optimised (`opt-level = 2`), for the tests' 3D FFTs.
- **`oscope-compute`:** `Cuda::richardson_lucy`, on cuFFT's 3D real transforms, and four kernels in `kernels/deconvolve.cu`: the spectra's products, the ratio, total variation's normals, and the update.
- **`oscope-pipeline`:** the nodes `volume.psf` and `volume.deconvolve`. Its `cuda` feature turns on `oscope-volume`'s, so a server built with CUDA deconvolves on the GPU.
- **`oscope-workflows`:** the job, in `deconvolve` (`DECONVOLVE`, `DeconvolveJobOptions`, `deconvolve_acquisition`, `deconvolve_jobs`). It reads stacks as P42's job does, through `depth`'s `Stack`, now shared within the crate.
- **`oscope-api`:** the job, registered by the projects Thing.
- **Examples:** `examples/ex-deconvolve` (port 8248) and `examples/ex-checkpoint-e` (port 8249), with their pages. `ex-depth-from-focus` places its truth where the stage was.

## How to try it

`cargo run --release -p ex-deconvolve`, then open http://127.0.0.1:8248/. For Checkpoint E, `cargo run --release -p ex-checkpoint-e`, then open http://127.0.0.1:8249/. Their READMEs show what they print: [`ex-deconvolve`](../../../examples/ex-deconvolve/README.md) and [`ex-checkpoint-e`](../../../examples/ex-checkpoint-e/README.md).

- **On the owner's PC, at real time,** the z-scan of 21 planes took 19.9 s, and the deconvolution 11.0 s on the CPU: 684 × 456 × 21 voxels, 20 iterations. Built with `--features oscope-api/cuda`, it deconvolves on the GPU, in 1.4 s.
- **The page** draws the stack or its deconvolution as a maximum intensity projection, composited, or as an isosurface, with its level and its depth on sliders.
- **Checkpoint E** ran in 85.1 s. Its page holds both examples' pages.

## Performance

No §5.20 budget covers deconvolution.

- **The job on the CPU,** in a release build: 11.0 s for the example's 684 × 456 × 21 voxels, 20 iterations. They are padded to 729 × 500 × 63, and each iteration makes four 3D transforms of that size.
- **On the GPU,** the same with `--features oscope-api/cuda`: 1.4 s, eight times as fast, with the same NRMSE to three places, 1.149.
- **The z-scan** took 19.9 s at real time, the simulator rendering each plane.

## Design notes and deviations from the plan

### Where the spec moved

- **The background is taken off first:** the stack's 1st percentile, from a sample of at least a hundred thousand voxels evenly spaced, the result clamped at 0. The spec's u₀ = d is the data less its background.
  - **Why:** Richardson–Lucy takes all of the data for light that the PSF blurred. A camera's black level, about 2,000 counts of 65,535 on the MIChrome 20, isn't light. Left in, it is a floor the estimate must keep under everything, which damps each update.
  - **On the job's test,** which stores its stacks over that black level, the effect is mixed. Taken off, it brings the structures nearer their truth, an NRMSE of 1.302 against 1.324, and the cells a little further from theirs, 1.157 against 1.144.
  - **The 1st percentile, not the median,** as a sample may fill most of the field.
- **Total variation does better on noisier data, over more iterations:** the test's 5 % noise and 40 iterations, an NRMSE of 0.805 against 0.810 without it. At 2 % noise and 20 iterations it did no better. The spec asked for no worse; the test holds it to better, there.
- **The CUDA twin uses cuFFT's real transforms,** not its complex ones, as the volume is real: half the spectrum, half the memory. The transfer function's half spectrum is the CPU's, cut to the half the transforms keep (`Padded::half_otf`).
- **A volume of unknown voxels is refused too:** the PSF needs their size.
- **The PSF is sampled through a radial table** for each plane, every quarter of a voxel, each voxel's value taken between the nearest two: within 3 × 10⁻³ of the integral at a voxel between the table's samples.
- **The truth of the cells is averaged through each voxel's depth,** at 5 heights. The membranes are 0.3 µm thick, and a plane every 0.5 µm sampled at one height would miss them between planes.
- **The truth is placed where the stage was,** in both examples: the scan's first frame's position, not where the stage was sent. The simulated stage's play left the commanded position a few µm off.
  - **In `ex-deconvolve`'s smoke,** placed where the stage was sent, the stack's projection was an NRMSE of 0.560 from the truth's, and the deconvolution seemed further from the truth than the stack, 1.238 against 1.225. Placed where the stage was, the projection was 0.319 from it, and the deconvolution 1.165 against the stack's 1.182.
  - **In P42's `ex-depth-from-focus`,** the median error went from 0.071 µm to 0.054, and the dome's RMS from 0.13 µm to 0.10.
- **Modest gains on the simulator.** The example's cells go from an NRMSE of 1.171 to 1.149, and the job's test's structures from 1.353 to 1.302:
  - widefield at NA 0.55 resolves about 3 µm in depth, and the membranes are 0.3 µm thick;
  - a widefield transfer function is zero in a cone about the axis, the missing cone, and what falls there no deconvolution restores;
  - the simulator blurs by a Gaussian, not by Born and Wolf's PSF, so the PSF deconvolved by isn't the one that blurred.
- **Checkpoint E's page** frames the two examples' pages at `/depth/index.html` and `/deconvolve/index.html`: the server's pages are served by their files' paths, and not at a folder's.

### What the mutants' reading found

Each mutant broke the code on purpose, then its tests were run: fifty-six of fifty-six caught, fourteen of them by the GPU's test, on the GPU.

- **Reading the tests against the list first,** twice, found these gaps, now covered:
  - **the PSF:** its sampled axis never compared with the closed form; its size, and its voxels between the radial table's samples, never checked; the medium only ever 1;
  - **the padding:** its mirror never looked at; a point blurred never seen to stay where it is; data under 0 never given;
  - **every PSF symmetric,** so a correction convolved rather than correlated, on the CPU or in the kernel, would pass. One is now off its centre, in the energy's test and the GPU's;
  - **`deconvolve`'s GPU path** never run, nor its options passed;
  - **the progress,** on the CPU and the GPU, never seen to count from the first iteration;
  - **the job:** its options never shown to reach its result; its refusal of a flat image tested on one of unknown pixels, which fails both conditions at once; a volume of unknown voxels never given.
- **The run then missed four,** now caught:
  - **a quarter of the integral's intervals each turn.** The tests' points needed no more than its floor of 32. The axis far from focus, at u = 80 and 100, is now held within 1 % of its closed form; with a quarter of the intervals, it is 58 % off, then 170 times.
  - **voxels as deep as wide.** The record's voxel was checked in x alone, and now in y and z too.
  - **no background taken off, and the median taken for it.** The job's test passes either way, the effect being mixed. Now a field lit at 0.5, with a dark corner at 0.1, a sixteenth of it, must be left at 0.4 far from the corner.

| Mutant | Caught by |
|---|---|
| J₀'s series past its range; its phase without π/4; Simpson's odd weights 2; no floor of 32 intervals; a quarter of the intervals a turn; the amplitude's scale halved | J₀'s table and Born and Wolf's closed forms |
| u without the medium; the cone's radius paraxial; one Airy radius of margin; the radial table every voxel; the interpolation's weights swapped; the mirror planes left empty; not normalised; an NA at the medium's index taken | the sampled PSF |
| The mirror's period one short; padded by repeating the edge; data under 0 kept; the transfer function not centred, or not in z; the crop's y offset lost; the transform along y, or z, skipped | the padding, the blur and the crop |
| Total variation's sign; the correction convolved, not correlated; the iterations told from 0; the NRMSE's correlation unnormalised | Richardson–Lucy on the object: its error, its energy, total variation and the cancel |
| The half spectrum one short; total variation's gradient along y backward, or its divergence without z; `deconvolve` giving the GPU no total variation, or 20 iterations | the GPU against the CPU |
| The kernel's conjugate dropped; its product's sign; its normals without z; its divergence of y read from x; its total variation's sign; its ratio inverted; the GPU's iterations told from 0; its normals not made; its correction convolved | the GPU against the CPU |
| The PSF node's medium, or the deconvolve node's total variation, ignored | the nodes |
| Not a volume taken; unknown voxels taken; the NA, the wavelength, the medium or total variation ignored; a cancel ignored; no background taken off; the median for the background | the job's options, background, refusals and cancel |
| Raw cells the pixels' size; voxels as deep as wide; one iteration; the planes written in reverse | the job over the cells and the structures |
| The job registered under another name | `ex-deconvolve --smoke` |

### The page

- **The volume** is VTK.js's `vtkImageData` from the float brick, through `vtkVolumeMapper` as a maximum intensity projection or composited, or as an isosurface by `vtkImageMarchingCubes` through `vtkMapper`. The voxels' spacing comes from the metadata, and the depth slider stretches z.
- **Seen in a browser?** Not here. Under Node, against the running example, the page ran with VTK.js's real objects, all but the WebGL render window, without errors: the brick of 684 × 456 × 21 voxels of 0.384 × 0.384 × 0.5 µm, the projection, then composited, then the isosurface, of 190,494 points deconvolved and 134,838 for the stack, the depth stretched 4 times. Drawing it is for the owner's manual test.

### Two cores

- **Five times each, side by side,** with `scripts/two-core.sh` (ADR-0038), every suite P43 adds or changes passed 5 times in 5, each a run in:

  | Suite | Seconds |
  |---|---|
  | `oscope-volume`'s `deconvolve` | 7.5–7.7 |
  | `oscope-pipeline`'s `stock` | 0.7–1.3 |
  | `oscope-workflows`'s `deconvolve` | 23.6–24.3 |
  | `oscope-workflows`'s `depth` | 16.1–16.8 |
  | `oscope-api`'s `projects` | 0.7–2.2 |
  | `ex-deconvolve --smoke` | 60.7–65.4 |
  | `ex-checkpoint-e --smoke` | 99.0–117.7 |
  | `ex-depth-from-focus --smoke` | 46.8–55.1 |

- **They found no fault.**

### The size

L's budget is 900; P43 measured 2,733, 9 % under its estimate of ~3,000.

- **One pull request,** under the owner's standing choice: the PSF, Richardson–Lucy, its twin, the job and the examples are one path.
- **Against the estimate:**
  - code took 1,081 lines against ~1,010;
  - tests 823 against ~750;
  - the examples 760 against ~700;
  - glue 69 against ~60.
- **Near the estimate in all, if not part by part:** `oscope-volume` took 566 lines against ~430, and the job 235 against ~200. The CUDA twin took 183 against ~230, on cuFFT's real transforms, and the nodes 93 against ~150. The tenth on top went unspent.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-volume` | `deconvolve` (3) | **The PSF:** J₀ against Abramowitz and Stegun's table within 10⁻⁷; Born and Wolf's 1 at focus, (sin(u/4)/(u/4))² on the axis within 10⁻³, dry and in water, and Airy's first zero; sampled, summing to 1, symmetric, its size the spec's, and a voxel between the table's samples within 3 × 10⁻³ of the integral; refusals. **Richardson–Lucy** on an object of beads and a shell, blurred by the PSF, with noise: the NRMSE falling through 1, 5, 10 and 20 iterations, 1.08, 1.00, 0.93 and 0.86; the energy kept within 10⁻⁵ with the PSF centred and off its centre; total variation better on noisier data; a cancel, told of each iteration from the first. **Padding** mirrored about each edge, the data clamped at 0, sizes of 2, 3, 5 and 7, and a point blurred staying where it is; planes and back; an even PSF refused; the NRMSE ignoring units and offsets |
| `oscope-volume` | `cuda` (1 new) | **On the GPU,** with `OSCOPE_CUDA=1`, a PSF off its centre: the CPU's estimate within 10⁻³ of its largest voxel after 10 iterations, without and with total variation (1.4 and 1.6 × 10⁻⁶); a cancel, told of each iteration from the first; `deconvolve` taking the GPU with its options |
| `oscope-pipeline` | `stock` (1 new) | **The nodes,** in water and with total variation, give what the library gives |
| `oscope-workflows` | `deconvolve` (2) | **The job** over widefield z-stacks of the simulator's cells and sub-diffraction structures at 50×, raw, with the camera's black level and noise: nearer the truth than the stack, the cells' NRMSE from 1.182 to 1.157, and the structures' from 1.353 to 1.302; its record, provenance and voxel size; its progress to the 20th iteration. Each option reaching the result; a cancel; a flat image and a volume of unknown voxels refused |
| `ex-deconvolve` | `--smoke` | Over the frame's middle ninth: the deconvolution nearer the cells' truth than the stack; the result's voxels, one for each of the stack's cells; the page and VTK.js |
| `ex-checkpoint-e` | `--smoke` | The four results; the heights' and the deconvolved volume's metadata; the page, both examples' pages and their scripts, and VTK.js |
| `ex-depth-from-focus` | `--smoke` (changed) | Its truth placed where the stage was |

**Mutation checks:** fifty-six of fifty-six caught, fourteen of them by the GPU's test, on the GPU; see what the mutants' reading found, above.

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh --cuda`, run alone on `8290957`, in 15 minutes: 60 of 60 steps, 540 tests in 203 suites.
  - **The CUDA tests ran on the GPU,** as P43 adds CUDA kernels: the twin within 2 × 10⁻⁶ of the CPU, and `deconvolve` taking the GPU.
  - **The rest:** the Python tests ran for real, and every example's `--smoke`, `ex-deconvolve`'s and `ex-checkpoint-e`'s among them. Each suite and smoke run had two CPUs of its own, eight at a time (ADR-0038).
  - **The first run, on `1c3815a`, failed two steps:**
    - **the docs.** `oscope-volume`'s crate docs linked `psf` and `deconvolve`, each both a module and a function. `8290957` links the modules.
    - **`oscope-hal`'s conformance suite,** which P43 doesn't touch. The fake camera dropped a frame under the replay's load, and three of its tests failed on it. The suite then passed ten times in ten on two cores, and in the rerun.
  - The commit since adds only these notes.
- **On Linux,** `scripts/ci-clone.sh` under WSL, run alone on a fresh clone of `8290957`, in 7 minutes: 59 of 59 steps, 540 tests in 203 suites, the Python tests real; 754 files, none with a CR. WSL has no CUDA toolkit.
- **On two cores,** five times each, side by side: the suites in the table above.

## Follow-ups and known gaps

- **Gibson and Lanni's PSF** (1991), for a coverslip or an immersion medium that isn't the lens's design: Born and Wolf's is free of aberration.
- **The simulator blurs by a Gaussian.** Rendering its widefield planes by Born and Wolf's PSF would let deconvolution be tested against its own model.
- **A measured PSF:** the node `volume.deconvolve` takes any PSF as a stack, but nothing measures one from beads.
- **Volumes larger than memory:** on the CPU, the padded volume, the transfer function, the spectrum and the steps between take about 40 bytes a padded voxel, by their buffers: some 0.9 GB for the example's. A slide's tiled z-stack would need blocks with overlaps.
- **The job reserves no GPU memory** from the scheduler (`gpu_mib` 0), as P42's doesn't. Its buffers take about 20 bytes a padded voxel, 32 with total variation, besides cuFFT's work areas.
- **Total variation's weight** is the user's to choose; nothing estimates it from the noise.
- **`oscope-hal`'s conformance suite** runs the fake camera in real time, and dropped a frame once under the replay's load. P12 slowed the simulator's run of it for the same reason; the fake's run may need the same.
- **The job runs on the GPU** through the compute layer when the server is built with CUDA. The examples' builds run on the CPU, unless built with `--features oscope-api/cuda`.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The PSF** is Born and Wolf's integral (1999, §8.8), by Simpson's rule, with J₀ from Abramowitz and Stegun's polynomials (1964, 9.4.1 and 9.4.3).
- **Richardson–Lucy** follows Richardson (1972) and Lucy (1974), and its total variation Dey et al. (2006). Mirrored padding and convolution by FFT are textbook.
- **The NRMSE** of standardised volumes is elementary.
- **The FFTs** are rustfft's (MIT or Apache-2.0), a dependency already, and cuFFT's, loaded at run time from the CUDA toolkit through `cudarc`, not redistributed.
- **VTK.js 38.0.4** is P41's vendored copy. The page uses its documented classes, marching cubes among them (Lorensen and Cline, 1987).
