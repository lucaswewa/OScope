//! The sample's camera, packed, installed and loaded from its library, as
//! a start loads it, passes the HAL's conformance suite; the drivers'
//! registry knows it for a camera, and its frames' pixels come through; the
//! devices' configuration names it; and a manifest must provide the drivers
//! its library has.

use std::sync::Arc;
use std::time::Duration;

use ext_rust_camera::level;
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_core::{Clock, FrameBuffer, FramePool, HeapAllocator, PixelFormat, SystemClock};
use oscope_ext::Extensions;
use oscope_hal::{Camera, FrameSink, Trigger, conformance};

/// The bytes of `camera`'s next two frames, in its full mode, in 8 bits of
/// grey, free-running.
async fn two_frames(camera: &dyn Camera) -> [Vec<u8>; 2] {
    let mut settings = camera.settings().await.unwrap();
    settings.mode = "full".to_owned();
    settings.format = PixelFormat::Mono8;
    settings.roi = None;
    settings.trigger = Trigger::FreeRunning;
    let settings = camera.configure(settings).await.unwrap();
    let (_, width, height) = camera.capabilities().frame_shape(&settings).unwrap();
    let bytes = settings.format.frame_bytes(width, height);
    let (sender, frames) = std::sync::mpsc::channel();
    let sink: FrameSink = Arc::new(move |buffer: FrameBuffer, _| {
        let _ = sender.send(buffer.bytes()[..bytes].to_vec());
    });
    let pool = FramePool::new(bytes, 4, &HeapAllocator);
    camera.start(pool, sink).await.unwrap();
    let next = || frames.recv_timeout(Duration::from_secs(10)).unwrap();
    let both = [next(), next()];
    camera.stop().await.unwrap();
    both
}

#[tokio::test(flavor = "multi_thread")]
async fn the_extension_s_camera_passes_the_hal_conformance_suite() {
    let folder = tempfile::tempdir().unwrap();
    let library = ex_rust_ext::library_of("ext_rust_camera").unwrap();
    let sample = ex_rust_ext::sample_folder("ext-rust-camera");
    let package = folder.path().join("fakecam.omext");
    ex_rust_ext::pack_sample(&sample, &library, folder.path(), &package).unwrap();
    let root = folder.path().join("extensions");
    Extensions::open(&root).unwrap().install(&package).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    let names: Vec<_> = extensions.drivers().names().collect();
    assert_eq!(names, [("example.fakecam", "camera")]);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let drivers = extensions.drivers();
    let wrong = drivers
        .stage("example.fakecam", Arc::clone(&clock))
        .unwrap()
        .err();
    let said = "`example.fakecam` is a camera driver, not a stage driver";
    assert_eq!(wrong.as_deref(), Some(said));
    assert!(
        drivers
            .camera("example.nothing", Arc::clone(&clock))
            .is_none()
    );
    let camera = drivers.camera("example.fakecam", clock).unwrap().unwrap();
    conformance::camera(&*camera).await.unwrap();

    // Its pixels come through: the sample's stripes, a pixel further on in
    // the next frame. The frame's count is even or odd: one of them holds.
    let counted = |frame: &[u8]| {
        let even = 2 * u64::from(frame[0]);
        let holds = |n: u64| {
            let row = |y: u32| (0..640).all(|x| frame[(y * 640 + x) as usize] == level(x, y, n));
            (0..480).all(row)
        };
        [even, even + 1].into_iter().find(|n| holds(*n))
    };
    let [first, second] = two_frames(&*camera).await;
    let first = counted(&first).expect("the sample's stripes");
    assert_eq!(counted(&second), Some((first + 1) % 512));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_devices_name_the_extension_s_camera_and_a_manifest_must_match_its_drivers() {
    let folder = tempfile::tempdir().unwrap();
    let library = ex_rust_ext::library_of("ext_rust_camera").unwrap();
    let sample = ex_rust_ext::sample_folder("ext-rust-camera");
    let package = folder.path().join("fakecam.omext");
    ex_rust_ext::pack_sample(&sample, &library, folder.path(), &package).unwrap();
    let root = folder.path().join("extensions");
    Extensions::open(&root).unwrap().install(&package).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    let fake = || "fake".to_owned();
    let mut config = DevicesConfig {
        camera: "example.fakecam".to_owned(),
        stage: fake(),
        illumination: fake(),
        objective: fake(),
        ..DevicesConfig::default()
    };
    let devices = Devices::from_config_with(&config, folder.path(), extensions.drivers());
    let devices = devices.await.unwrap();
    assert_eq!(
        devices.camera.capabilities().model,
        "OScope's example fake camera"
    );
    config.camera = "example.nothing".to_owned();
    let unknown = Devices::from_config_with(&config, folder.path(), extensions.drivers()).await;
    let said =
        "`camera = \"example.nothing\"`: the camera drivers are sim, fake and example.fakecam";
    assert_eq!(unknown.err().as_deref(), Some(said));

    // The same library, its manifest providing another driver: not loaded.
    let source = folder.path().join("ext-rust-camera");
    let manifest = std::fs::read_to_string(source.join("manifest.toml")).unwrap();
    let other = manifest.replace("\"example.fakecam\"", "\"example.othercam\"");
    std::fs::write(source.join("manifest.toml"), other).unwrap();
    let renamed = folder.path().join("othercam.omext");
    oscope_ext::pack(&source, &renamed).unwrap();
    let elsewhere = folder.path().join("elsewhere");
    Extensions::open(&elsewhere)
        .unwrap()
        .install(&renamed)
        .unwrap();
    let extensions = Extensions::open(&elsewhere).unwrap();
    assert_eq!(extensions.drivers().names().count(), 0);
    let extension = extensions.get("org.oscope.examples.fakecam").unwrap();
    let said = "its library has the drivers `example.fakecam`, and its manifest provides `example.othercam`";
    assert_eq!(extension.problem.as_deref(), Some(said));
}
