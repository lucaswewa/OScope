//! Geometry, and property tests of [`Affine2`] over seeded random cases.

use std::f64::consts::PI;

use oscope_core::{Affine2, Error, Point2, Rect, Um, Xyz};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const CASES: u64 = 500;
/// The camera's sensor, in pixels.
const SENSOR: (f64, f64) = (5472.0, 3648.0);

/// A random map like a camera-to-stage calibration's: about 0.05–2.4 µm per
/// pixel (the 50× to 1× objectives), at any angle, perhaps flipped, a little
/// shear, and anywhere on 100 mm of travel.
fn random_affine(rng: &mut StdRng) -> Affine2 {
    let scale = rng.random_range(0.05..2.4);
    let flip = if rng.random_bool(0.5) { -1.0 } else { 1.0 };
    let shear = Affine2 {
        m: [[1.0, rng.random_range(-0.01..0.01)], [0.0, 1.0]],
        t: [0.0, 0.0],
    };
    Affine2::scale(scale, flip * scale * rng.random_range(0.99..1.01))
        .then(&shear)
        .then(&Affine2::rotation(rng.random_range(-PI..PI)))
        .then(&Affine2::translation(
            rng.random_range(-50_000.0..50_000.0),
            rng.random_range(-50_000.0..50_000.0),
        ))
}

fn random_pixel(rng: &mut StdRng) -> Point2 {
    Point2::new(
        rng.random_range(0.0..SENSOR.0),
        rng.random_range(0.0..SENSOR.1),
    )
}

fn corners() -> [Point2; 4] {
    let (w, h) = SENSOR;
    [
        Point2::new(0.0, 0.0),
        Point2::new(w, 0.0),
        Point2::new(0.0, h),
        Point2::new(w, h),
    ]
}

#[test]
fn a_map_then_its_inverse_returns_every_point() {
    for case in 0..CASES {
        let mut rng = StdRng::seed_from_u64(case);
        let map = random_affine(&mut rng);
        let inverse = map.inverse().unwrap();
        for _ in 0..10 {
            let p = random_pixel(&mut rng);
            let back = inverse.apply(map.apply(p));
            assert!(
                back.distance(p) < 1e-6,
                "case {case}: {p:?} came back as {back:?}"
            );
        }
        let identity = map.then(&inverse);
        for p in corners() {
            assert!(
                identity.apply(p).distance(p) < 1e-6,
                "case {case}: {identity:?}"
            );
        }
    }
}

#[test]
fn then_applies_the_first_map_and_then_the_second() {
    for case in 0..CASES {
        let mut rng = StdRng::seed_from_u64(case);
        let (a, b) = (random_affine(&mut rng), random_affine(&mut rng));
        let p = random_pixel(&mut rng);
        let (together, one_by_one) = (a.then(&b).apply(p), b.apply(a.apply(p)));
        assert!(together.distance(one_by_one) < 1e-6, "case {case}");
    }
}

#[test]
fn a_fit_recovers_a_known_map_from_noisy_pairs() {
    // Stage positions measured to ±0.05 µm on each axis, 25 points across the
    // sensor. The corners are extrapolated from them, so least squares puts
    // them about 0.02 µm out on average; the worst of the 2,000 here is
    // 0.08 µm, a third of a pixel at 10×.
    let noise = 0.05;
    let (mut worst, mut total): (f64, f64) = (0.0, 0.0);
    for case in 0..CASES {
        let mut rng = StdRng::seed_from_u64(case);
        let truth = random_affine(&mut rng);
        let pairs: Vec<_> = (0..25)
            .map(|_| {
                let p = random_pixel(&mut rng);
                let q = truth.apply(p);
                let jitter = Point2::new(
                    rng.random_range(-noise..noise),
                    rng.random_range(-noise..noise),
                );
                (p, q + jitter)
            })
            .collect();
        let fitted = Affine2::fit(&pairs).unwrap();
        for corner in corners() {
            let error = fitted.apply(corner).distance(truth.apply(corner));
            (worst, total) = (worst.max(error), total + error);
        }
        assert!(fitted.rms_error(&pairs) < noise, "case {case}");
    }
    let mean = total / (4 * CASES) as f64;
    assert!(
        mean < 0.6 * noise,
        "the corners were {mean} µm out on average"
    );
    assert!(worst < 2.0 * noise, "the worst corner was {worst} µm out");
}

#[test]
fn a_fit_without_noise_is_exact() {
    let mut rng = StdRng::seed_from_u64(7);
    let truth = random_affine(&mut rng);
    let pairs: Vec<_> = corners().into_iter().map(|p| (p, truth.apply(p))).collect();
    let fitted = Affine2::fit(&pairs).unwrap();
    for (p, q) in &pairs {
        assert!(fitted.apply(*p).distance(*q) < 1e-6);
    }
}

#[test]
fn degenerate_fits_and_inverses_are_errors() {
    let p = |x, y| Point2::new(x, y);
    let two = [(p(0.0, 0.0), p(1.0, 1.0)), (p(1.0, 0.0), p(2.0, 1.0))];
    assert!(matches!(
        Affine2::fit(&two),
        Err(Error::TooFewPoints { needed: 3, got: 2 })
    ));
    let line: Vec<_> = (0..5)
        .map(|i| (p(i as f64, 2.0 * i as f64), p(0.0, i as f64)))
        .collect();
    assert!(matches!(Affine2::fit(&line), Err(Error::Collinear)));
    let flat = Affine2::scale(1.0, 0.0);
    assert!(matches!(flat.inverse(), Err(Error::Singular { .. })));
    assert!(
        Affine2::scale(1e-3, 1e-3).inverse().is_ok(),
        "small, but not singular"
    );
}

#[test]
fn rectangles_contain_their_near_edges_and_overlap() {
    let tile = Rect::new(0.0, 0.0, 512.0, 512.0);
    assert!(tile.contains(Point2::new(0.0, 0.0)));
    assert!(
        !tile.contains(Point2::new(512.0, 10.0)),
        "the far edge belongs to the next tile"
    );
    assert_eq!(tile.centre(), Point2::new(256.0, 256.0));
    let next = Rect::new(400.0, 100.0, 512.0, 512.0);
    assert_eq!(
        tile.intersection(&next),
        Some(Rect::new(400.0, 100.0, 112.0, 412.0))
    );
    assert_eq!(
        tile.intersection(&Rect::new(512.0, 0.0, 10.0, 10.0)),
        None,
        "touching isn't overlapping"
    );
}

#[test]
fn geometry_serialises_as_plain_json() {
    let position = Xyz::new(Um(12_500.0), Um(-3.25), Um::from_mm(1.5));
    let json = serde_json::to_value(position).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"x": 12500.0, "y": -3.25, "z": 1500.0})
    );
    assert_eq!(serde_json::from_value::<Xyz>(json).unwrap(), position);
    let map = Affine2::translation(5.0, -2.0);
    let json = serde_json::to_value(map).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"m": [[1.0, 0.0], [0.0, 1.0]], "t": [5.0, -2.0]})
    );
    assert_eq!(serde_json::from_value::<Affine2>(json).unwrap(), map);
    let rect = Rect::new(1.0, 2.0, 3.0, 4.0);
    let back: Rect = serde_json::from_str(&serde_json::to_string(&rect).unwrap()).unwrap();
    assert_eq!(back, rect);
}
