# P43: Deconvolution and volume rendering

- Status: In review
- Branch / PR: `m1/p43-deconvolution` / [#58](https://github.com/lucaswewa/OScope/pull/58)
- Size: L · estimated ~3,000 before it started (in `oscope-volume`, the Born–Wolf widefield PSF with its Bessel function and integral ~150, Richardson–Lucy on rustfft's 3D FFTs with mirrored padding, total-variation regularisation and its iterations ~280; in `oscope-compute`, its CUDA twin on cuFFT with its kernels ~230; the pipeline's nodes ~150; in `oscope-workflows`, the deconvolution job, from a volume acquisition to a result volume, ~200; tests ~750; `ex-deconvolve` and its VTK.js page with MIP, composite, isosurface and a before-and-after toggle ~500; `ex-checkpoint-e` ~200; glue ~60; and about a tenth on top); one pull request, at the owner's standing choice

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
