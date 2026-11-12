//! The live and batch pools, and what `Compute` reports.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use oscope_compute::{Backend, Compute, ComputeConfig, Image, Lane, Shape, SimdLevel, cpu};

fn compute(live: usize, batch: usize) -> Compute {
    let config = ComputeConfig {
        live_threads: Some(live),
        batch_threads: Some(batch),
    };
    Compute::new(&config).unwrap()
}

/// Waits until `done` holds, for up to 10 s.
fn wait_until(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "not in time");
        std::thread::yield_now();
    }
}

#[test]
fn a_full_batch_pool_does_not_hold_up_live_work() {
    let compute = Arc::new(compute(2, 2));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let started = Arc::new(AtomicUsize::new(0));
    let lanes = Arc::new(Mutex::new(Vec::new()));
    // Six batch jobs: two take the batch threads and wait at the gate, and
    // four queue behind them.
    for _ in 0..6 {
        let (gate, started, lanes) = (gate.clone(), started.clone(), lanes.clone());
        compute.pools().spawn(Lane::Batch, move || {
            lanes.lock().unwrap().push(Lane::current());
            started.fetch_add(1, Ordering::SeqCst);
            let (open, opened) = &*gate;
            let mut open = open.lock().unwrap();
            while !*open {
                open = opened.wait(open).unwrap();
            }
        });
    }
    wait_until(|| started.load(Ordering::SeqCst) == 2);

    // Live work, on a thread of its own, so that a wait shows as a timeout.
    let (sender, receiver) = mpsc::channel();
    let live = {
        let compute = compute.clone();
        std::thread::spawn(move || {
            let image = Image::from_vec(Shape::new(64, 64, 1), vec![1.0; 4096]).unwrap();
            let lane = compute.pools().install(Lane::Live, Lane::current);
            let _ = sender.send((lane, compute.bin2x2(Lane::Live, &image)));
        })
    };
    let result = receiver.recv_timeout(Duration::from_secs(10));
    let started_meanwhile = started.load(Ordering::SeqCst);
    let (open, opened) = &*gate;
    *open.lock().unwrap() = true;
    opened.notify_all();
    live.join().unwrap();

    let (lane, binned) = result.expect("the live work waited for the batch pool");
    assert_eq!(lane, Some(Lane::Live));
    assert!(binned.data().iter().all(|&v| v == 4.0));
    assert_eq!(
        started_meanwhile, 2,
        "batch work ran beyond its two threads"
    );
    wait_until(|| started.load(Ordering::SeqCst) == 6);
    let lanes = lanes.lock().unwrap();
    assert!(
        lanes.iter().all(|&lane| lane == Some(Lane::Batch)),
        "{lanes:?}"
    );
}

#[test]
fn the_pools_have_the_threads_asked_for_and_name_them_by_lane() {
    let compute = compute(3, 5);
    assert_eq!(compute.pools().pool(Lane::Live).current_num_threads(), 3);
    assert_eq!(compute.pools().pool(Lane::Batch).current_num_threads(), 5);
    assert_eq!(
        compute.pools().install(Lane::Batch, Lane::current),
        Some(Lane::Batch)
    );
    assert_eq!(Lane::current(), None);
    let names = compute.pools().install(Lane::Live, || {
        std::thread::current().name().map(str::to_owned)
    });
    assert!(names.unwrap().starts_with("oscope-live-"));

    // The live pool runs a kernel's parallel work itself.
    let image = Image::from_vec(Shape::new(512, 512, 1), vec![0.5; 512 * 512]).unwrap();
    let lanes = compute.pools().install(Lane::Live, || {
        use rayon::prelude::*;
        (0..64)
            .into_par_iter()
            .map(|_| Lane::current())
            .collect::<Vec<_>>()
    });
    assert!(lanes.iter().all(|&lane| lane == Some(Lane::Live)));
    assert_eq!(cpu::bin2x2(&image).width(), 256);

    let tiny = compute_with(ComputeConfig {
        live_threads: Some(0),
        batch_threads: Some(0),
    });
    assert_eq!(
        (tiny.info().live_threads, tiny.info().batch_threads),
        (1, 1)
    );
}

fn compute_with(config: ComputeConfig) -> Compute {
    Compute::new(&config).unwrap()
}

#[test]
fn the_info_lists_the_backends_and_divides_the_processor() {
    let compute = compute_with(ComputeConfig::default());
    let info = compute.info();
    let threads = std::thread::available_parallelism().unwrap().get();
    assert_eq!(info.cpu_threads, threads);
    assert_eq!(info.live_threads, (threads / 4).max(2));
    assert_eq!(
        info.batch_threads,
        threads.saturating_sub(info.live_threads).max(1)
    );
    let backends: Vec<(Backend, bool)> = info
        .backends
        .iter()
        .map(|b| (b.backend, b.available))
        .collect();
    assert_eq!(backends, [(Backend::Cpu, true), (Backend::Cuda, false)]);
    assert_eq!(info.backend(), Backend::Cpu);
    #[cfg(target_arch = "x86_64")]
    assert!(info.simd >= SimdLevel::Sse2);
    let json = serde_json::to_value(info).unwrap();
    assert_eq!(json["backends"][0]["backend"], "cpu");
}
