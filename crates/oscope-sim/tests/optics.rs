//! The optics of Mitutoyo's objectives on the MIChrome 20's pixels.

use oscope_core::Um;
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::Optics;

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

const NAMES: [&str; 4] = ["5x", "10x", "20x", "50x"];

#[test]
fn in_focus_the_blur_is_diffraction_s() {
    for name in NAMES {
        let o = optics(name);
        assert!(
            (o.blur(Um(0.0)).0 - 0.21 * 0.55 / o.na).abs() < 1e-12,
            "{name}"
        );
    }
}

#[test]
fn the_blur_grows_with_distance_from_focus_on_either_side() {
    for name in NAMES {
        let o = optics(name);
        let mut before = o.blur(Um(0.0));
        for step in 1..=200 {
            let dz = Um(f64::from(step) * 0.5);
            let blur = o.blur(dz);
            assert!(blur > before, "{name} at {dz}");
            assert_eq!(blur, o.blur(-dz));
            before = blur;
        }
        // Far from focus, the disc of light dominates: |dz| tan θ / 2.
        let tan = o.na / (1.0 - o.na * o.na).sqrt();
        let far = o.blur(Um(100.0)).0;
        assert!(
            (far / (100.0 * tan / 2.0) - 1.0).abs() < 0.01,
            "{name}: {far}"
        );
    }
}

#[test]
fn pixels_and_depths_of_field() {
    // λ / NA² + e / (M NA), with λ = 0.55 µm and e = 2.4 µm.
    let expected = [
        ("5x", 0.48, 28.061 + 3.429),
        ("10x", 0.24, 7.015 + 0.857),
        ("20x", 0.12, 3.118 + 0.286),
        ("50x", 0.048, 1.818 + 0.087),
    ];
    for (name, pixel, depth) in expected {
        let o = optics(name);
        assert!((o.pixel_size().0 - pixel).abs() < 1e-12, "{name}");
        assert!(
            (o.depth_of_field().0 - depth).abs() < 0.002,
            "{name}: {}",
            o.depth_of_field()
        );
    }
}
