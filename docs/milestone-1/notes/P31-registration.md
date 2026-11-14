# P31: Registration: phase correlation on CPU and CUDA

- Status: In review
- Branch / PR: `m1/p31-registration` / [#45](https://github.com/lucaswewa/OScope/pull/45)
- Size: M · estimated ~1,900 before it started (phase correlation on the CPU ~450; on CUDA ~300; the batch API ~100; tests ~500; `ex-register` ~200; glue ~30; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured {{SIZE}}

## Summary

{{SUMMARY}}

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

### Confidence: the peak ratio

- **c₀** is the peak's value: near the fraction of the images that overlap, where they carry the shift's phase across the weight's band. Noise gives a surface of many small peaks.
- **c₂** is the largest value of r outside the integer peak's neighbourhood, among the shifts considered: the whole surface without an expected shift. The neighbourhood is the shifts within ⌈3σ_w⌉ pixels of the peak along both axes: a square of 13 × 13 for 2 px. There, the peak's own Gaussian has fallen to e^(−4.5), about 1 %.
- **confidence = clamp(1 − max(c₂, 0) / c₀, 0, 1)**: near 1 when one peak stands out, near 0 when the best peak is barely above the rest, or c₀ ≤ 0.

A registration reports **peak** = c₀ too. A caller decides on a threshold: the tests set what a good pair and a poor one score. With a search radius within the neighbourhood, no shift lies outside it, and the confidence is 1 whenever c₀ > 0.

### The batch

`register_batch` registers many pairs of the same size with the same options, in order, and gives each its registration as if alone. On the CPU, pairs run in parallel. On CUDA, the pairs' transforms are one batched plan.

### The backends

- **The CPU,** with `rustfft`'s transforms, is the reference.
- **CUDA,** with cuFFT's, computes the same steps on the GPU, then gathers and brings back only the shifts considered, and a pixel around them: the search window, or the whole surface without an expected shift. The CPU gathers the same region from its surface. The peak, its refinement and the confidence are then found from the region by the same code for both.
- **Equivalence:** for the same pair, the two agree within 0.01 px on the shift, and within 0.01 on the confidence.

### What the tests show

- **Known shifts,** integer and sub-pixel, of a textured field with noise added to both images, sharp and blurred as the 5× to the 20× blur: each recovered within **0.1 px**.
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

{{BUILT}}

## How to try it

{{TRY}}

## Performance

{{PERFORMANCE}}

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

{{DESIGN}}

## Tests

{{TESTS}}

## Follow-ups and known gaps

{{FOLLOWUPS}}

## Clean room

Nothing was copied from `microscope` or any other codebase. Phase correlation follows Kuglin and Hines (1975); the sub-pixel estimate, Foroosh, Zerubia and Berthod (2002); the window, Harris (1978); each written from the papers' equations, as the spec above sets them out.
