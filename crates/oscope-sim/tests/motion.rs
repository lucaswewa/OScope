//! Motion segments: d ÷ v without acceleration, d ÷ v + v ÷ a with it,
//! triangular profiles, the path speed of multi-axis moves, and stops.

use std::time::Duration;

use oscope_core::{Timestamp, Um, Xyz};
use oscope_sim::Segment;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn mm(x: f64, y: f64, z: f64) -> Xyz {
    Xyz::new(Um::from_mm(x), Um::from_mm(y), Um::from_mm(z))
}

fn seconds(d: Duration) -> f64 {
    d.as_secs_f64()
}

const START: Timestamp = Timestamp(1_000_000_000);

#[test]
fn without_acceleration_a_move_takes_its_distance_over_its_speed() {
    let s = Segment::new(mm(0.0, 0.0, 0.0), mm(10.0, 0.0, 0.0), START, 5_000.0, None);
    assert_eq!(s.duration(), Duration::from_secs(2));
    assert_eq!(
        s.position(START + Duration::from_secs(1)),
        mm(5.0, 0.0, 0.0)
    );
}

#[test]
fn with_acceleration_it_takes_a_ramp_s_time_more() {
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..1_000 {
        let (d, v, a) = (
            rng.random_range(1.0..100_000.0),
            rng.random_range(100.0..50_000.0),
            rng.random_range(1_000.0..1_000_000.0),
        );
        let s = Segment::new(
            mm(0.0, 0.0, 0.0),
            Xyz::new(Um(d), Um(0.0), Um(0.0)),
            START,
            v,
            Some(a),
        );
        // Long enough to reach v: d / v + v / a. Shorter: up to the middle
        // and back down, in 2 √(d / a).
        let expected = if d >= v * v / a {
            d / v + v / a
        } else {
            2.0 * (d / a).sqrt()
        };
        assert!(
            (seconds(s.duration()) - expected).abs() < 1e-9,
            "{d} {v} {a}"
        );
        assert!((s.to().x.0 - d).abs() < 1e-9);
    }
}

#[test]
fn a_multi_axis_move_keeps_the_path_speed() {
    // 3 mm by 4 mm: a 5 mm path at 5 mm/s.
    let s = Segment::new(
        mm(10.0, 10.0, 5.0),
        mm(13.0, 14.0, 5.0),
        START,
        5_000.0,
        Some(100_000.0),
    );
    assert!((seconds(s.duration()) - (1.0 + 0.05)).abs() < 1e-9);
    let mid = START + Duration::from_millis(500);
    assert!((s.speed(mid) - 5_000.0).abs() < 1e-9);
    let p = s.position(mid);
    // On the straight line between the ends.
    let along = (p.x.0 - 10_000.0) / 3_000.0;
    assert!((along - (p.y.0 - 10_000.0) / 4_000.0).abs() < 1e-12 && p.z.0 == 5_000.0);
}

#[test]
fn time_and_distance_are_each_other_s_inverse() {
    let mut rng = StdRng::seed_from_u64(2);
    for _ in 0..200 {
        let d = rng.random_range(10.0..50_000.0);
        let s = Segment::new(
            mm(0.0, 0.0, 0.0),
            Xyz::new(Um(0.0), Um(d), Um(0.0)),
            START,
            10_000.0,
            Some(100_000.0),
        );
        let mut before = 0.0;
        for k in 0..=100 {
            let at = START + s.duration().mul_f64(f64::from(k) / 100.0);
            let gone = s.travelled(at);
            assert!(gone >= before - 1e-9, "it went back");
            assert!((s.travelled(s.time_at(gone)) - gone).abs() < 1e-3);
            before = gone;
        }
        assert!((before - d).abs() < 1e-9);
    }
}

#[test]
fn a_stop_slows_down_over_v_squared_over_twice_a() {
    let (v, a) = (10_000.0, 100_000.0);
    let s = Segment::stop(mm(1.0, 2.0, 3.0), [1.0, 0.0, 0.0], START, v, Some(a));
    assert!((s.distance() - v * v / (2.0 * a)).abs() < 1e-9);
    assert!((seconds(s.duration()) - v / a).abs() < 1e-9);
    assert!((s.speed(START) - v).abs() < 1e-9 && s.speed(s.end()) == 0.0);
    // Without acceleration it stops at once.
    let at_once = Segment::stop(mm(1.0, 2.0, 3.0), [1.0, 0.0, 0.0], START, v, None);
    assert_eq!(at_once.duration(), Duration::ZERO);
}
