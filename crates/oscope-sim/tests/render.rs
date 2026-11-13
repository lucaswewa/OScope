//! The renderer against the test target's exact answers: Beer–Lambert, the
//! glass, reflection, blur against distance from focus, scale between
//! objectives, and levels of detail.

use std::sync::Arc;

use oscope_compute::{Image, Shape, cpu};
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Field, Layer, LayerKind, Light, Optics, Sample, Slide, TestTarget, World};

/// The glass under the coverslip: four surfaces, each passing 96%.
const GLASS: f64 = 0.849_346_56;
/// The focus: the stage z at which the slide's surface is sharp.
const FOCUS: f64 = 1_000.0;

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

/// A target at (30, 10) mm on a slide whose corner is at the stage's
/// origin, so slide and stage coordinates agree.
fn world(od: f64, reflectance: f64) -> World {
    let target = TestTarget::new(Point2::new(30_000.0, 10_000.0), od, reflectance);
    World::with_slide(
        Slide::new(Arc::new(target), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

/// A point on the target, from its corner.
fn on_target(x: f64, y: f64) -> Point2 {
    Point2::new(30_000.0 + x, 10_000.0 + y)
}

fn field(centre: Point2, dz: f64, width: u32, height: u32) -> Field {
    Field {
        centre,
        z: Um(FOCUS + dz),
        width,
        height,
    }
}

fn all_near(image: &Image, value: f64, tolerance: f64) -> bool {
    image
        .data()
        .iter()
        .all(|v| (f64::from(*v) - value).abs() <= tolerance)
}

#[test]
fn a_square_passes_ten_to_the_minus_its_optical_density() {
    let world = world(0.5, 0.0);
    // Inside a square, and between squares, at 20×.
    let inside = world.render(
        &field(on_target(1_025.0, 1_025.0), 0.0, 64, 64),
        &optics("20x"),
        Light::Transmitted,
    );
    assert!(all_near(&inside, GLASS * 10f64.powf(-0.5), 1e-4));
    let between = world.render(
        &field(on_target(1_075.0, 1_075.0), 0.0, 64, 64),
        &optics("20x"),
        Light::Transmitted,
    );
    assert!(all_near(&between, GLASS, 1e-4));
}

#[test]
fn the_glass_and_its_edges() {
    let world = world(0.5, 0.0);
    let o = optics("10x");
    let at = |x: f64, y: f64| {
        world.render(
            &field(Point2::new(x, y), 0.0, 16, 16),
            &o,
            Light::Transmitted,
        )
    };
    // Off the slide, on the slide's glass alone, and under the coverslip.
    assert!(all_near(&at(-5_000.0, 10_000.0), 1.0, 1e-6));
    assert!(all_near(&at(10_000.0, 10_000.0), 0.96 * 0.96, 1e-4));
    assert!(all_near(&at(25_000.0, 10_000.0), GLASS, 1e-4));
    // The coverslip's edge is darker than either side.
    let edge = at(22_000.0, 10_000.0)
        .data()
        .iter()
        .copied()
        .fold(f32::MAX, f32::min);
    assert!(f64::from(edge) < 0.5 * GLASS, "{edge}");
    // No slide: the light passes, and nothing reflects.
    let empty = World::empty();
    let f = field(Point2::new(0.0, 0.0), 0.0, 8, 8);
    assert!(all_near(
        &empty.render(&f, &o, Light::Transmitted),
        1.0,
        0.0
    ));
    assert!(all_near(&empty.render(&f, &o, Light::Reflected), 0.0, 0.0));
}

#[test]
fn reflected_light_shows_the_bars_in_focus() {
    let world = world(0.5, 0.6);
    let o = optics("20x");
    let height = TestTarget::BAR_HEIGHT.0;
    // A bar's middle, and a gap's, with the bars' layer in focus: the glass
    // reflects 4%, and a bar 60% more.
    let bar = world.render(
        &field(on_target(1_010.0, 1_025.0), height, 32, 32),
        &o,
        Light::Reflected,
    );
    assert!(all_near(&bar, 0.04 + 0.6, 1e-4));
    let gap = world.render(
        &field(on_target(1_030.0, 1_025.0), height, 32, 32),
        &o,
        Light::Reflected,
    );
    assert!(all_near(&gap, 0.04, 1e-4));
    // Across a bar and a gap: sharp in focus; focused on the squares, 20 µm
    // below, blurred by about 4.6 µm, so the bar's middle dims to about
    // 0.62 and the gap's brightens to about 0.06.
    let span = |dz: f64| {
        let image = world.render(
            &field(on_target(1_020.0, 1_025.0), dz, 400, 1),
            &o,
            Light::Reflected,
        );
        let (lo, hi) = image
            .data()
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        (f64::from(lo), f64::from(hi))
    };
    let (lo, hi) = span(height);
    assert!(
        (lo - 0.04).abs() < 1e-3 && (hi - 0.64).abs() < 1e-3,
        "{lo}, {hi}"
    );
    let (lo, hi) = span(0.0);
    assert!(lo > 0.05 && hi < 0.63, "{lo}, {hi}");
    // In transmitted light, a bar passes what it doesn't reflect. Between
    // rows of squares, nothing else is in the way.
    let through = world.render(
        &field(on_target(1_010.0, 1_075.0), height, 32, 32),
        &o,
        Light::Transmitted,
    );
    assert!(all_near(&through, 0.4 * GLASS, 1e-4));
}

/// A uniform stain of optical density 0.3 two micrometres above a uniform
/// mirror that reflects half the light.
struct Stained;

impl Sample for Stained {
    fn name(&self) -> &str {
        "stained mirror"
    }
    fn layers(&self) -> &[Layer] {
        static LAYERS: std::sync::OnceLock<Vec<Layer>> = std::sync::OnceLock::new();
        LAYERS.get_or_init(|| {
            let layer = |z: f64, kind| Layer {
                name: String::new(),
                z: Um(z),
                thickness: Um(0.1),
                kind,
            };
            vec![
                layer(0.0, LayerKind::Reflective),
                layer(2.0, LayerKind::Absorbing),
            ]
        })
    }
    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, 75_000.0, 25_000.0)
    }
    fn render(&self, index: usize, _: Point2, _: Um, width: u32, height: u32) -> Image {
        let mut image = Image::zeros(Shape::new(width, height, 3));
        image.data_mut().fill(if index == 0 { 0.5 } else { 0.3 });
        image
    }
}

#[test]
fn a_reflection_passes_twice_through_the_stain_above_it() {
    let world = World::with_slide(
        Slide::new(Arc::new(Stained), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    );
    let at = field(Point2::new(40_000.0, 10_000.0), 0.0, 16, 16);
    let o = optics("10x");
    let reflected = world.render(&at, &o, Light::Reflected);
    assert!(all_near(&reflected, 0.04 + 0.5 * 10f64.powf(-0.6), 1e-5));
    let passed = world.render(&at, &o, Light::Transmitted);
    assert!(all_near(&passed, GLASS * 0.5 * 10f64.powf(-0.3), 1e-5));
}

/// The 10–90% width of the edge of a square `dz` from focus, in µm.
fn edge_width(world: &World, o: &Optics, dz: f64) -> f64 {
    let p = o.pixel_size().0;
    // The squares' layer is 0.05 µm above the slide.
    let image = world.render(
        &field(on_target(1_050.0, 1_025.0), 0.05 + dz, 256, 1),
        o,
        Light::Transmitted,
    );
    let (dark, bright) = (GLASS * 10f64.powf(-0.5), GLASS);
    let level: Vec<f64> = image
        .data()
        .iter()
        .step_by(3)
        .map(|v| (f64::from(*v) - dark) / (bright - dark))
        .collect();
    let crossing = |at: f64| {
        let i = level.iter().position(|v| *v >= at).unwrap();
        (i - 1) as f64 + (at - level[i - 1]) / (level[i] - level[i - 1])
    };
    (crossing(0.9) - crossing(0.1)) * p
}

#[test]
fn the_blur_grows_with_distance_from_focus() {
    let world = world(0.5, 0.0);
    let o = optics("10x");
    let mut before = 0.0;
    for dz in [0.0, 5.0, 10.0, 20.0, 40.0] {
        let width = edge_width(&world, &o, dz);
        assert!(width > before, "at {dz} µm: {width}");
        assert!((width - edge_width(&world, &o, -dz)).abs() < 1e-6);
        // A Gaussian-blurred edge rises from 10% to 90% over 2.563 σ.
        if dz >= 10.0 {
            let expected = 2.563 * o.blur(Um(dz)).0;
            assert!(
                (width / expected - 1.0).abs() < 0.05,
                "at {dz} µm: {width} against {expected}"
            );
        }
        before = width;
    }
}

#[test]
fn a_square_scales_with_the_objective() {
    let world = world(0.5, 0.0);
    let mut widths = Vec::new();
    for name in ["5x", "10x", "20x"] {
        let o = optics(name);
        let p = o.pixel_size().0;
        let across = (80.0 / p).ceil() as u32;
        let image = world.render(
            &field(on_target(1_025.0, 1_025.0), 0.0, across, 1),
            &o,
            Light::Transmitted,
        );
        let half = GLASS * (1.0 + 10f64.powf(-0.5)) / 2.0;
        let dark = image
            .data()
            .iter()
            .step_by(3)
            .filter(|v| f64::from(**v) < half)
            .count();
        assert!((dark as f64 * p - 50.0).abs() <= p, "{name}: {dark} pixels");
        widths.push(dark as f64);
    }
    assert!((widths[1] / widths[0] - 2.0).abs() < 0.03);
    assert!((widths[2] / widths[1] - 2.0).abs() < 0.02);
}

#[test]
fn a_coarse_rendering_is_a_fine_one_averaged_down() {
    let target = TestTarget::new(Point2::new(0.0, 0.0), 0.5, 0.6);
    let origin = Point2::new(980.0, 990.0);
    for layer in 0..2 {
        let fine = target.render(layer, origin, Um(0.25), 512, 512);
        let coarse = target.render(layer, origin, Um(1.0), 128, 128);
        let averaged = cpu::downsample(&fine, 4).unwrap();
        let worst = averaged
            .data()
            .iter()
            .zip(coarse.data())
            .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(worst < 1e-5, "layer {layer}: {worst}");
    }
}
