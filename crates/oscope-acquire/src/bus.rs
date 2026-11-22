//! The frame bus: one producer's frames, fanned out to many subscribers (plan
//! §5.5, ADR-0008).
//!
//! Every subscriber gets a clone of the same [`Frame`], so no pixels are
//! copied.
//!
//! - **Lossless** subscribers get every frame, in order, through a bounded
//!   queue. When it is full, the producer either waits ([`WhenFull::Wait`]),
//!   or drops the frame for that subscriber alone and counts it
//!   ([`WhenFull::Drop`]).
//! - **Latest-wins** subscribers have one slot. A new frame replaces one not
//!   yet taken, which counts as dropped.
//! - [`FrameBus::next_frame_after`] waits for the first frame exposed after a
//!   given moment.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use oscope_core::{Frame, Timestamp};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc, oneshot};

/// What a lossless subscriber's full queue does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhenFull {
    /// The producer waits for room, so the subscriber loses nothing: for the
    /// recorder.
    Wait,
    /// The frame is dropped for this subscriber alone, and counted.
    Drop,
}

/// A subscriber's counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriberStats {
    /// Its name.
    pub name: String,
    /// Whether it is lossless, rather than latest-wins.
    pub lossless: bool,
    /// Frames it was given: queued, for a lossless subscriber; taken, for a
    /// latest-wins one.
    pub delivered: u64,
    /// Frames it missed.
    pub dropped: u64,
}

#[derive(Default)]
struct Counts {
    delivered: AtomicU64,
    dropped: AtomicU64,
}

#[derive(Default)]
struct Slot {
    frame: Mutex<Option<Frame>>,
    notify: Notify,
    closed: AtomicBool,
}

#[derive(Clone)]
enum Route {
    Lossless(mpsc::Sender<Frame>, WhenFull),
    Latest(Arc<Slot>),
}

struct Subscriber {
    name: String,
    route: Route,
    counts: Arc<Counts>,
}

type Waiter = (Timestamp, oneshot::Sender<Frame>);

#[derive(Default)]
struct Shared {
    subscribers: Mutex<Vec<Subscriber>>,
    waiters: Mutex<Vec<Waiter>>,
    published: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Route {
    /// Hands over `frame`; `false` once the subscriber has gone.
    fn deliver(&self, frame: &Frame, counts: &Counts) -> bool {
        let count = |counter: &AtomicU64| counter.fetch_add(1, Ordering::Relaxed);
        match self {
            Route::Lossless(queue, when_full) => match queue.try_send(frame.clone()) {
                Ok(()) => count(&counts.delivered),
                Err(TrySendError::Full(frame)) if *when_full == WhenFull::Wait => {
                    if queue.blocking_send(frame).is_err() {
                        return false;
                    }
                    count(&counts.delivered)
                }
                Err(TrySendError::Full(_)) => count(&counts.dropped),
                Err(TrySendError::Closed(_)) => return false,
            },
            Route::Latest(slot) => {
                if lock(&slot.frame).replace(frame.clone()).is_some() {
                    count(&counts.dropped);
                }
                slot.notify.notify_one();
                0
            }
        };
        true
    }

    fn is_open(&self) -> bool {
        match self {
            Route::Lossless(queue, _) => !queue.is_closed(),
            Route::Latest(slot) => Arc::strong_count(slot) > 1,
        }
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        for subscriber in lock(&self.subscribers).iter() {
            if let Route::Latest(slot) = &subscriber.route {
                slot.closed.store(true, Ordering::SeqCst);
                slot.notify.notify_one();
            }
        }
    }
}

/// One producer's frames, fanned out to many subscribers. Cloning the bus
/// shares it. When the last clone is dropped, every subscription ends.
#[derive(Clone, Default)]
pub struct FrameBus {
    shared: Arc<Shared>,
}

impl FrameBus {
    /// A bus with no subscribers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands `frame` to every subscriber, and to whoever waits for it.
    ///
    /// Call it from the producer's own thread, not from async code: a
    /// [`WhenFull::Wait`] subscriber's full queue makes it wait.
    pub fn publish(&self, frame: Frame) {
        self.shared.published.fetch_add(1, Ordering::Relaxed);
        self.wake_waiters(&frame);
        let routes: Vec<_> = lock(&self.shared.subscribers)
            .iter()
            .map(|s| (s.route.clone(), Arc::clone(&s.counts)))
            .collect();
        let mut gone = false;
        for (route, counts) in &routes {
            gone |= !route.deliver(&frame, counts);
        }
        drop(routes);
        if gone {
            lock(&self.shared.subscribers).retain(|s| s.route.is_open());
        }
    }

    fn wake_waiters(&self, frame: &Frame) {
        let start = frame.meta().exposure_start;
        let ready: Vec<Waiter> = {
            let mut waiters = lock(&self.shared.waiters);
            let (ready, waiting) = std::mem::take(&mut *waiters)
                .into_iter()
                .filter(|(_, reply)| !reply.is_closed())
                .partition(|(after, _)| *after <= start);
            *waiters = waiting;
            ready
        };
        for (_, reply) in ready {
            let _ = reply.send(frame.clone());
        }
    }

    fn add(&self, name: &str, route: Route) -> Arc<Counts> {
        let counts = Arc::new(Counts::default());
        lock(&self.shared.subscribers).push(Subscriber {
            name: name.to_owned(),
            route,
            counts: Arc::clone(&counts),
        });
        counts
    }

    /// A lossless subscription, with a queue of `capacity` frames. It ends
    /// when the receiver is dropped.
    pub fn subscribe_lossless(
        &self,
        name: &str,
        capacity: usize,
        when_full: WhenFull,
    ) -> mpsc::Receiver<Frame> {
        let (queue, receiver) = mpsc::channel(capacity.max(1));
        self.add(name, Route::Lossless(queue, when_full));
        receiver
    }

    /// A latest-wins subscription. It ends when the receiver is dropped.
    pub fn subscribe_latest(&self, name: &str) -> LatestReceiver {
        let slot = Arc::new(Slot::default());
        let counts = self.add(name, Route::Latest(Arc::clone(&slot)));
        LatestReceiver { slot, counts }
    }

    /// The first frame whose exposure starts at or after `after`: "the first
    /// frame exposed once the stage has stopped". The wait starts when this is
    /// called, not when the future is first polled. `None` if the bus goes
    /// away first.
    pub fn next_frame_after(
        &self,
        after: Timestamp,
    ) -> impl Future<Output = Option<Frame>> + Send + 'static {
        let (reply, frame) = oneshot::channel();
        lock(&self.shared.waiters).push((after, reply));
        async move { frame.await.ok() }
    }

    /// How many frames have been published.
    pub fn published(&self) -> u64 {
        self.shared.published.load(Ordering::Relaxed)
    }

    /// Each subscriber's counts.
    pub fn subscribers(&self) -> Vec<SubscriberStats> {
        lock(&self.shared.subscribers)
            .iter()
            .map(|s| SubscriberStats {
                name: s.name.clone(),
                lossless: matches!(s.route, Route::Lossless(..)),
                delivered: s.counts.delivered.load(Ordering::Relaxed),
                dropped: s.counts.dropped.load(Ordering::Relaxed),
            })
            .collect()
    }
}

/// A latest-wins subscription: one slot, which each new frame replaces.
pub struct LatestReceiver {
    slot: Arc<Slot>,
    counts: Arc<Counts>,
}

impl LatestReceiver {
    /// The newest frame not yet taken, waiting for one if there is none.
    /// `None` once the bus has gone away.
    pub async fn recv(&mut self) -> Option<Frame> {
        let slot = Arc::clone(&self.slot);
        loop {
            let published = slot.notify.notified();
            if let Some(frame) = self.try_recv() {
                return Some(frame);
            }
            if self.slot.closed.load(Ordering::SeqCst) {
                return None;
            }
            published.await;
        }
    }

    /// The newest frame not yet taken, if there is one.
    pub fn try_recv(&mut self) -> Option<Frame> {
        let frame = lock(&self.slot.frame).take();
        if frame.is_some() {
            self.counts.delivered.fetch_add(1, Ordering::Relaxed);
        }
        frame
    }
}
