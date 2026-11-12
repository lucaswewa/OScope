//! A tour of `oscope-core`: units, an affine map fitted to noisy point pairs,
//! and the clock. With `--smoke`, it also checks what it prints.

use std::process::ExitCode;
use std::time::Duration;

use oscope_core::{Affine2, Clock, FakeClock, Ms, Point2, SystemClock, Um};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn main() -> ExitCode {
    let smoke = std::env::args().any(|a| a == "--smoke");
    let mut failures = Vec::new();
    let mut check = |ok: bool, what: &str| {
        if !ok {
            failures.push(what.to_owned());
        }
    };

    println!("Units");
    let travel = Um::from_mm(25.0);
    let pixel = Um::from_nm(240.0);
    println!(
        "  25 mm of travel is {travel}: {:.0} pixels of {pixel}, the 10x objective's",
        travel / pixel
    );
    let exposure = Ms::from(Duration::from_micros(2_500));
    println!(
        "  an exposure of {exposure} is {:?}",
        exposure.to_duration().unwrap()
    );
    check(
        travel == Um(25_000.0) && exposure == Ms(2.5),
        "unit conversions",
    );

    println!("\nA camera-to-stage map, fitted to noisy point pairs");
    // The truth: 0.24 µm per pixel, the image's y flipped against the stage's,
    // turned by 0.5°, with the sensor's corner at x = 12.5 mm, y = 8 mm.
    let truth = Affine2::scale(0.24, -0.24)
        .then(&Affine2::rotation(0.5_f64.to_radians()))
        .then(&Affine2::translation(12_500.0, 8_000.0));
    // 24 features on a 6 × 4 grid over the 5472 × 3648 sensor, their stage
    // positions measured to ±0.05 µm.
    let mut rng = StdRng::seed_from_u64(2026);
    let pairs: Vec<(Point2, Point2)> = (0..24)
        .map(|i| {
            let pixel = Point2::new(
                456.0 + 912.0 * (i % 6) as f64,
                456.0 + 912.0 * (i / 6) as f64,
            );
            let noise = Point2::new(rng.random_range(-0.05..0.05), rng.random_range(-0.05..0.05));
            (pixel, truth.apply(pixel) + noise)
        })
        .collect();
    let fitted = Affine2::fit(&pairs).expect("24 points on a grid fix a map");
    let size = fitted.determinant().abs().sqrt();
    let angle = fitted.m[1][0].atan2(fitted.m[0][0]).to_degrees();
    let corner = Point2::new(5472.0, 3648.0);
    let off = fitted.apply(corner).distance(truth.apply(corner));
    println!(
        "  pixel size {size:.5} µm, turned {angle:.3}°, flipped: {}",
        fitted.determinant() < 0.0
    );
    println!(
        "  residual {:.3} µm rms; the far corner {off:.3} µm from the truth",
        fitted.rms_error(&pairs)
    );
    let back = fitted
        .inverse()
        .unwrap()
        .apply(Point2::new(12_500.0, 8_000.0));
    println!(
        "  the sensor's corner, at stage (12500, 8000) µm, maps back to pixel ({:.2}, {:.2})",
        back.x, back.y
    );
    check(
        (size - 0.24).abs() < 1e-4 && (angle - 0.5).abs() < 0.01 && off < 0.1,
        "the fit",
    );

    println!("\nThe clock");
    let clock = SystemClock::new();
    let now = clock.now();
    println!(
        "  started at {}; now is {now} later, {}",
        clock.start_utc(),
        clock.utc(now)
    );
    // A frame exposed for 2.5 ms, 69 ms after start, on a clock that moves only when told.
    let fake = FakeClock::new(clock.start_utc());
    fake.advance(Duration::from_millis(69));
    let start = fake.now();
    fake.advance(exposure.to_duration().unwrap());
    let midpoint = start.midpoint(fake.now());
    println!(
        "  an exposure from {start} to {} has its midpoint at {midpoint}",
        fake.now()
    );
    check(midpoint.0 == 70_250_000 && clock.now() >= now, "the clock");

    if smoke {
        if !failures.is_empty() {
            eprintln!("\nsmoke: failed: {}", failures.join(", "));
            return ExitCode::FAILURE;
        }
        println!("\nsmoke: ok");
    }
    ExitCode::SUCCESS
}
