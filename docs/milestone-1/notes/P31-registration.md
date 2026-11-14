# P31: Registration: phase correlation on CPU and CUDA

- Status: In review
- Branch / PR: `m1/p31-registration` / [#45](https://github.com/lucaswewa/OScope/pull/45)
- Size: M · estimated ~1,900 before it started (phase correlation on the CPU ~450; on CUDA ~300; the batch API ~100; tests ~500; `ex-register` ~200; glue ~30; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,052** hand-written lines added (code 932: `oscope-imaging` 627, `oscope-compute` 305; tests 815; examples 262; glue 43), and 10 removed. **That is 1,452 over M's 600, and about 8 % over the estimate:** the overlap pass, the plans kept, and the tests of large shifts and of the setup came after it.

## Summary

Registration: the shift between two images, to a fraction of a pixel, and how sure it is ([spec](../phases.md#p31); plan §5.20).

- **Phase correlation** (Kuglin and Hines, 1975) with the Hann window, a Gaussian weight on the normalised cross-power spectrum, and the three-point Gaussian estimate of the peak (Willert and Gharib, 1991). Known shifts come back within 0.1 px on textures sharp and blurred as the 5× to the 20× blur, with noise, and within 0.05 px on the simulator's tissue, end to end.
- **The overlap pass** registers the part two images share again, so that large shifts lose nothing to the window.
- **The confidence,** the peak ratio, tells a good pair from a featureless or barely overlapping one.
- **An expected shift and a radius** restrict the search, for calibration and scans.
- **On the CPU** with `rustfft`, the reference; **on CUDA** with cuFFT's batched plans and three kernels, within 0.01 px of it. A 2048² pair takes about 11 ms on the GPU, within §5.20's 15 ms.
- **The spec was revised twice,** before the tests and before the example, each time from a measurement (see the design notes).
- **P30 is marked Done.**

## Behaviour spec

Written before the code, as a spec-first phase's must be ([spec](../phases.md#p31)).

### What registration answers

Given two images of the same size, **A** (the reference) and **B**, registration finds the translation that carries A's content onto B's: the shift **d = (dx, dy)**, in pixels, such that **B(x, y) ≈ A(x − dx, y − dy)** where they overlap. A positive `dx` means B's content lies to the right of A's. It also says how sure it is.

```text
register(a, b, options) -> Registration { dx, dy, confidence, peak }
register_batch(pairs, options) -> [Registration]
```

- **Images:** one channel of `f32`, both the same size. A caller with colour converts to grey first.
- **Options:**
  - **an expected shift** `(ex, ey)` and **a search radius** `r` in pixels: only shifts within `r` of the expected one, along each axis, are considered. Without an expected shift, every shift is. Calibration, scans and autofocus checks know roughly where the answer lies, and restricting the search keeps a stronger false peak elsewhere from winning;
  - **the window:** the Hann window, which may be turned off;
  - **the weight's width** σ_w, in pixels: 2 unless set (see the method's step 4).

### The method: phase correlation (Kuglin and Hines, 1975)

1. **Mean and window.** From each image its mean is subtracted, then it is multiplied by the separable Hann window, w(x, y) = h(x) · h(y), with h(n) = ½ (1 − cos(2πn / (N − 1))) for n = 0 … N − 1 along an axis of N pixels (Harris, 1978). The window tapers each image to zero at its edges, so the discontinuity where the discrete Fourier transform wraps an image round doesn't dominate the spectrum.
2. **Transforms.** F_A = F(a) and F_B = F(b), the two-dimensional discrete Fourier transforms, at the images' own size, without padding.
3. **The normalised cross-power spectrum.** R = F_B · conj(F_A) / |F_B · conj(F_A)|. Where the magnitude is below 10⁻³⁰, R is 0. Only the phase remains, whatever the images' contrast at each frequency.
4. **The weight.** R is multiplied by a Gaussian of the frequency, G(f) = exp(−2π² σ_w² |f|²), where f = (f_x, f_y) is in cycles per pixel: f_x = k/W for k < W/2, and k/W − 1 otherwise, and likewise f_y. Weighting the cross-power spectrum is the generalised cross-correlation's idea (Knapp and Carter, 1976), and Takita et al. (2003) weight phase-only correlation's spectrum against noise. This weight does two things:
   - **the peak's shape:** the weighted surface is the impulse at d blurred by a Gaussian of standard deviation σ_w pixels, so near its peak it is a sampled Gaussian, whatever the shift;
   - **the noise:** a microscope's image holds little above its optics' cut-off, so there the phase is the noise's and the window's, not the shift's. Unweighted, those frequencies count as much as the image's own, and pull the peak toward whole pixels (as measured in the design notes). Weighted, they count for little.
5. **The correlation surface.** r = F⁻¹(R · G), whose real part is kept, scaled by 1 / ΣG, so that a perfect match peaks at 1. If B is A shifted by d, the shift theorem makes R = e^(−2πi f·d), and r the Gaussian centred at d: its peak's place is the shift.
6. **The integer peak.** The largest value of r, among the shifts allowed: a surface index (x, y) means a shift of x, or of x − W for x ≥ W/2, and likewise for y. So shifts run from −W/2 to W/2 − 1.

### The sub-pixel peak (Willert and Gharib, 1991)

The logarithm of a Gaussian is a parabola, so three samples fix its centre exactly. Along each axis, with c₀ the peak's value and c₋ and c₊ its neighbours before and after it, the shift is refined by the three-point Gaussian estimate:

```text
δ = (ln c₋ − ln c₊) / (2 (ln c₋ − 2 ln c₀ + ln c₊))
```

It is taken along x, then along y, each from the peak's row or column. If a neighbour isn't positive, there is no refinement along that axis.

**The weight's width.** The weighted surface is that Gaussian only where the images carry the shift's phase across the weight's band: G falls to e⁻² at |f| = 1 / (π σ_w), 0.16 cycles per pixel for 2 px.

- **The instrument's blur:** in focus, a Gaussian of σ = 0.21 λ / NA (Zhang, Zerubia and Olivo-Marin, 2007), as the simulator models it. In green light, on the MIChrome 20's 2.4 µm pixels, that is 1.7 px for the 5× and 10×, 2.3 px for the 20×, and 4.4 px for the 50×.
- **So σ_w = 2 px** suits the 5× to the 20×. A caller registering a blurrier image sets σ_w to about its blur: 4.4 px for the 50×.
- **σ_w is at least 1 px.** A narrower Gaussian is too narrow for its samples, and is refused.

### The overlap pass

The window weights the content that A and B share by w(x) · w(x + d), which falls as the shift grows, and the share itself shrinks with it. Both lean a broad peak toward smaller shifts: more for smaller images and larger shifts (as measured in the design notes). So, unless the integer peak is at (0, 0):

1. **The cut.** A and B are cut to the part they share at the integer peak's shift: its middle, at most 512 px a side.
2. **Again.** The two parts are registered by the steps above, about (0, 0) within 2 px, where the lean vanishes: the window and the share are the same on either side of no shift.
3. **The shift** is the integer peak's plus the second registration's. The confidence and the peak are the first's.

Where the part shared is under 8 × 8, or the second peak isn't positive, the first registration stands.

### Confidence: the peak ratio

- **c₀** is the peak's value: near the fraction of the images that overlap, where they carry the shift's phase across the weight's band. Noise gives a surface of many small peaks.
- **c₂** is the largest value of r outside the integer peak's neighbourhood, among the shifts considered: the whole surface without an expected shift. The neighbourhood is the shifts within ⌈3σ_w⌉ pixels of the peak along both axes: a square of 13 × 13 for 2 px. There, the peak's own Gaussian has fallen to e^(−4.5), about 1 %.
- **confidence = clamp(1 − max(c₂, 0) / c₀, 0, 1)**: near 1 when one peak stands out, near 0 when the best peak is barely above the rest, or c₀ ≤ 0.

A registration reports **peak** = c₀ too. A caller decides on a threshold: the tests set what a good pair and a poor one score. With a search radius within the neighbourhood, no shift lies outside it, and the confidence is 1 whenever c₀ > 0.

### The batch

`register_batch` registers many pairs of the same size with the same options, in order, and gives each its registration as if alone. On the CPU, pairs run in parallel. On CUDA, the pairs' first transforms are one batched plan, and each pair's overlap pass follows alone.

### The backends

- **The CPU,** with `rustfft`'s transforms, is the reference.
- **CUDA,** with cuFFT's, computes the same steps on the GPU, then gathers and brings back only the shifts considered, and a pixel around them: the search window, or the whole surface without an expected shift. The CPU gathers the same region from its surface. The peak, its refinement and the confidence are then found from the region by the same code for both.
- **Equivalence:** for the same pair, the two agree within 0.01 px on the shift, and within 0.01 on the confidence.

### What the tests show

- **Known shifts,** integer and sub-pixel, of a textured field with noise added to both images, sharp and blurred as the 5× to the 20× blur: each recovered within **0.1 px**.
- **Large shifts,** of 35 % to 43 % of the field: within 0.05 px, which they miss without the overlap pass.
- **Low confidence** for a featureless pair (noise alone), and for a pair that barely overlaps; high for a good pair.
- **The expected shift** finds the true peak where a stronger false one lies outside the radius.
- **CPU and CUDA agree,** as above, when `OSCOPE_CUDA=1`.
- **§5.20's budget:** a 2048² pair, on CUDA, from its images in host memory to its registration, in at most **15 ms**. It is measured in a release build on the owner's PC, and recorded here.
- **The simulator, end to end:** while the stage jogs at a constant speed **v**, consecutive frames a time **Δt** apart register to the stage's displacement seen through the camera's mounting:

  ```text
  d = −L⁻¹(v · Δt) / p
  ```

  - **L** is the mounting's turn and mirror, which maps a step along the sensor's axes to one along the stage's;
  - **p** is a pixel's size on the sample: the sensor's pixel, divided by the magnification;
  - the minus sign is there because content moves across the sensor against the stage's motion.

  The rolling shutter shears both frames alike at a constant speed, so their shift is uniform.

### Sources

- C. D. Kuglin and D. C. Hines, "The phase correlation image alignment method", *Proceedings of the IEEE International Conference on Cybernetics and Society*, 1975, pp. 163–165.
- F. J. Harris, "On the use of windows for harmonic analysis with the discrete Fourier transform", *Proceedings of the IEEE* 66(1), 1978, pp. 51–83. doi:10.1109/PROC.1978.10837.
- C. H. Knapp and G. C. Carter, "The generalized correlation method for estimation of time delay", *IEEE Transactions on Acoustics, Speech, and Signal Processing* 24(4), 1976, pp. 320–327. doi:10.1109/TASSP.1976.1162830.
- K. Takita, T. Aoki, Y. Sasaki, T. Higuchi and K. Kobayashi, "High-accuracy subpixel image registration based on phase-only correlation", *IEICE Transactions on Fundamentals* E86-A(8), 2003, pp. 1925–1934.
- C. E. Willert and M. Gharib, "Digital particle image velocimetry", *Experiments in Fluids* 10(4), 1991, pp. 181–193. doi:10.1007/BF00190388.
- B. Zhang, J. Zerubia and J.-C. Olivo-Marin, "Gaussian approximations of fluorescence microscope point-spread function models", *Applied Optics* 46(10), 2007, pp. 1819–1829. doi:10.1364/AO.46.001819.
- H. Foroosh, J. B. Zerubia and M. Berthod, "Extension of phase correlation to subpixel registration", *IEEE Transactions on Image Processing* 11(3), 2002, pp. 188–200. doi:10.1109/83.988953: the spec's first sub-pixel estimate, replaced before the tests (see the design notes).
- M. Guizar-Sicairos, S. T. Thurman and J. R. Fienup, "Efficient subpixel image registration algorithms", *Optics Letters* 33(2), 2008, pp. 156–158. doi:10.1364/OL.33.000156: the upsampled refinement the design notes compare against.

## What was built

| Where | What |
|---|---|
| `oscope-imaging`, `registration` | `register`, `register_batch`, `RegisterOptions`, `Registration`, `RegistrationError`: the CPU's steps; the search region, the peak and the overlap pass, which both backends share |
| `oscope-imaging`, `registration::gpu` | `register` and `register_batch` on CUDA |
| `oscope-compute`, `cuda::registration` and `kernels/registration.cu` | `Cuda::phase_correlation` and `Correlation`: the window, the weighted cross-power spectrum and the region's gathering as kernels, around cuFFT's batched transforms; each lane's latest plans kept |
| `oscope-sim`'s tests | `rig_over`: the tests' rig over any world |
| `examples/ex-register` | Registration on fields of the simulated slide, on the CPU and the GPU, and a 2048² pair timed |

## How to try it

[`ex-register`](../../../examples/ex-register/README.md):

```sh
cargo run --release -p ex-register --features cuda -- --bench
```

It renders pairs of fields of the simulated H&E slide at 10×, with known shifts, registers them on the CPU and the GPU, and prints what it found, the confidence and the times. `--bench` adds a 2048² pair. Without the CUDA toolkit, leave out `--features cuda`.

## Performance

**§5.20, a 2048² pair on CUDA in at most 15 ms: met.** From the images in host memory to the registration, the overlap pass included, the median of 10 or 11 runs in release builds on the owner's PC (RTX A2000 12 GB) was 10.9 to 12.1 ms. `registration_gpu`'s test asserts it in release builds.

- **The first pass, 9.4 ms:**
  - on the GPU, 7.4 ms: the two images up, 2.7 ms; the whole surface down, as every shift is considered, 2.9 ms; the kernels and transforms, 1.8 ms;
  - on the host, about 2 ms: the means, and the peak found in the surface.
- **The overlap pass,** on parts of 512², about 1.5 ms.
- **What made it fit:**
  - finding the peak row by row in parallel: 6 ms to under 1;
  - keeping cuFFT's plans: 1.7 ms;
  - the overlap pass's cut at 512 px rather than 1,024: from a median of 13.9 ms.
- **The CPU** takes 0.33 s for the same pair, with 24 threads. A 512² pair takes about 19 ms on the CPU, and 2.5 ms on the GPU.

## Design notes and deviations from the plan

### The spec, revised before the tests

The spec first refined the unweighted surface's peak by Foroosh, Zerubia and Berthod's estimate, δ = s · c₁ / (c₁ + c₀), which assumes the surface is a sampled Dirichlet kernel: an image whose every frequency carries the shift. Before writing the tests, a scratch program measured it on what the instrument sees:

- **the pairs:** 256 × 256 windows of a random field, blurred by a Gaussian as the optics blur, with B shifted by ten shifts from (0, 0) to (12.45, −9.9) px, sub-pixel among them;
- **the noise:** Gaussian, added to both images, with the field's standard deviation SNR times the noise's.

The worst error over the ten shifts, in pixels:

| Blur σ | SNR | Foroosh, unweighted | Gaussian estimate, σ_w = 2 px |
|---|---|---|---|
| 0 (sharp) | ∞ / 10 / 4 | 0.001 / 0.002 / 0.004 | 0.005 / 0.015 / 0.024 |
| 1.0 px | ∞ / 10 / 4 | 0.005 / 0.256 / 0.316 | 0.009 / 0.014 / 0.017 |
| 1.7 px (5×, 10×) | ∞ / 10 / 4 | 0.065 / 0.421 / 0.443 | 0.017 / 0.020 / 0.021 |
| 2.5 px (beyond 20×'s 2.3) | ∞ / 10 / 4 | 0.341 / 0.473 / 0.487 | 0.029 / 0.030 / 0.038 |
| 4.4 px (50×) | ∞ / 10 / 4 | 2.494 / 1.022 / 1.411 | 0.107 / 0.180 / 0.221 |

- **Foroosh's estimate holds only for sharp images.** Above the optics' cut-off the phase is noise, and the window's leakage, so the unweighted peak is no Dirichlet kernel: the estimate locks to whole and half pixels.
- **It isn't only the estimate.** Refining the unweighted surface by evaluating its inverse transform on a 0.05 px grid (Guizar-Sicairos, Thurman and Fienup's upsampling, done directly) still missed by 0.23 px at 1.7 px and SNR 10. The noise-only frequencies, given full weight, move the peak itself.
- **The Gaussian weight fixes both.** It makes the peak a sampled Gaussian, which three samples fit exactly, and turns the noise-only frequencies down.
- **The 50×** needs σ_w at its blur. On 512 × 512 windows at a blur of 4.4 px, σ_w = 4.4 px gave 0.028 / 0.026 / 0.037, where 2 px gave 0.062 / 0.145 / 0.199.
- **The confidence** with the neighbourhood of ⌈3σ_w⌉ px was at least 0.94 for every pair above with a blur up to 2.5 px, at σ_w of 1.5 to 3 px. A 5 × 5 neighbourhood would have held the broader peak's own shoulders: about 0.5 at 2 px.

### The overlap pass, added while writing the example

`ex-register` registered a pair of the simulator's tissue 90 px apart on a 256 px field, and missed by 0.3 px: toward no shift. The tests' shifts had been at most 30 px, where the lean is small. So the spec gained the overlap pass. The error against the shift, on the tissue at 10×, each shift along x plus 0.3 px, found in a window of ±8 px around it:

| Field | Overlap pass | 2 % | 5 % | 8 % | 12 % | 20 % | 30 % | 40 % |
|---|---|---|---|---|---|---|---|---|
| 256 px | without | 0.030 | 0.062 | 0.088 | 0.124 | 0.182 | 0.267 | 0.399 |
| 256 px | with | 0.010 | 0.009 | 0.012 | 0.011 | 0.013 | 0.018 | 0.019 |
| 512 px | without | 0.006 | 0.016 | 0.027 | 0.041 | 0.071 | 0.105 | 0.131 |
| 512 px | with | 0.000 | 0.001 | 0.000 | 0.002 | 0.002 | 0.001 | 0.001 |
| 1024 px | without | 0.003 | 0.007 | 0.012 | 0.019 | 0.033 | 0.048 | 0.057 |
| 1024 px | with | 0.000 | 0.001 | 0.001 | 0.001 | 0.001 | 0.000 | 0.001 |

- **The lean** grows with the shift and falls with the field's size, as the slope of the window's weight does. Without the window, the shrinking overlap alone leaned the random field's peak about as much.
- **The cut's size,** 512 px, keeps a 2048² pair within §5.20's 15 ms on the GPU. With the whole overlap, up to 1024 px, the median was 13.9 ms, too close to the budget.
- **The simulator's end-to-end test** had found its longer jog within 0.074 px; with the overlap pass, 0.018.

### Other choices

- **Both backends compute from the same inputs.**
  - `oscope-imaging` computes each image's mean (in `f64`, in a fixed order), the window and the weight once, and passes them to the kernels.
  - The kernels compute each step in the CPU's order, with fused multiply-adds off. Only the transforms differ, `rustfft`'s and cuFFT's.
  - The two agreed within 0.01 px and 0.01 of confidence wherever they were compared.
- **The magnitude** of the cross power is |F_A| · |F_B|, taken apart. It equals |F_B · conj(F_A)|, but the product of two large spectra of raw counts can't overflow `f32`.
- **cuFFT's real transforms** compute half the spectrum, `W/2 + 1` columns, so the GPU's weight is the CPU's first `W/2 + 1` values. The inverse assumes the cross power is Hermitian, as it is for real images.
- **cuFFT's plans** are kept for each lane's four latest shapes: making a pair of plans took 1.7 ms of the 2048² pair's time. A call holds its lane's plans while it runs; the lane's stream runs its calls in turn anyway.
- **Memory.**
  - The CPU's batch runs as many pairs at once as fit in 256 MiB of working memory; the GPU's, 1 GiB.
  - A batch on the GPU agrees with the same pairs registered alone to 10⁻⁶ px, not to the bit: cuFFT plans each batch size its own way.
- **The peak** is the first of equal values in reading order, found row by row in parallel. Its refinement is held within a pixel of it, which matters only for a peak at the search window's edge.
- **A flat pair** has no phase to compare: its surface is 0, and it registers with confidence 0 and peak 0.
- **The simulator's end-to-end shift has the rolling shutter's term.**
  - The spec's d = −L⁻¹(v · Δt) / p holds along the rows. Content moving down the sensor is read later, so dy = u_y Δt / (1 − u_y L) and dx = u_x (Δt + dy L), as P16's jog test derived it.
  - The spec said the shift is uniform, which it is; its size has that term.
- **The simulated camera leaves `FrameMeta.position` empty,** so the end-to-end test takes the stage's velocity from its jog.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-imaging` | `registration` (4, unit) | **The peak:** a sampled Gaussian's centre found within 10⁻⁴ px; the confidence against the highest value outside the peak's neighbourhood, with the margin left out; nothing positive, no confidence. **The search regions:** every shift, or the window, each with its margin. **The setup** as the spec has it: the window, the weight, the scale to 1, the neighbourhood |
| `oscope-imaging` | `registration` (9) | **Known shifts:** ten from none to (12.45, −29.9) px, of a random field sharp and blurred by 1.7 and 2.3 px, with and without noise (SNR 4), within 0.1 px (the worst 0.062) and a confidence of at least 0.9 (the least 0.93). **Large shifts,** of 35 % to 43 % of the field, everywhere and in a window, within 0.05 px (the worst 0.015). **Low confidence:** two unrelated fields (at most 0.07, peak 0.07), and pairs sharing a tenth of the field (at most 0.22); a good pair 0.97 and more. **A flat pair:** confidence 0, peak 0, no NaN. **The expected shift** finds a faint true copy (within 0.074 px) where a stronger false one, outside the radius, wins without it. **A rolled field without the window:** exact, to 3 × 10⁻⁷ px. **A batch** gives what each pair gives alone, to the bit. **Refusals:** no pairs, sizes that differ, colour, under 8 × 8, σ_w under 1 or not finite, an expected shift not finite, a pixel not finite |
| `oscope-imaging` | `registration_gpu` (2, `OSCOPE_CUDA=1`) | **The GPU agrees with the CPU** within 0.01 px and 0.01 of confidence: four shifts at 256², 250 × 190, 255 × 129 and 64 × 9, everywhere, in a window, and without the window; a pair alone as in its batch, within 10⁻⁴ px. **§5.20:** in a release build, a 2048² pair's median within 15 ms |
| `oscope-sim` | `registration` (1) | **End to end,** jogging under the default sample at 10×: 70 µm/s along x through a mounting turned 30° and mirrored, and 250 µm/s along y turned −12°. Consecutive frames register to the stage's displacement through the mounting, the rolling shutter's term included, within 0.05 px (0.010 and 0.018), at confidences 0.98 and 0.86 |
| `oscope-sim` | `camera` | Unchanged: its rig is now built by `rig_over` |
| `ex-register` | `--smoke` | The four cases on 256² fields |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh --cuda`, run alone on `7f964e0`: 42 of 42 steps, 407 tests in 160 suites, the Python tests real, every example's `--smoke` with `ex-register`'s new among them, and the CUDA tests.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of the same commit, run alone: 41 of 41 steps, 407 tests in 160 suites, the Python tests real; 570 files, none with a CR.
- **On two cores,** pinned, 5 times out of 5 each: `oscope-imaging`'s `registration`, its unit tests, and `registration_gpu` with `OSCOPE_CUDA=1`; `oscope-sim`'s `registration` and `camera`; and `ex-register --smoke`.
- **In release builds,** `registration_gpu` with `OSCOPE_CUDA=1`, §5.20's test among them.

**Mutation checks** (each broke the code on purpose, then the tests were run): eighteen of eighteen caught. Reading the tests against the list first found two gaps, now covered:

- the window and the weight could be wrong, or missing, while the shift tests passed;
- nothing registered a flat pair, which the guard for no magnitude protects.

| Broken on purpose | Caught by |
|---|---|
| No Gaussian weight | `the_window_the_weight_and_the_neighbourhood_are_as_specified`; and the known, large and expected shifts, and the good pair's confidence |
| The surface not scaled to 1 | `an_integer_shift_round_the_frame_is_exact_without_the_window`, `a_featureless_pair_scores_low_and_a_good_pair_high` |
| No window | `the_window_the_weight_and_the_neighbourhood_are_as_specified` |
| The window over N, not N − 1 | the same |
| The cross power conjugates B, not A | the known, large and expected shifts, and the rolled field |
| No guard for no magnitude | `a_flat_pair_gives_no_confidence_and_no_nan`, the rolled field, and the known shifts |
| The peak's neighbourhood left in | the good pair's confidence, the known shifts, and the rolled field |
| The search window ignored | `the_expected_shift_finds_the_true_peak_where_a_stronger_false_one_lies_outside` |
| No sub-pixel refinement | the known, large and expected shifts |
| The Gaussian estimate's sign flipped | the same |
| No overlap pass | `large_shifts_lose_no_precision_to_the_window_or_the_shrinking_overlap` (0.242 px) |
| The overlap's cuts misaligned | the large, known and expected shifts |
| The second pass's shift subtracted | the same |
| CUDA: the gather doesn't wrap | `the_gpu_agrees_with_the_cpu` |
| CUDA: the cross power conjugates B | the same |
| CUDA: no weight | the same |
| CUDA: no mean taken away | the same |
| CUDA: the half spectrum's weight off by one | the same |

## Follow-ups and known gaps

- **Rotation and scale** aren't registered: translation only. Calibration (P32) finds the mounting's turn from several translations.
- **σ_w for blurrier optics** is the caller's to set. P32 can set it from the objective's NA and the pixel size.
- **The GPU's copies** are most of its time.
  - An entry point for images already on the GPU, frames converted there, would save the 2.7 ms upload.
  - Finding the peak on the GPU would save the 2.9 ms download, at the cost of the peak code that both backends share.
- **A batch's overlap passes** run one pair at a time on the GPU.
- **Colour** is the caller's to convert to grey.
- **`FrameMeta.position` from the simulated camera,** if a caller wants the stage's position with each frame.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each step is written from the papers' equations, as the spec above sets them out:

- phase correlation follows Kuglin and Hines (1975);
- the window, Harris (1978);
- the weighting of the cross-power spectrum, Knapp and Carter (1976), and Takita et al. (2003);
- the three-point Gaussian estimate, Willert and Gharib (1991);
- the blur's Gaussian, Zhang, Zerubia and Olivo-Marin (2007), as the simulator's optics already cite it.

Foroosh, Zerubia and Berthod's estimate (2002) and Guizar-Sicairos, Thurman and Fienup's upsampling (2008) were measured against in a scratch program, and not kept.
