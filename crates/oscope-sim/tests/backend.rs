//! The composed simulator prefers CUDA and retains a CPU-only/fallback path.

use oscope_sim::{Simulator, SimulatorConfig};

#[tokio::test]
async fn composed_simulator_selects_the_available_backend() {
    let config = SimulatorConfig {
        sensor: "quarter".into(),
        sample: String::new(),
        ..SimulatorConfig::default()
    };
    let simulator = Simulator::new(&config, std::path::Path::new("."))
        .await
        .unwrap();
    #[cfg(feature = "cuda")]
    let available = oscope_compute::cuda::Cuda::open(0).is_ok();
    #[cfg(not(feature = "cuda"))]
    let available = false;
    assert_eq!(simulator.camera.on_gpu(), available);
    if std::env::var("OSCOPE_CUDA").as_deref() == Ok("1") {
        assert!(simulator.camera.on_gpu(), "CUDA required but unavailable");
    }
    assert_eq!(simulator.camera.prefer_gpu(), available);
}
