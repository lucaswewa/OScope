//! The sub-diffraction structures: their blur against the optics' transfer
//! function, their fine truth, the amount of dye at any pixel, and their
//! determinism.

use std::f64::consts::PI;
use std::sync::Arc;

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Dye, Excitation, Field, Light, Optics, Sample, Slide, Structures, World};

const FOCUS: f64 = 1_000.0;
const BOUNDS: Rect = Rect::new(20_000.0, 5_000.0, 2_000.0, 2_000.0);

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

/// The power of a square image's channel `c` in rings of spatial
/// frequency, after a Hann window: for each ring centre in cycles a µm,
/// the summed |F|² of the frequencies within 0.1 of it. A plain 2D
/// discrete Fourier transform, rows then columns.
fn ring_power(image: &Image, c: usize, pixel: f64, rings: &[f64]) -> Vec<f64> {
    let n = image.width() as usize;
    let hann = |i: usize| 0.5 - 0.5 * (2.0 * PI * (i as f64 + 0.5) / n as f64).cos();
    let mut re: Vec<f64> = (0..n * n)
        .map(|i| f64::from(image.data()[i * 3 + c]) * hann(i % n) * hann(i / n))
        .collect();
    let mut im = vec![0.0; n * n];
    let twiddle: Vec<(f64, f64)> = (0..n)
        .map(|k| (-2.0 * PI * k as f64 / n as f64).sin_cos())
        .collect();
    // Along rows, then along columns.
    for stride in [(1, n), (n, 1)] {
        let (step, line) = stride;
        let (mut out_re, mut out_im) = (vec![0.0; n * n], vec![0.0; n * n]);
        for l in 0..n {
            for k in 0..n {
                let (mut sr, mut si) = (0.0, 0.0);
                for x in 0..n {
                    let (s, c) = twiddle[(k * x) % n];
                    let at = l * line + x * step;
                    sr += re[at] * c - im[at] * s;
                    si += re[at] * s + im[at] * c;
                }
                out_re[l * line + k * step] = sr;
                out_im[l * line + k * step] = si;
            }
        }
        re = out_re;
        im = out_im;
    }
    let frequency = |k: usize| {
        let k = if k > n / 2 {
            k as f64 - n as f64
        } else {
            k as f64
        };
        k / (n as f64 * pixel)
    };
    rings
        .iter()
        .map(|ring| {
            (0..n * n)
                .filter(|i| ((frequency(i % n)).hypot(frequency(i / n)) - ring).abs() < 0.1)
                .map(|i| re[i] * re[i] + im[i] * im[i])
                .sum()
        })
        .collect()
}

#[test]
fn sub_diffraction_detail_is_blurred_as_the_optics_predict() {
    // In focus at 50×, the filaments' image against their own amounts at
    // the same pixels: in each ring of spatial frequency, the ratio of
    // their spectra is the optics' transfer function, exp(−2π² σ² f²) for
    // the Gaussian of σ = 0.21 λ / NA at the green dye's emission.
    let structures = Structures::new(5, BOUNDS);
    let o = optics("50x");
    let p = o.pixel_size().0;
    let n = 256_u32;
    let centre = Point2::new(21_000.0, 6_000.0);
    let origin = Point2::new(
        centre.x - f64::from(n) * p / 2.0,
        centre.y - f64::from(n) * p / 2.0,
    );
    let object = structures.render(0, origin, Um(p), n, n);
    let world = World::with_slide(
        Slide::new(Arc::new(structures), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let field = Field {
        centre,
        z: Um(FOCUS + Structures::HEIGHT.0),
        width: n,
        height: n,
    };
    let light = Light::Fluorescence {
        line: Excitation::Blue,
        confocal: false,
    };
    let image = world.render(&field, &o, light);
    assert!(
        object.data().iter().any(|v| *v > 0.0),
        "no filament in the field"
    );
    let rings = [0.5, 1.0, 1.5, 2.0];
    let (seen, there) = (
        ring_power(&image, 1, p, &rings),
        ring_power(&object, 0, p, &rings),
    );
    let gain = Dye::Green.colour()[1] * Dye::Green.excited_by(Excitation::Blue);
    let sigma = 0.21 * Dye::Green.peaks().1.0 / o.na;
    for (k, f) in rings.iter().enumerate() {
        let measured = (seen[k] / there[k]).sqrt() / gain;
        let predicted = (-2.0 * PI * PI * sigma * sigma * f * f).exp();
        assert!(
            (measured - predicted).abs() < 0.015,
            "{f} cycles a µm: {measured} against {predicted}"
        );
    }
}

/// The truth along the line through `at` across `direction`, from −`half`
/// to `half` µm in steps of `step`, in channel `c`.
fn across(
    structures: &Structures,
    at: Point2,
    direction: (f64, f64),
    half: f64,
    step: f64,
    c: usize,
) -> Vec<f64> {
    let n = (2.0 * half / step).round() as i64;
    let (nx, ny) = (-direction.1, direction.0);
    (0..=n)
        .map(|k| {
            let d = -half + k as f64 * step;
            let point = Point2::new(at.x + d * nx - step / 2.0, at.y + d * ny - step / 2.0);
            f64::from(structures.truth(point, Um(step), 1, 1).data()[c])
        })
        .collect()
}

/// The middle of a structure's middle piece, and the piece's direction.
fn middle(points: &[Point2]) -> (Point2, (f64, f64)) {
    let k = points.len() / 2;
    let (a, b) = (points[k - 1], points[k]);
    let length = (b.x - a.x).hypot(b.y - a.y);
    (
        Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        ((b.x - a.x) / length, (b.y - a.y) / length),
    )
}

#[test]
fn the_truth_shows_what_the_optics_cannot() {
    let structures = Structures::new(5, BOUNDS);
    let region = Rect::new(20_900.0, 5_900.0, 200.0, 200.0);
    // A filament, across it at 2.5 nm: there over 25 nm only.
    let filament = structures
        .filaments(region)
        .into_iter()
        .find(|f| region.contains(f.points[f.points.len() / 2]))
        .unwrap();
    let (at, direction) = middle(&filament.points);
    let profile = across(&structures, at, direction, 0.05, 0.0025, 0);
    let width = profile.iter().filter(|v| **v > 0.0).count() as f64 * 0.0025;
    assert!((width - 0.025).abs() <= 0.005, "{width} µm");
    // A tubule, across it at 5 nm: its wall brighter than its hollow middle.
    let tubule = structures
        .tubules(region)
        .into_iter()
        .find(|t| region.contains(t.points[1]))
        .unwrap();
    let (at, direction) = middle(&tubule.points);
    let profile = across(&structures, at, direction, 0.25, 0.005, 1);
    let centre = profile[profile.len() / 2];
    let wall = profile.iter().copied().fold(0.0, f64::max);
    assert!(
        centre > 0.0 && centre < 0.5 * wall,
        "{centre} at the axis, {wall} at the wall"
    );
    // Through the 50×, the hollow is gone: the tubule is one blur, brightest
    // near its axis.
    let o = optics("50x");
    let world = World::with_slide(
        Slide::new(Arc::new(structures), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let field = Field {
        centre: at,
        z: Um(FOCUS + Structures::HEIGHT.0),
        width: 33,
        height: 33,
    };
    let light = Light::Fluorescence {
        line: Excitation::Green,
        confocal: false,
    };
    let image = world.render(&field, &o, light);
    let red = |x: usize, y: usize| f64::from(image.data()[(y * 33 + x) * 3]);
    let peak = (0..33 * 33)
        .map(|i| red(i % 33, i / 33))
        .fold(0.0, f64::max);
    assert!(
        red(16, 16) > 0.8 * peak,
        "{} at the axis, {peak} at the brightest",
        red(16, 16)
    );
}

#[test]
fn a_coarse_rendering_holds_as_much_dye_as_a_fine_one() {
    // Over 40 µm, each kind's amount at 0.02 µm pixels against 0.25 and
    // 1 µm pixels, where every structure is widened: within 2.5%, the
    // little that spills over the square's edges and bends included.
    let structures = Structures::new(5, BOUNDS);
    let origin = Point2::new(21_000.0, 6_000.0);
    for kind in 0..2 {
        let total = |p: f64| {
            let n = (40.0 / p).round() as u32;
            let image = structures.render(kind, origin, Um(p), n, n);
            image
                .data()
                .iter()
                .step_by(3)
                .map(|v| f64::from(*v))
                .sum::<f64>()
                * p
                * p
        };
        let fine = total(0.02);
        for p in [0.25, 1.0] {
            let coarse = total(p);
            assert!(
                (coarse / fine - 1.0).abs() < 0.025,
                "kind {kind} at {p} µm: {coarse} against {fine}"
            );
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_structures() {
    let render = |seed: u64| {
        Structures::new(seed, BOUNDS)
            .render(1, Point2::new(21_000.0, 6_000.0), Um(0.05), 128, 128)
            .data()
            .to_vec()
    };
    let first = render(5);
    assert!(first.iter().any(|v| *v > 0.0));
    assert_eq!(first, render(5));
    assert_ne!(first, render(6));
}
