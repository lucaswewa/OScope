//! Deconvolution (P43): Born and Wolf's PSF against its closed forms and
//! J₀ against its table; Richardson–Lucy on an object blurred by that PSF,
//! with noise, nearer the object at every check, its energy kept, and total
//! variation no worse; the NRMSE's definition.

use oscope_volume::deconvolve::{DeconvolveOptions, Padded, Volume, blur, nrmse, richardson_lucy};
use oscope_volume::psf::{Optics, bessel_j0, born_wolf, psf};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const OPTICS: Optics = Optics {
    na: 0.55,
    wavelength_um: 0.519,
    medium: 1.0,
};

#[test]
fn j0_is_its_table_and_the_psf_its_closed_forms() {
    // Abramowitz and Stegun's table 9.1, and the first zero.
    for (x, j0) in [
        (0.0, 1.0),
        (1.0, 0.765_197_686_6),
        (2.0, 0.223_890_779_1),
        (3.0, -0.260_051_954_9),
        (5.0, -0.177_596_771_3),
        (10.0, -0.245_935_764_5),
        (2.404_825_557_695_773, 0.0),
    ] {
        assert!(
            (bessel_j0(x) - j0).abs() < 1e-7,
            "J0({x}) = {}",
            bessel_j0(x)
        );
    }
    // At focus 1; on the axis (sin(u/4)/(u/4))²; in focus, Airy's pattern,
    // its first zero at v = 3.8317.
    assert!((born_wolf(0.0, 0.0) - 1.0).abs() < 1e-9);
    for u in [1.0_f64, 5.0, 10.0, 20.0, 40.0] {
        let want = ((u / 4.0).sin() / (u / 4.0)).powi(2);
        assert!((born_wolf(u, 0.0) - want).abs() < 1e-3, "u = {u}");
    }
    // Far from focus, where the phase turns many times over the pupil:
    // within 1 % of it.
    for u in [80.0_f64, 100.0] {
        let want = ((u / 4.0).sin() / (u / 4.0)).powi(2);
        assert!((born_wolf(u, 0.0) - want).abs() < 1e-2 * want, "u = {u}");
    }
    assert!(born_wolf(0.0, 3.8317) < 1e-6 && born_wolf(0.0, 3.0) > 1e-3);

    // Sampled: summing to 1, at its largest at the centre, symmetric in z
    // and in x and y; its focal plane's first minimum at Airy's radius.
    let voxel = [0.1, 0.1, 0.25];
    let p = psf(&OPTICS, voxel, [80, 80, 9]).unwrap();
    // Over the depth either way; laterally, the cone's radius 2 µm up,
    // 2 × 0.55 / √(1 − 0.55²) = 1.32 µm, and three Airy radii, 1.73 µm: 31
    // voxels either way.
    assert_eq!(p.size, [63, 63, 17]);
    let [nx, ny, nz] = p.size;
    let sum: f64 = p.data.iter().map(|v| f64::from(*v)).sum();
    assert!((sum - 1.0).abs() < 1e-5);
    let at = |x: usize, y: usize, z: usize| p.data[(z * ny + y) * nx + x];
    let (cx, cy, cz) = (nx / 2, ny / 2, nz / 2);
    let peak = p.data.iter().copied().fold(0.0_f32, f32::max);
    assert_eq!(at(cx, cy, cz), peak);
    for (x, y, z) in [
        (cx + 3, cy, cz + 2),
        (cx + 1, cy + 4, cz + 5),
        (cx, cy + 2, cz + 8),
    ] {
        assert_eq!(at(x, y, z), at(x, y, 2 * cz - z));
        assert!((at(x, y, z) - at(y + cx - cy, x + cy - cx, z)).abs() <= 1e-6 * peak);
    }
    // Its axis, against the closed form at u = (2π/λ)(NA²/n) z, dry and in
    // water.
    let k = 2.0 * std::f64::consts::PI / OPTICS.wavelength_um;
    let water = Optics {
        na: 1.0,
        medium: 1.33,
        ..OPTICS
    };
    for optics in [OPTICS, water] {
        let q = psf(&optics, voxel, [8, 8, 9]).unwrap();
        let [qx, qy, qz] = q.size;
        let axis = |dz: usize| f64::from(q.data[((qz / 2 + dz) * qy + qy / 2) * qx + qx / 2]);
        for dz in [2, 4, 6] {
            let u = k * optics.na * optics.na / optics.medium * dz as f64 * voxel[2];
            let want = ((u / 4.0).sin() / (u / 4.0)).powi(2);
            let got = axis(dz) / axis(0);
            assert!(
                (got - want).abs() < 1e-3,
                "{optics:?}, {dz} planes up: {got} not {want}"
            );
        }
    }
    // Off the axis, between the radial table's samples: Born and Wolf's
    // intensity at the voxel's own distance.
    let u = k * OPTICS.na * OPTICS.na * 2.0 * voxel[2];
    let v = k * OPTICS.na * voxel[0].hypot(voxel[1]);
    let got = f64::from(at(cx + 1, cy + 1, cz + 2) / at(cx, cy, cz));
    assert!(
        (got - born_wolf(u, v)).abs() < 3e-3,
        "{got} not {}",
        born_wolf(u, v)
    );
    let first_minimum = (cx..nx - 1)
        .find(|&x| at(x + 1, cy, cz) > at(x, cy, cz))
        .unwrap();
    let radius = (first_minimum - cx) as f64 * voxel[0];
    assert!(
        (radius - OPTICS.airy_radius()).abs() <= voxel[0],
        "{radius} µm"
    );
    // Refused: an NA at the medium's index, voxels of no size.
    let wide = Optics { na: 1.0, ..OPTICS };
    assert!(psf(&wide, voxel, [8, 8, 3]).is_err());
    assert!(psf(&OPTICS, [0.0, 0.1, 0.25], [8, 8, 3]).is_err());
}

/// A cell-like object of 64 × 48 × 16 voxels of 0.2 × 0.2 × 0.4 µm: three
/// bright beads and a thin shell; and its blur by the PSF, with noise.
fn object(noise: f32) -> (Volume, Volume, Volume) {
    let size = [64, 48, 16];
    let [w, h, d] = size;
    let mut data = vec![0.0_f32; w * h * d];
    // Beads two voxels in radius.
    for (bx, by, bz) in [(20, 20, 7), (40, 28, 9), (30, 12, 5)] {
        for z in 0..d {
            for y in 0..h {
                for x in 0..w {
                    let r2 =
                        (x as i32 - bx).pow(2) + (y as i32 - by).pow(2) + (z as i32 - bz).pow(2);
                    if r2 <= 4 {
                        data[(z * h + y) * w + x] = 1.0;
                    }
                }
            }
        }
    }
    for z in 0..d {
        for y in 0..h {
            for x in 0..w {
                let r = ((x as f32 - 32.0) / 18.0)
                    .hypot((y as f32 - 24.0) / 12.0)
                    .hypot((z as f32 - 6.0) / 4.0);
                if (r - 1.0).abs() < 0.08 {
                    data[(z * h + y) * w + x] += 0.2;
                }
            }
        }
    }
    let truth = Volume { size, data };
    let kernel = psf(&OPTICS, [0.2, 0.2, 0.4], size).unwrap();
    let mut blurred = blur(&truth, &kernel).unwrap();
    let mut random = StdRng::seed_from_u64(43);
    let top = blurred.data.iter().copied().fold(0.0_f32, f32::max);
    for v in &mut blurred.data {
        *v = (*v + noise * top * random.random_range(-1.0..1.0)).max(0.0);
    }
    (truth, kernel, blurred)
}

/// `psf` moved a voxel along x, each row turned: off its centre, as a
/// measured PSF may be.
fn skewed(psf: &Volume) -> Volume {
    let mut skewed = psf.clone();
    skewed
        .data
        .chunks_mut(psf.size[0])
        .for_each(|row| row.rotate_right(1));
    skewed
}

fn deconvolved(data: &Volume, kernel: &Volume, iterations: u32, tv: f32) -> Volume {
    let padded = Padded::new(data, kernel).unwrap();
    let options = DeconvolveOptions { iterations, tv };
    padded.crop(&richardson_lucy(&padded, &options, &mut |_| true).unwrap())
}

#[test]
fn richardson_lucy_nears_the_object_keeps_its_energy_and_total_variation_is_no_worse() {
    let (truth, kernel, data) = object(0.002);
    let mut last = nrmse(&data.data, &truth.data);
    let first = last;
    for iterations in [1, 5, 10, 20] {
        let error = nrmse(
            &deconvolved(&data, &kernel, iterations, 0.0).data,
            &truth.data,
        );
        eprintln!("{iterations} iterations: NRMSE {error:.4}");
        assert!(error < last, "{iterations}: {error} after {last}");
        last = error;
    }
    assert!(last < 0.8 * first, "{first} to {last}");

    // Energy, over the padded volume, at every iteration; with the PSF off
    // its centre too, for which the correction is the ratio correlated with
    // it, not convolved.
    for psf in [&kernel, &skewed(&kernel)] {
        let padded = Padded::new(&data, psf).unwrap();
        let total: f64 = padded.data.iter().map(|v| f64::from(*v)).sum();
        for iterations in [1, 3, 10] {
            let options = DeconvolveOptions {
                iterations,
                tv: 0.0,
            };
            let u = richardson_lucy(&padded, &options, &mut |_| true).unwrap();
            let kept: f64 = u.iter().map(|v| f64::from(*v)).sum();
            assert!(
                ((kept - total) / total).abs() < 1e-5,
                "{iterations}: {kept} of {total}"
            );
        }
    }

    // On noisier data, over more iterations, total variation does better.
    let (truth, kernel, noisy) = object(0.05);
    let plain = nrmse(&deconvolved(&noisy, &kernel, 40, 0.0).data, &truth.data);
    let smoothed = nrmse(&deconvolved(&noisy, &kernel, 40, 0.002).data, &truth.data);
    eprintln!("noisy: NRMSE {plain:.4} plain, {smoothed:.4} with total variation");
    assert!(smoothed < plain, "{smoothed} against {plain}");

    // A cancel at the third iteration stops it, told of each from the first.
    let padded = Padded::new(&data, &kernel).unwrap();
    let mut told = Vec::new();
    let stopped = richardson_lucy(&padded, &DeconvolveOptions::default(), &mut |k| {
        told.push(k);
        k < 3
    });
    assert!(stopped.is_none() && told == [1, 2, 3], "{told:?}");
}

#[test]
fn volumes_pad_and_crop_and_the_nrmse_ignores_units() {
    let (_, kernel, data) = object(0.0);
    let padded = Padded::new(&data, &kernel).unwrap();
    assert_eq!(padded.crop(&padded.data), data);
    // The data clamped at 0.
    let mut below = data.clone();
    below.data[0] = -1.0;
    let clamped = Padded::new(&below, &kernel).unwrap();
    assert!(clamped.data.iter().all(|v| *v >= 0.0));
    // Mirrored about each edge: the voxel before the first is the first, the
    // one after the last the last.
    let [px, py, _] = padded.size;
    let [ox, oy, oz] = padded.offset;
    let [w, h, _] = data.size;
    let at = |x: usize, y: usize, z: usize| padded.data[(z * py + y) * px + x];
    let row = 7 * w;
    assert_eq!(at(ox - 1, oy + 7, oz), data.data[row]);
    assert_eq!(at(ox + w, oy + 7, oz), data.data[row + w - 1]);
    assert_eq!(at(ox - 2, oy + 7, oz), data.data[row + 1]);
    // A point blurred stays where it is: the PSF is centred.
    let mut point = Volume {
        size: data.size,
        data: vec![0.0; data.data.len()],
    };
    let centre = (8 * h + 24) * w + 32;
    point.data[centre] = 1.0;
    let blurred = blur(&point, &kernel).unwrap();
    let brightest = (0..blurred.data.len())
        .max_by(|a, b| blurred.data[*a].total_cmp(&blurred.data[*b]))
        .unwrap();
    assert_eq!(brightest, centre);
    assert!(padded.size.iter().all(|n| {
        let mut m = *n;
        for p in [2, 3, 5, 7] {
            while m.is_multiple_of(p) {
                m /= p;
            }
        }
        m == 1
    }));
    // Planes and back.
    assert_eq!(Volume::from_planes(&data.planes()).unwrap(), data);
    // An even PSF is refused.
    let even = Volume {
        size: [4, 3, 3],
        data: vec![1.0 / 36.0; 36],
    };
    assert!(Padded::new(&data, &even).is_err());
    // The NRMSE: 0 for the same volume scaled and offset; 2 for its
    // negative.
    let scaled: Vec<f32> = data.data.iter().map(|v| 2.0 * v + 0.01).collect();
    assert!(nrmse(&scaled, &data.data) < 1e-3);
    let negative: Vec<f32> = data.data.iter().map(|v| -v).collect();
    assert!((nrmse(&negative, &data.data) - 2.0).abs() < 1e-6);
}
