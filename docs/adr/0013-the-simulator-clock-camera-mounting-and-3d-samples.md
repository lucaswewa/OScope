# ADR-0013: The simulator: its clock, camera mounting and 3D samples

- Status: Accepted
- Date: 2026-11-12
- Phase: P10

## Context

- **The simulator stands in for the whole instrument until M3** (plan §5.10). The live view, scans, stitching, autofocus and analysis are all built and tested against it. So what the camera sees has to behave like a microscope:
  - focus that blurs with depth;
  - scales that follow the objective;
  - stains that absorb;
  - samples with depth (D23).
- **Fields come from anywhere, at any scale.** A slide at the 50× pixel, 0.048 µm, would be 1.5 million pixels by half a million. It can't be rendered ahead, so samples are rendered on demand, per field.
- **Tests and ground truth need determinism.** The same seed must give the same image, whichever field, tile or thread renders it. Nucleus detection (P46) needs the true nuclei.
- **Time and motion are shared (D30).** The stage, the camera and the lights must agree on what happened when, at real time, faster, or stepped by a test. ADR-0007 left waiting on the clock to the simulator.
- **The camera sits on the stage at some angle.** The camera-to-stage calibration (P32) must recover that mounting, so the simulator needs a true one to check it against.
- **P10 builds the samples, the optics and the image.** P11 builds the clock and the stage, P12 the camera and its mounting, P13–P14 more samples, and P15 the speed. This ADR sets the design they share.

## Decision

**1. Determinism by hashing.**
- **Every random choice is a hash** of the seed, a salt for its purpose, and the integer cell it belongs to, using SplitMix64's finaliser. Nothing draws from a generator. So any pixel's value depends only on where it is, and on the pixel size.
- **Value noise** blends hashed lattice values with Perlin's quintic fade. Fractal noise sums octaves of it.
- **Rendering is parallel by rows,** and every pixel is computed alone. So a field equals the same pixels of a larger field, and a field rendered twice is the same to the bit. Tests check both.

**2. Samples are layers.** A `Sample` has `Layer`s, each with:
- its height above the slide's surface and its thickness;
- its kind:
  - **absorbing:** it renders as optical densities in R, G and B;
  - **reflective:** it renders as reflectances.

`render(layer, origin, pixel, width, height)` gives a layer over any grid of square pixels, in slide coordinates (µm from the slide's corner).

**3. Levels of detail.** A pixel should hold its square's mean, so that a coarse rendering matches a fine one averaged down.
- **Exact shapes** (the test target's squares and bars) integrate exactly.
- **Procedural shapes** are evaluated at the pixel's centre, with their edges anti-aliased by a box filter across the edge.
- **Noise octaves shorter than half a pixel** fade to their mean, in from a whole pixel. Measured against fine renderings averaged down, that came closer than fading earlier or not at all (P10's notes).
- **The tissue:** a 4× coarser rendering agrees with the averaged fine one to 0.5–0.7%.

**4. Optics.**
- **The pixel at the sample** is the sensor's pixel over the magnification, which scales with the tube lens (ADR-0012).
- **The point spread function** is a Gaussian of standard deviation √((0.21 λ / NA)² + (|dz| tan θ / 2)²):
  - diffraction's Gaussian approximation (Zhang, Zerubia and Olivo-Marin, 2007);
  - in quadrature with the geometric disc that a cone of half-angle θ = asin(NA / n) spreads a point over, at |dz| from focus.
- **The depth of field** is λ n / NA² + n e / (M NA) (Berek, as in Inoué and Spring).
- **Defaults:** λ = 0.55 µm, and n = 1 for dry objectives.

**5. Image formation.** Each layer is blurred by its own distance from the focal plane. The result is linear RGB, with 1 as unhindered light; the sensor (P12) turns it into raw counts.
- **Transmitted light:**
  - the product of each absorbing layer's blurred 10^−OD (Beer–Lambert);
  - and of each reflective layer's blurred 1 − R;
  - times the glass: 96% per surface (Fresnel, for n = 1.5), two surfaces for the slide and two for the coverslip, with dark bands at their edges.
- **Reflected light:** the glass's 4%, plus each reflective layer's blurred reflectance, dimmed twice by the absorbing layers above it.
- **Thin layers blurred one at a time** is the incoherent, thin-layer approximation. It suits 4 µm sections, and the reflective surfaces of P13.

**6. The world.**
- **A slide** is 75 × 25 mm, with a 50 × 24 mm coverslip past the label. Its corner sits at a set position on the stage.
- **The focus** is the stage z at which the slide's surface is sharp.
- **The H&E tissue** lies in islands under the coverslip, with glass between them, so scans meet background. It is a 4 µm section in two half-layers, so nuclei come into focus at different depths at 50×. It reports its nuclei as ground truth.
- **P10 renders fields aligned with the stage's axes.** The camera's mounting (point 8) is P12's.

**7. The simulation clock** (built in P11).
- **One `SimClock` implements `Clock`** (ADR-0007). It is OScope's clock whenever the simulator is in use.
- **Its rates:**
  - real time, 1×, following the host's monotonic clock: the default;
  - scaled by k;
  - stepped: it moves only when a test advances it.
- **Waiting on it:** `sleep_until(t)` and timeouts in simulated time.
  - At 1× and scaled, a waiter sleeps on the host's timer for (t − now) / k, and is re-armed if the rate changes.
  - Stepped, waiters are kept in time order and woken as `advance` passes them.
- **Device state is a function of the clock:** the stage's position, a move's progress, the camera's next frame. It never depends on how often it is read.

**8. The camera's mounting** (built in P12).
- **The simulator's configuration holds the true mounting:**
  - a rotation θ;
  - whether the image is mirrored;
  - the offset of the sensor's centre from the stage position.
- **Sensor pixel (u, v)** sees the stage point:

  P + offset + R(θ) · F · ((u + ½ − W/2) p, (v + ½ − H/2) p)

  - P is the stage position;
  - F is the mirror;
  - p is the pixel at the sample.
- **P12 renders an axis-aligned field** covering the sensor's rotated footprint, plus a margin, and resamples it into the sensor's grid.
  - A moving stage blurs the image: P12 averages renderings over the exposure.
  - The rolling shutter is honoured: each row is placed at its own time.
- **P32's calibration has to recover the mounting,** and tests check it against this truth.

**9. Speed.** P10's renderer is direct, on the CPU, with rayon.
- **Measured:** 60–100 ms for a 768 × 512 field on the owner's PC.
- **Full resolution:** P15 adds cached, pre-blurred levels of detail per z bucket, and a CUDA path. The budget is ≥ 14.5 fps at full resolution with CUDA, and ≥ 5 fps on the CPU (§5.20).

## Consequences

- **Gains:**
  - Any field, at any scale and focus, renders on demand, the same every time. Tests can tile it, repeat it, and compare scales.
  - Blur, scale and stain colour follow published optics, so autofocus, stitching and stain analysis meet realistic images.
  - Samples are pluggable: P13's surface and cell volume, P14's structures, and imported images are further `Sample`s.
  - The clock and the mounting are designed once, for P11, P12 and P32.
- **Costs:**
  - Procedural samples are only as realistic as their rules. H&E's look is plausible, not a copy of real tissue.
  - The Gaussian PSF ignores the true PSF's rings and asymmetry, and spherical aberration through the coverslip.
  - The levels of detail aren't exact for procedural shapes. Nuclei smaller than a pixel are hit or missed at very coarse scales, about 16–18% off at 64 times the fine pixel.
  - The CPU renderer is too slow for full resolution at the frame rate, until P15.
- **To watch:**
  - P15's cached levels must agree with these renderings.
  - The sensor model (P12) owns noise, the Bayer mosaic and quantisation; P10's images have none.

## Alternatives considered

- **A seeded generator,** drawing values in order: a region would then depend on what was rendered before it, and tiles wouldn't match.
- **Pre-rendered slide images:** far too large at high magnification, and fixed to one focus.
- **Wave-optical PSFs** (such as Gibson and Lanni's): more exact, much slower, and more than autofocus and stitching need. A Sample or a renderer can add them later.
- **Blurring optical density, not transmittance:** wrong for absorption. An out-of-focus dark spot spreads its light, not its density.
- **Supersampling each pixel** for exact means: four to sixteen times the cost at every scale. Anti-aliased edges and faded octaves come within 1% at 4×.
- **Fading octaves from two or four pixels a wavelength,** as procedural texturing often does: measured further from the averaged fine rendering (2.0% at 4×, against 0.5–0.7%).
- **A simulation clock separate from OScope's:** timestamps, position tracks and timeouts would then disagree at any rate other than 1×.
