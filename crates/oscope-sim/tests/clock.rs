//! The simulation clock: stepped, scaled and at 1×, waiting on it, and
//! changing its rate.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use oscope_core::{Clock, Timestamp};
use oscope_sim::{Rate, SimClock};

const MS: u64 = 1_000_000;

#[test]
fn a_stepped_clock_moves_only_when_advanced() {
    let clock = SimClock::new(Rate::Stepped);
    let before = clock.now();
    let busy: u64 = (0..1_000_000_u64).sum();
    assert!(busy > 0 && clock.now() == before);
    clock.advance(Duration::from_millis(5));
    assert_eq!(clock.now(), Timestamp(5 * MS));
    clock.advance_to(Timestamp(3 * MS));
    assert_eq!(clock.now(), Timestamp(5 * MS), "it never goes back");
    clock.advance_to(Timestamp(9 * MS));
    assert_eq!(clock.now(), Timestamp(9 * MS));
}

#[tokio::test]
async fn a_sleeper_wakes_when_the_clock_passes_its_moment() {
    let clock = Arc::new(SimClock::new(Rate::Stepped));
    let woke = Arc::new(AtomicBool::new(false));
    let sleeper = tokio::spawn({
        let (clock, woke) = (Arc::clone(&clock), Arc::clone(&woke));
        async move {
            clock.sleep_until(Timestamp(MS)).await;
            woke.store(true, Ordering::SeqCst);
        }
    });
    tokio::task::yield_now().await;
    clock.advance(Duration::from_micros(999));
    tokio::task::yield_now().await;
    assert!(!woke.load(Ordering::SeqCst), "woken early");
    clock.advance(Duration::from_micros(1));
    sleeper.await.unwrap();
    assert!(woke.load(Ordering::SeqCst));
}

#[tokio::test]
async fn scaled_time_runs_faster_than_the_host() {
    // 2 s at 20× is 100 ms on the host.
    let clock = SimClock::new(Rate::Scaled(20.0));
    let host = Instant::now();
    clock.sleep(Duration::from_secs(2)).await;
    let took = host.elapsed();
    assert!(clock.now() >= Timestamp(2_000 * MS));
    assert!(
        took >= Duration::from_millis(99) && took < Duration::from_millis(300),
        "{took:?}"
    );
}

#[tokio::test]
async fn real_time_follows_the_host() {
    let clock = SimClock::new(Rate::RealTime);
    let host = Instant::now();
    clock.sleep(Duration::from_millis(200)).await;
    let took = host.elapsed();
    assert!(
        took >= Duration::from_millis(199) && took < Duration::from_millis(400),
        "{took:?}"
    );
}

#[tokio::test]
async fn a_change_of_rate_never_moves_the_clock() {
    let clock = Arc::new(SimClock::new(Rate::Scaled(1_000.0)));
    let mut last = clock.now();
    for rate in [
        Rate::Stepped,
        Rate::RealTime,
        Rate::Scaled(50.0),
        Rate::Stepped,
    ] {
        clock.set_rate(rate);
        let now = clock.now();
        assert!(now >= last, "{rate:?}");
        last = now;
    }
    clock.advance(Duration::from_secs(1));
    assert_eq!(clock.now(), last + Duration::from_secs(1));

    // A sleeper waiting on a stepped clock wakes once it runs again.
    let at = clock.now() + Duration::from_millis(30);
    let sleeper = tokio::spawn({
        let clock = Arc::clone(&clock);
        async move { clock.sleep_until(at).await }
    });
    tokio::task::yield_now().await;
    clock.set_rate(Rate::RealTime);
    sleeper.await.unwrap();
    assert!(clock.now() >= at);
}
