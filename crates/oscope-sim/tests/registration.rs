//! Registration end to end (P31): while the stage jogs at a constant speed
//! under the default sample, H&E tissue at 10×, through a turned mounting,
//! consecutive frames register to the stage's displacement seen through the
//! mounting.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{FOCUS, rig_over, set, start};
use oscope_compute::{Image, Shape};
use oscope_core::{FrameBuffer, FrameMeta, Ms, Point2, Timestamp, Um};
use oscope_hal::{Axis, Camera, Stage, UmPerS};
use oscope_imaging::registration::{RegisterOptions, Registration, register};
use oscope_sim::{HeTissue, Mounting, Rate, SampleCatalogue, SimCameraConfig, Slide, World};

/// A frame's mosaic as grey: each 2 × 2 cell's four counts summed, so an
/// image of half the frame's width and height, free of the mosaic.
fn grey(buffer: &FrameBuffer, meta: &FrameMeta) -> Image {
    let (w, h) = (meta.width as usize, meta.height as usize);
    let counts: Vec<f32> = buffer.bytes()[..w * h * 2]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(u16::from_le_bytes(*b)))
        .collect();
    let mut cells = Vec::with_capacity(w * h / 4);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let at = |dx: usize, dy: usize| counts[(y + dy) * w + x + dx];
            cells.push(at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1));
        }
    }
    Image::from_vec(Shape::new(meta.width / 2, meta.height / 2, 1), cells).unwrap()
}

/// Jogs along `axis` at `speed` under the tissue's first island, through
/// `mounting`, and registers the first two frames' grey images: what is
/// found, and the shift expected, in the grey images' cells.
async fn jog(mounting: Mounting, axis: Axis, speed: f64) -> (Registration, (f64, f64)) {
    let entry = SampleCatalogue::builtin();
    let sample = entry
        .get("H&E tissue")
        .unwrap()
        .build(Path::new("."))
        .unwrap();
    let world = World::with_slide(Slide::new(sample, Point2::new(0.0, 0.0)), Um(FOCUS));
    let mut config = SimCameraConfig::small(768, 576);
    config.mounting = mounting;
    // The catalogue's tissue is from seed 1, over the coverslip.
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let rig = rig_over(config, world, island, Rate::Stepped);
    set(&rig.camera, |s| s.exposure = Ms(1.0)).await;
    rig.stage
        .jog(axis, true, Some(UmPerS(speed)))
        .await
        .unwrap();
    let frames = start(&rig.camera).await;
    // Full resolution in 16 bits: 7.24 fps; 60 ms of readout over 576 rows.
    let interval = Duration::from_secs_f64(1.0 / 7.24);
    let line = Ms(60.0 / 576.0).to_duration().unwrap();
    let mut taken = Vec::new();
    for k in 0..2_u32 {
        let ends = Timestamp(0) + interval * k + line * 575 + Duration::from_millis(1);
        rig.clock.advance_to(ends);
        taken.push(
            frames
                .recv_timeout(Duration::from_secs(60))
                .expect("a frame"),
        );
    }
    rig.camera.stop().await.unwrap();
    let ((first, a_meta), (second, b_meta)) = (&taken[0], &taken[1]);
    let found = register(
        &grey(first, a_meta),
        &grey(second, b_meta),
        &RegisterOptions::default(),
    );

    // The stage moves at v; the image moves by u = −(mirror · turn⁻¹) v / p
    // sensor pixels a second, p = 0.24 µm at 10×. The rolling shutter adds
    // to it: content dy rows lower was exposed dy line times L later, so
    // dy = u_y (Δt + dy L), and dx = u_x (Δt + dy L), for frames Δt apart.
    let dt = b_meta
        .exposure_start
        .saturating_since(a_meta.exposure_start);
    let (dt, l) = (dt.as_secs_f64(), line.as_secs_f64());
    let (vx, vy) = match axis {
        Axis::X => (speed, 0.0),
        _ => (0.0, speed),
    };
    let (sin, cos) = mounting.rotation.sin_cos();
    let (sx, sy) = (cos * vx + sin * vy, -sin * vx + cos * vy);
    let sx = if mounting.mirrored { -sx } else { sx };
    let (ux, uy) = (-sx / 0.24, -sy / 0.24);
    let dy = uy * dt / (1.0 - uy * l);
    let dx = ux * (dt + dy * l);
    (found.unwrap(), (dx / 2.0, dy / 2.0))
}

#[tokio::test]
async fn consecutive_frames_of_a_jog_register_to_the_stage_s_displacement() {
    // 70 µm/s along x, turned 30° and mirrored: 9.7 µm between frames, 20
    // cells; and 250 µm/s along y, turned −12°: 34.5 µm, 72 cells, where
    // the rolling shutter adds a tenth.
    let turned = |degrees: f64, mirrored| Mounting {
        rotation: degrees.to_radians(),
        mirrored,
        offset: Point2::new(0.0, 0.0),
    };
    for (mounting, axis, speed) in [
        (turned(30.0, true), Axis::X, 70.0),
        (turned(-12.0, false), Axis::Y, 250.0),
    ] {
        let (found, expected) = jog(mounting, axis, speed).await;
        let error = (found.dx - expected.0)
            .abs()
            .max((found.dy - expected.1).abs());
        assert!(error <= 0.1, "found {found:?}, expected {expected:?}");
        // The longer jog's frames overlap less: 0.98 and 0.86 were found.
        assert!(found.confidence > 0.75, "{found:?}");
    }
}
