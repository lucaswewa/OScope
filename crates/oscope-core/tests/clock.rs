//! The clock: monotonic, mapped once to UTC, and faked for tests.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::{TimeDelta, TimeZone, Utc};
use oscope_core::{Clock, FakeClock, SystemClock, Timestamp};

#[test]
fn the_system_clock_never_goes_back_on_any_thread() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let clock = Arc::clone(&clock);
            thread::spawn(move || {
                let mut last = clock.now();
                for _ in 0..100_000 {
                    let now = clock.now();
                    assert!(now >= last, "{now} came after {last}");
                    last = now;
                }
                last
            })
        })
        .collect();
    let latest = readers
        .into_iter()
        .map(|r| r.join().unwrap())
        .max()
        .unwrap();
    assert!(clock.now() >= latest);
}

#[test]
fn the_system_clock_moves_and_maps_to_utc_from_its_start() {
    let before = Utc::now();
    let clock = SystemClock::new();
    let after = Utc::now();
    assert!(before <= clock.start_utc() && clock.start_utc() <= after);
    let first = clock.now();
    // Wait for the clock to move, rather than for a fixed time.
    let later = loop {
        let now = clock.now();
        if now > first {
            break now;
        }
        std::hint::spin_loop();
    };
    let elapsed = TimeDelta::nanoseconds(later.0 as i64);
    assert_eq!(clock.utc(later), clock.start_utc() + elapsed);
}

#[test]
fn a_fake_clock_moves_only_forwards_and_when_told() {
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 16, 0, 0).unwrap();
    let clock = FakeClock::new(start);
    assert_eq!(clock.now(), Timestamp::START);
    clock.advance(Duration::from_millis(69));
    assert_eq!(clock.now(), Timestamp(69_000_000));
    clock.advance_to(Timestamp(1_000));
    assert_eq!(
        clock.now(),
        Timestamp(69_000_000),
        "a clock never goes back"
    );
    clock.advance_to(Timestamp(2_000_000_000));
    assert_eq!(clock.utc(clock.now()), start + TimeDelta::seconds(2));
    clock.advance(Duration::MAX);
    assert_eq!(
        clock.now(),
        Timestamp(u64::MAX),
        "it stops at the end of time"
    );
}

#[test]
fn timestamps_add_subtract_and_find_midpoints() {
    let start = Timestamp(1_500_000_000);
    let end = start + Duration::from_micros(2_500);
    assert_eq!(end, Timestamp(1_502_500_000));
    assert_eq!(end.checked_since(start), Some(Duration::from_micros(2_500)));
    assert_eq!(start.checked_since(end), None);
    assert_eq!(start.saturating_since(end), Duration::ZERO);
    assert_eq!(start.midpoint(end), Timestamp(1_501_250_000));
    assert_eq!(
        Timestamp(u64::MAX).midpoint(Timestamp(u64::MAX - 2)),
        Timestamp(u64::MAX - 1)
    );
    assert_eq!(start - Duration::from_secs(10), Timestamp::START);
    assert_eq!(end.to_string(), "1.502500000 s");
    assert_eq!(serde_json::to_string(&end).unwrap(), "1502500000");
}
