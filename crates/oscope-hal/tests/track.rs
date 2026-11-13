//! `PositionTrack`: property tests of its interpolation, and its ring.

use std::sync::Arc;
use std::thread;

use oscope_core::{Timestamp, Um, Xyz};
use oscope_hal::{PositionSample, PositionTrack};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// `n` samples at random positions over a 100 mm stage, 1 to 20 ms apart.
fn random_samples(rng: &mut StdRng, n: usize) -> Vec<PositionSample> {
    let mut time = rng.random_range(0..1_000_000_000_u64);
    (0..n)
        .map(|_| {
            time += rng.random_range(1_000_000..20_000_000);
            let mut axis = || Um(rng.random_range(0.0..100_000.0));
            PositionSample {
                time: Timestamp(time),
                position: Xyz::new(axis(), axis(), axis()),
            }
        })
        .collect()
}

fn track_of(samples: &[PositionSample]) -> PositionTrack {
    let track = PositionTrack::new(samples.len());
    for s in samples {
        assert!(track.push(*s));
    }
    track
}

fn close(a: Xyz, b: Xyz, tolerance: f64) -> bool {
    [(a.x, b.x), (a.y, b.y), (a.z, b.z)]
        .iter()
        .all(|(p, q)| (p.0 - q.0).abs() <= tolerance)
}

#[test]
fn a_sample_s_own_time_gives_its_own_position() {
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..100 {
        let n = rng.random_range(1..50);
        let samples = random_samples(&mut rng, n);
        let track = track_of(&samples);
        for s in &samples {
            assert_eq!(track.at(s.time), Some(s.position));
        }
    }
}

#[test]
fn between_two_samples_it_is_their_weighted_mean() {
    let mut rng = StdRng::seed_from_u64(2);
    for _ in 0..200 {
        let n = rng.random_range(2..50);
        let samples = random_samples(&mut rng, n);
        let track = track_of(&samples);
        let (first, last) = (samples[0].time.0, samples[samples.len() - 1].time.0);
        for _ in 0..20 {
            let t = rng.random_range(first..=last);
            // The oracle: the samples on either side, found by a plain scan,
            // weighted by how close each is to t.
            let i = samples.iter().rposition(|s| s.time.0 <= t).unwrap();
            let a = samples[i];
            let b = *samples.get(i + 1).unwrap_or(&a);
            let (ta, tb) = (a.time.0 as f64, b.time.0 as f64);
            let weigh = |p: Um, q: Um| {
                if tb == ta {
                    p
                } else {
                    Um((p.0 * (tb - t as f64) + q.0 * (t as f64 - ta)) / (tb - ta))
                }
            };
            let expected = Xyz::new(
                weigh(a.position.x, b.position.x),
                weigh(a.position.y, b.position.y),
                weigh(a.position.z, b.position.z),
            );
            let got = track.at(Timestamp(t)).unwrap();
            assert!(
                close(got, expected, 1e-6),
                "at {t}: {got:?} against {expected:?}"
            );
        }
    }
}

#[test]
fn a_steady_move_is_followed_exactly() {
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..100 {
        // 20 mm/s along x, -5 mm/s along y and 0.1 mm/s along z, from a
        // random start, reported at irregular intervals.
        let start = Xyz::new(
            Um(rng.random_range(0.0..50_000.0)),
            Um(rng.random_range(50_000.0..100_000.0)),
            Um(rng.random_range(0.0..10_000.0)),
        );
        let at = |ns: u64| {
            let s = ns as f64 / 1e9;
            Xyz::new(
                start.x + Um(20_000.0 * s),
                start.y - Um(5_000.0 * s),
                start.z + Um(100.0 * s),
            )
        };
        let mut time = 0;
        let track = PositionTrack::new(64);
        for _ in 0..64 {
            time += rng.random_range(1_000_000..20_000_000);
            track.push(PositionSample {
                time: Timestamp(time),
                position: at(time),
            });
        }
        for _ in 0..50 {
            let t = rng.random_range(track.oldest().unwrap().time.0..=time);
            assert!(close(track.at(Timestamp(t)).unwrap(), at(t), 1e-6));
        }
    }
}

#[test]
fn it_never_guesses_outside_its_samples() {
    let track = PositionTrack::new(8);
    assert_eq!(track.at(Timestamp(5)), None);
    assert!(track.is_empty());
    let mut rng = StdRng::seed_from_u64(4);
    let samples = random_samples(&mut rng, 8);
    let track = track_of(&samples);
    assert_eq!(track.at(Timestamp(samples[0].time.0 - 1)), None);
    assert_eq!(track.at(Timestamp(samples[7].time.0 + 1)), None);
    assert_eq!(track.latest(), Some(samples[7]));
}

#[test]
fn the_ring_keeps_the_latest_samples() {
    let mut rng = StdRng::seed_from_u64(5);
    let samples = random_samples(&mut rng, 100);
    let track = PositionTrack::new(16);
    for s in &samples {
        track.push(*s);
    }
    assert_eq!((track.len(), track.capacity()), (16, 16));
    assert_eq!(track.oldest(), Some(samples[84]));
    assert_eq!(track.at(samples[83].time), None);
    assert_eq!(track.at(samples[84].time), Some(samples[84].position));
    assert_eq!(PositionTrack::new(0).capacity(), 2);
}

#[test]
fn a_late_sample_is_ignored_and_a_repeated_time_replaced() {
    let sample = |t: u64, x: f64| PositionSample {
        time: Timestamp(t),
        position: Xyz::new(Um(x), Um(0.0), Um(0.0)),
    };
    let track = PositionTrack::new(8);
    assert!(track.push(sample(10, 1.0)));
    assert!(track.push(sample(20, 2.0)));
    assert!(!track.push(sample(15, 9.0)));
    assert!(track.push(sample(20, 3.0)));
    assert_eq!(track.len(), 2);
    assert_eq!(track.at(Timestamp(15)).unwrap().x, Um(2.0));
}

#[test]
fn readers_and_a_writer_share_it() {
    // A stage moving at 10 mm/s along x: every position read, interpolated
    // or not, must lie on its line.
    let on_line = |t: u64| Um(t as f64 * 1e-5);
    let track = Arc::new(PositionTrack::new(256));
    let writer = {
        let track = Arc::clone(&track);
        thread::spawn(move || {
            for i in 1..=20_000_u64 {
                let time = i * 1_000_000;
                let position = Xyz::new(on_line(time), Um(0.0), Um(0.0));
                track.push(PositionSample {
                    time: Timestamp(time),
                    position,
                });
            }
        })
    };
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let track = Arc::clone(&track);
            thread::spawn(move || {
                let mut read = 0;
                while read < 2_000 {
                    let (Some(oldest), Some(latest)) = (track.oldest(), track.latest()) else {
                        continue;
                    };
                    let t = (oldest.time.0 + latest.time.0) / 2;
                    if let Some(p) = track.at(Timestamp(t)) {
                        assert!((p.x.0 - on_line(t).0).abs() < 1e-6);
                        read += 1;
                    }
                }
            })
        })
        .collect();
    writer.join().unwrap();
    for reader in readers {
        reader.join().unwrap();
    }
}
