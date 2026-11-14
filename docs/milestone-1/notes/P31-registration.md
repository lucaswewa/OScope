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
  - **the window:** the Hann window, which may be turned off.

### The method: phase correlation (Kuglin and Hines, 1975)

1. **Mean and window.** From each image its mean is subtracted, then it is multiplied by the separable Hann window, w(x, y) = h(x) · h(y), with h(n) = ½ (1 − cos(2πn / (N − 1))) for n = 0 … N − 1 along an axis of N pixels (Harris, 1978). The window tapers each image to zero at its edges, so the discontinuity where the discrete Fourier transform wraps an image round doesn't dominate the spectrum.
2. **Transforms.** F_A = F(a) and F_B = F(b), the two-dimensional discrete Fourier transforms, at the images' own size, without padding.
3. **The normalised cross-power spectrum.** R = F_B · conj(F_A) / |F_B · conj(F_A)|. Where the magnitude is below 10⁻³⁰, R is 0. Only the phase remains, so every frequency counts alike, whatever the image's contrast.
4. **The correlation surface.** r = F⁻¹(R), whose real part is kept, scaled by 1 / (W · H), so that a perfect match peaks at 1. If B is A shifted by d, the shift theorem makes R = e^(−2πi(k·d)/N), and r a Dirac impulse at d: its peak's place is the shift.
5. **The integer peak.** The largest value of r, among the shifts allowed: a surface index (x, y) means a shift of x, or of x − W for x ≥ W/2, and likewise for y. So shifts run from −W/2 to W/2 − 1.

### The sub-pixel peak (Foroosh, Zerubia and Berthod, 2002)

The surface around a non-integer shift is close to a sampled two-dimensional sinc, the Dirichlet kernel. Along each axis, with c₀ the peak's value and c₁ the larger of its two neighbours, in direction s = ±1, the shift is refined by:

```text
δ = s · c₁ / (c₁ + c₀)
```

That is their estimate's root of magnitude below one, toward the larger neighbour. It is taken along x, then along y, each from the peak's row or column. If c₁ ≤ 0, there is no refinement along that axis.

### Confidence: the peak ratio

- **c₀** is the peak's value. Phase correlation's Dirac peak has a height near the fraction of the images that overlap; noise gives a surface of many small peaks.
- **c₂** is the largest value of r outside the 5 × 5 neighbourhood of the integer peak, among the shifts considered: the whole surface without an expected shift.
- **confidence = clamp(1 − max(c₂, 0) / c₀, 0, 1)**: near 1 when one peak stands out, near 0 when the best peak is barely above the rest, or c₀ ≤ 0.

A registration reports **peak** = c₀ too. A caller decides on a threshold: the tests set what a good pair and a poor one score.

### The batch

`register_batch` registers many pairs of the same size with the same options, in order, and gives each its registration as if alone. On the CPU, pairs run in parallel. On CUDA, the pairs' transforms are one batched plan.

### The backends

- **The CPU,** with `rustfft`'s transforms, is the reference.
- **CUDA,** with cuFFT's, computes the same steps on the GPU, then gathers and brings back only the shifts considered, and a pixel around them: the search window, or the whole surface without an expected shift. The CPU gathers the same region from its surface. The peak, its refinement and the confidence are then found from the region by the same code for both.
- **Equivalence:** for the same pair, the two agree within 0.01 px on the shift, and within 0.01 on the confidence.

### What the tests show

- **Known shifts,** integer and sub-pixel, of a textured field with noise added to both images: each recovered within **0.1 px**.
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
- H. Foroosh, J. B. Zerubia and M. Berthod, "Extension of phase correlation to subpixel registration", *IEEE Transactions on Image Processing* 11(3), 2002, pp. 188–200. doi:10.1109/83.988953.
- F. J. Harris, "On the use of windows for harmonic analysis with the discrete Fourier transform", *Proceedings of the IEEE* 66(1), 1978, pp. 51–83. doi:10.1109/PROC.1978.10837.

## What was built

{{BUILT}}

## How to try it

{{TRY}}

## Performance

{{PERFORMANCE}}

## Design notes and deviations from the plan

{{DESIGN}}

## Tests

{{TESTS}}

## Follow-ups and known gaps

{{FOLLOWUPS}}

## Clean room

Nothing was copied from `microscope` or any other codebase. Phase correlation follows Kuglin and Hines (1975); the sub-pixel estimate, Foroosh, Zerubia and Berthod (2002); the window, Harris (1978); each written from the papers' equations, as the spec above sets them out.
