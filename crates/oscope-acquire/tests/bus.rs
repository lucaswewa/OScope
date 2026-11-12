//! The frame bus: fan-out without copies, lossless and latest-wins delivery,
//! counts, and the first frame after a moment.

use std::thread;
use std::time::Duration;

use chrono::Utc;
use oscope_acquire::{FrameBus, SubscriberStats, WhenFull};
use oscope_core::{
    Clock, FakeClock, Frame, FrameId, FrameMeta, FramePool, HeapAllocator, PixelFormat,
};

/// A frame of 4 × 4 grey pixels, exposed now on `clock`.
fn frame(pool: &FramePool, id: u64, clock: &dyn Clock) -> Frame {
    let mut buffer = pool.take().expect("a free slab");
    buffer.bytes_mut().fill(id as u8);
    Frame::new(
        buffer,
        FrameMeta::new(FrameId(id), PixelFormat::Mono8, 4, 4, clock.now()),
    )
    .unwrap()
}

fn stats(bus: &FrameBus, name: &str) -> SubscriberStats {
    bus.subscribers()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap()
}

#[tokio::test]
async fn every_subscriber_gets_the_same_pixels() {
    let (clock, pool, bus) = (
        FakeClock::new(Utc::now()),
        FramePool::new(16, 4, &HeapAllocator),
        FrameBus::new(),
    );
    let mut a = bus.subscribe_lossless("a", 4, WhenFull::Drop);
    let mut b = bus.subscribe_lossless("b", 4, WhenFull::Drop);
    let mut c = bus.subscribe_latest("c");
    let sent = frame(&pool, 0, &clock);
    bus.publish(sent.clone());
    for received in [
        a.recv().await.unwrap(),
        b.recv().await.unwrap(),
        c.recv().await.unwrap(),
    ] {
        assert!(received.same_pixels(&sent));
    }
    assert_eq!(pool.stats().in_use, 1, "one slab, however many subscribers");
}

#[tokio::test]
async fn a_waiting_lossless_subscriber_gets_every_frame_in_order() {
    let pool = FramePool::new(16, 8, &HeapAllocator);
    let bus = FrameBus::new();
    let mut recorder = bus.subscribe_lossless("recorder", 2, WhenFull::Wait);
    // The producer owns the bus, so the queue ends when it is done: a lost
    // frame fails the test rather than leaving it waiting.
    let producer = thread::spawn(move || {
        let clock = FakeClock::new(Utc::now());
        for id in 0..200 {
            bus.publish(frame(&pool, id, &clock));
        }
        stats(&bus, "recorder")
    });
    let mut received = 0;
    while let Some(frame) = recorder.recv().await {
        assert_eq!(frame.meta().id, FrameId(received));
        assert_eq!(frame.bytes()[0], received as u8);
        if received % 50 == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await; // a slow disk: the producer must wait
        }
        received += 1;
    }
    assert_eq!(received, 200);
    let counts = producer.join().unwrap();
    assert_eq!((counts.delivered, counts.dropped), (200, 0));
}

#[tokio::test]
async fn a_dropping_lossless_subscriber_keeps_the_oldest_and_counts_the_rest() {
    let (clock, pool, bus) = (
        FakeClock::new(Utc::now()),
        FramePool::new(16, 4, &HeapAllocator),
        FrameBus::new(),
    );
    let mut queue = bus.subscribe_lossless("statistics", 2, WhenFull::Drop);
    for id in 0..10 {
        bus.publish(frame(&pool, id, &clock));
    }
    assert_eq!(queue.recv().await.unwrap().meta().id, FrameId(0));
    assert_eq!(queue.recv().await.unwrap().meta().id, FrameId(1));
    assert!(queue.try_recv().is_err());
    let counts = stats(&bus, "statistics");
    assert_eq!((counts.delivered, counts.dropped), (2, 8));
    assert_eq!(
        pool.stats().in_use,
        0,
        "dropped frames gave their slabs back"
    );
}

#[tokio::test]
async fn a_latest_wins_subscriber_gets_the_newest_frame() {
    let (clock, pool, bus) = (
        FakeClock::new(Utc::now()),
        FramePool::new(16, 2, &HeapAllocator),
        FrameBus::new(),
    );
    let mut preview = bus.subscribe_latest("preview");
    for id in 0..5 {
        bus.publish(frame(&pool, id, &clock));
    }
    assert_eq!(preview.recv().await.unwrap().meta().id, FrameId(4));
    assert!(preview.try_recv().is_none(), "nothing newer yet");
    let counts = stats(&bus, "preview");
    assert_eq!((counts.delivered, counts.dropped), (1, 4));
    let waiting = tokio::spawn(async move { preview.recv().await.map(|f| f.meta().id) });
    tokio::task::yield_now().await;
    bus.publish(frame(&pool, 5, &clock));
    assert_eq!(
        waiting.await.unwrap(),
        Some(FrameId(5)),
        "a waiting receiver is woken"
    );
}

#[tokio::test]
async fn the_first_frame_after_a_moment_is_found_by_its_exposure() {
    let (clock, pool, bus) = (
        FakeClock::new(Utc::now()),
        FramePool::new(16, 4, &HeapAllocator),
        FrameBus::new(),
    );
    clock.advance(Duration::from_millis(10));
    let between = bus.next_frame_after(clock.now() + Duration::from_millis(15));
    let exactly = bus.next_frame_after(clock.now() + Duration::from_millis(20));
    for _ in 0..4 {
        bus.publish(frame(&pool, clock.now().0 / 1_000_000, &clock)); // at 10, 20, 30 and 40 ms
        clock.advance(Duration::from_millis(10));
    }
    let between = between.await.unwrap().meta().id;
    assert_eq!(between, FrameId(30), "the first at or after 25 ms");
    let exactly = exactly.await.unwrap().meta().id;
    assert_eq!(
        exactly,
        FrameId(30),
        "a frame exposed at the moment itself counts"
    );
}

#[tokio::test]
async fn subscriptions_end_with_their_receiver_or_with_the_bus() {
    let (clock, pool, bus) = (
        FakeClock::new(Utc::now()),
        FramePool::new(16, 4, &HeapAllocator),
        FrameBus::new(),
    );
    let lossless = bus.subscribe_lossless("gone", 1, WhenFull::Wait);
    let latest = bus.subscribe_latest("also gone");
    let mut kept = bus.subscribe_latest("kept");
    drop((lossless, latest));
    bus.publish(frame(&pool, 0, &clock));
    let names: Vec<_> = bus.subscribers().into_iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        ["kept"],
        "a full waiting queue with no receiver doesn't block the producer"
    );
    let mut queue = bus.subscribe_lossless("queue", 1, WhenFull::Drop);
    let waiter = bus.next_frame_after(clock.now() + Duration::from_secs(1));
    assert!(kept.recv().await.is_some());
    drop(bus);
    assert!(kept.recv().await.is_none(), "the bus is gone");
    assert!(queue.recv().await.is_none());
    assert!(waiter.await.is_none());
}
