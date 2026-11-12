//! Without a usable GPU, `Compute` uses the CPU, says why, and warns once.
//! It runs everywhere: CI's runners have no GPU, and here GPU 99 is asked
//! for. It is alone in this file, since the warning is once per process.

#![cfg(feature = "cuda")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use oscope_compute::{Backend, Compute, ComputeConfig, Image, Lane, Shape};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Counts warnings.
struct Warnings(Arc<AtomicUsize>);

impl<S: Subscriber> Layer<S> for Warnings {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if *event.metadata().level() == Level::WARN {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn a_missing_gpu_falls_back_to_the_cpu_and_warns_once() {
    let warnings = Arc::new(AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(Warnings(warnings.clone()));
    let _recording = tracing::subscriber::set_default(subscriber);
    let config = ComputeConfig {
        cuda_device: Some(99),
        ..ComputeConfig::default()
    };
    for _ in 0..2 {
        let compute = Compute::new(&config).unwrap();
        assert!(compute.cuda().is_none());
        let info = compute.info();
        assert_eq!((info.backend(), info.gpu.as_ref()), (Backend::Cpu, None));
        let cuda = info
            .backends
            .iter()
            .find(|b| b.backend == Backend::Cuda)
            .unwrap();
        assert!(!cuda.available);
        let why = &cuda.detail;
        assert!(
            why.contains("no GPU 99") || why.contains("no CUDA driver"),
            "{why}"
        );
        let image = Image::from_vec(Shape::new(2, 2, 1), vec![1.0; 4]).unwrap();
        assert_eq!(compute.bin2x2(Lane::Live, &image).data(), [4.0]);
    }
    assert_eq!(warnings.load(Ordering::SeqCst), 1);
}
