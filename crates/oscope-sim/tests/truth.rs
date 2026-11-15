//! The simulator's truth for photometric calibration (P33): the optics'
//! vignetting, the sensor's colour response, and the colour target.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{FOCUS, rig, set, start};
use oscope_core::{Ms, PixelRect, Point2, Timestamp, Um};
use oscope_hal::{Camera, ObjectiveCatalogue};
use oscope_imaging::color::ColourTarget;
use oscope_sim::camera::vignette;
use oscope_sim::{ColourTargetSample, Field, Light, Optics, Rate, SimCameraConfig, Slide, World};

/// A tint the sensor reads in white light: red 1, green 0.85, blue 0.6.
const TINTED: [[f64; 3]; 3] = [[0.9, 0.1, 0.0], [0.05, 0.75, 0.05], [0.0, 0.1, 0.5]];

/// The black level, in 12-bit counts.
const BLACK: f64 = 125.0;

/// One frame of an empty slide, in transmitted light, without noise,
/// exposed `exposure` ms: its counts above black, 12-bit, and its width.
async fn blank(
    config: SimCameraConfig,
    mode: &str,
    roi: Option<PixelRect>,
    exposure: f64,
) -> (Vec<f64>, usize) {
    let rig = rig(
        config,
        false,
        Point2::new(40_000.0, 10_000.0),
        Rate::Stepped,
    );
    let settings = set(&rig.camera, |s| {
        s.mode = mode.into();
        s.roi = roi;
        s.exposure = Ms(exposure);
    })
    .await;
    let capabilities = rig.camera.capabilities();
    let rate = capabilities
        .mode(&settings.mode)
        .unwrap()
        .fps(settings.format)
        .unwrap();
    let stream = start(&rig.camera).await;
    let ends = Timestamp(0) + Duration::from_secs_f64(1.0 / rate) - Duration::from_millis(1);
    rig.clock.advance_to(ends);
    let (buffer, meta) = stream.recv_timeout(Duration::from_secs(60)).unwrap();
    rig.camera.stop().await.unwrap();
    let counts = buffer.bytes()[..(meta.width * meta.height * 2) as usize]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f64::from(u16::from_le_bytes(*b) >> 4) - BLACK)
        .collect();
    (counts, meta.width as usize)
}

/// The mean over a `side`-pixel square from `(x, y)`, of the mosaic's sites
/// of colour `c` (0 red, 1 green, 2 blue, RGGB).
fn mean(counts: &[f64], width: usize, (x, y): (usize, usize), side: usize, c: usize) -> f64 {
    let (mut sum, mut n) = (0.0, 0.0);
    for v in y..y + side {
        for u in x..x + side {
            if [[0, 1], [1, 2]][v % 2][u % 2] == c {
                sum += counts[v * width + u];
                n += 1.0;
            }
        }
    }
    sum / n
}

fn quiet() -> SimCameraConfig {
    let mut config = SimCameraConfig::small(768, 576);
    config.sensor.noise = false;
    config.sensor.dark_current = 0.0;
    config
}

#[tokio::test(flavor = "multi_thread")]
async fn the_light_falls_off_towards_the_corners_as_cos4_says() {
    let mut config = quiet();
    config.vignetting = 0.3;
    let r2 = (768.0_f64.powi(2) + 576.0_f64.powi(2)) / 4.0;
    // Full frame: a corner against the middle.
    let (counts, width) = blank(config.clone(), "full", None, 5.0).await;
    let middle = mean(&counts, width, (380, 284), 8, 1);
    let corner = mean(&counts, width, (0, 0), 8, 1);
    let expected = vignette(0.3, -380.0, -284.0, r2) / vignette(0.3, 0.0, 0.0, r2);
    let ratio = corner / middle;
    assert!(
        (ratio - expected).abs() < 0.01,
        "{ratio} against {expected}"
    );
    // Binned 2 × 2, a region of the sensor's lower right: the falloff
    // follows where on the sensor each pixel lies, not where in the frame.
    // Four pixels sum, so a shorter exposure keeps them unsaturated.
    let roi = Some(PixelRect::new(384, 288, 384, 288));
    let (counts, width) = blank(config, "bin2", roi, 1.0).await;
    let near = mean(&counts, width, (0, 0), 4, 1);
    let far = mean(&counts, width, (186, 138), 4, 1);
    let expected = vignette(0.3, 376.0, 280.0, r2) / vignette(0.3, 4.0, 4.0, r2);
    let ratio = far / near;
    assert!(
        (ratio - expected).abs() < 0.02,
        "{far} / {near} against {expected}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sensor_reads_white_light_through_its_colour_response() {
    let mut config = quiet();
    config.response = TINTED;
    let (counts, width) = blank(config, "full", None, 5.0).await;
    let channel = |c| mean(&counts, width, (300, 200), 64, c);
    let (r, g, b) = (channel(0), channel(1), channel(2));
    assert!(
        (g / r - 0.85).abs() < 0.01 && (b / r - 0.6).abs() < 0.01,
        "{r} {g} {b}"
    );
}

#[test]
fn the_colour_target_shows_its_patches_between_dark_borders() {
    let target = ColourTarget::oscope();
    let centre = Point2::new(47_000.0, 12_500.0);
    let sample = Arc::new(ColourTargetSample::new(centre));
    let world = World::with_slide(Slide::new(sample, Point2::new(0.0, 0.0)), Um(FOCUS));
    let catalogue = ObjectiveCatalogue::mitutoyo();
    let optics = Optics::new(&catalogue, catalogue.get("5x").unwrap(), Um(2.4));
    let p = optics.pixel_size().0;
    // A field of the whole target, focused on its patches.
    let (w, h) = (2_800, 1_900);
    let field = Field {
        centre,
        z: Um(FOCUS + 0.05),
        width: w,
        height: h,
    };
    let image = world.render(&field, &optics, Light::Transmitted);
    let at = |x_um: f64, y_um: f64| {
        let u = (f64::from(w) / 2.0 + x_um / p) as usize;
        let v = (f64::from(h) / 2.0 + y_um / p) as usize;
        let i = (v * w as usize + u) * 3;
        [0, 1, 2].map(|c| f64::from(image.data()[i + c]))
    };
    // The slide's and the coverslip's four surfaces pass 0.96⁴ of it all.
    let glass = 0.96_f64.powi(4);
    for patch in target.patches() {
        let seen = at(patch.x_um, patch.y_um);
        for (s, t) in seen.iter().zip(patch.rgb) {
            assert!((s - glass * t).abs() < 0.002, "{patch:?}: {seen:?}");
        }
        // Between patches, and around them, the border passes 1 %.
        let gap = (target.pitch_um + target.side_um) / 4.0;
        let border = at(patch.x_um + gap, patch.y_um);
        let dark = border.iter().all(|c| (c - glass * 0.01).abs() < 0.002);
        assert!(dark, "{border:?}");
    }
    // Beyond the target, clear glass.
    let clear = at(-target.size_um().0, 0.0);
    assert!(clear.iter().all(|c| (c - glass).abs() < 1e-4), "{clear:?}");
}

#[test]
fn each_sample_says_where_it_is_sharpest() {
    let at = Point2::new(47_000.0, 12_500.0);
    // The H&E section's middle: halves at 1 and 3 µm, each 2 µm thick.
    let tissue = oscope_sim::HeTissue::new(1, Slide::COVERSLIP);
    assert!((oscope_sim::Sample::in_focus(&tissue, at).0 - 2.0).abs() < 1e-9);
    // The reflective chip: its surface's height where it's looked at.
    let half = oscope_sim::Surface::SIDE.0 / 2.0;
    let chip = oscope_sim::Surface::new(1, Point2::new(at.x - half, at.y - half));
    let height = chip.height(at).unwrap().0;
    assert_eq!(oscope_sim::Sample::in_focus(&chip, at).0, height);
    // The cells: their dye's height, below their slab's middle, 3.25 µm, as
    // they lie thicker near the glass.
    let cells = oscope_sim::CellCulture::new(1, Slide::COVERSLIP);
    let dye = oscope_sim::Sample::in_focus(&cells, at).0;
    assert!(dye > 0.5 && dye < 2.75, "{dye}");
    // The world adds the slide's focus.
    let world = World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    assert_eq!(world.in_focus(at), Some(Um(FOCUS + 2.0)));
    assert_eq!(World::empty().in_focus(at), None);
}
