//! The reflective surface: depth from focus finds its height map; the
//! height map has the features it says; its metals shine, its chip is
//! opaque, and its seed decides its grain.

use std::sync::Arc;

use oscope_compute::Image;
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Field, Light, Optics, Sample, Slide, Surface, World};

const FOCUS: f64 = 1_000.0;
/// The chip's corner on the slide, whose corner is at the stage's origin.
const CHIP: Point2 = Point2::new(30_000.0, 10_000.0);

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

fn world(surface: &Surface) -> World {
    World::with_slide(
        Slide::new(Arc::new(surface.clone()), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

/// A die's centre, 1 mm into the chip.
fn die(surface: &Surface) -> Point2 {
    surface.die_centre(Point2::new(CHIP.x + 1_000.0, CHIP.y + 1_000.0))
}

fn render(
    world: &World,
    centre: Point2,
    z: f64,
    size: u32,
    optics: &Optics,
    light: Light,
) -> Image {
    let field = Field {
        centre,
        z: Um(FOCUS + z),
        width: size,
        height: size,
    };
    world.render(&field, optics, light)
}

/// The modified Laplacian's energy in green over a square of `tile`
/// pixels from `(x0, y0)`, with neighbours `k` pixels away: the focus
/// measure of Nayar and Nakagawa (*Shape from focus*, 1994).
fn focus_measure(image: &Image, x0: usize, y0: usize, tile: usize, k: usize) -> f64 {
    let w = image.width() as usize;
    let g = |x: usize, y: usize| f64::from(image.data()[(y * w + x) * 3 + 1]);
    let mut sum = 0.0;
    for y in y0.max(k)..(y0 + tile).min(w - k) {
        for x in x0.max(k)..(x0 + tile).min(w - k) {
            let lx = (2.0 * g(x, y) - g(x - k, y) - g(x + k, y)).abs();
            let ly = (2.0 * g(x, y) - g(x, y - k) - g(x, y + k)).abs();
            sum += (lx + ly).powi(2);
        }
    }
    sum
}

/// Depth from focus over a field 8 µm across at `centre`: for each tile
/// 1 µm or 16 pixels across, whichever is more, the z of its sharpest image, to a parabola through its
/// neighbours; and the error against the height map's mean over the tile.
fn depth_errors(surface: &Surface, objective: &str, centre: Point2) -> Vec<f64> {
    let (o, world) = (optics(objective), world(surface));
    let p = o.pixel_size().0;
    let size = (8.0 / p).round() as u32;
    let origin = Point2::new(
        centre.x - f64::from(size) * p / 2.0,
        centre.y - f64::from(size) * p / 2.0,
    );
    let truth = surface.height_map(origin, Um(p), size, size);
    let (lo, hi) = truth.iter().fold((f64::MAX, f64::MIN), |(l, h), v| {
        (l.min(f64::from(*v)), h.max(f64::from(*v)))
    });
    // Through the field's heights, and 1.5 µm beyond, in 0.25 µm steps.
    let step = 0.25;
    let zs: Vec<f64> = (0..)
        .map(|i| (lo - 1.5) + f64::from(i) * step)
        .take_while(|z| *z <= hi + 1.5)
        .collect();
    let stack: Vec<Image> = zs
        .iter()
        .map(|z| render(&world, centre, *z, size, &o, Light::Reflected))
        .collect();
    let (tile, k) = (
        ((1.0 / p).round() as usize).max(16),
        ((0.2 / p).round() as usize).max(1),
    );
    let n = size as usize;
    let mut errors = Vec::new();
    for ty in 0..n / tile {
        for tx in 0..n / tile {
            let sharp: Vec<f64> = stack
                .iter()
                .map(|image| focus_measure(image, tx * tile, ty * tile, tile, k))
                .collect();
            let i = (1..sharp.len() - 1)
                .max_by(|a, b| sharp[*a].total_cmp(&sharp[*b]))
                .unwrap();
            let (m, c, q) = (sharp[i - 1], sharp[i], sharp[i + 1]);
            let found = zs[i] + 0.5 * (m - q) / (m - 2.0 * c + q) * step;
            let heights: Vec<f64> = (ty * tile..(ty + 1) * tile)
                .flat_map(|y| (tx * tile..(tx + 1) * tile).map(move |x| (x, y)))
                .map(|(x, y)| f64::from(truth[y * n + x]))
                .collect();
            let mean = heights.iter().sum::<f64>() / heights.len() as f64;
            errors.push(found - mean);
        }
    }
    errors
}

#[test]
fn the_sharpest_z_matches_the_height_map() {
    // The dome's flank 25 µm from its top, the ridge's rising slope, a
    // terrace, and the
    // aluminium film between its pits, at 50×; the dome and the ridge at
    // 20×.
    let surface = Surface::new(7, CHIP);
    let d = die(&surface);
    let places = [
        ("50x", "dome", Point2::new(d.x + 25.0, d.y - 50.0)),
        ("50x", "ridge", Point2::new(d.x - 65.0, d.y + 35.0)),
        ("50x", "terrace", Point2::new(d.x - 50.0, d.y - 50.0)),
        ("50x", "film", Point2::new(d.x + 50.0, d.y + 50.0)),
        ("20x", "dome", Point2::new(d.x + 25.0, d.y - 50.0)),
        ("20x", "ridge", Point2::new(d.x - 65.0, d.y + 35.0)),
    ];
    for (objective, name, centre) in places {
        let errors = depth_errors(&surface, objective, centre);
        let rms = (errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64).sqrt();
        let worst = errors.iter().fold(0.0_f64, |m, e| m.max(e.abs()));
        assert!(
            rms < 0.05 && worst < 0.15,
            "{objective} {name}: {} tiles, rms {rms:.3} µm, worst {worst:.3} µm",
            errors.len()
        );
    }
}

#[test]
fn the_height_map_has_its_features() {
    let surface = Surface::new(7, CHIP);
    let d = die(&surface);
    let h = |du: f64, dv: f64| surface.height(Point2::new(d.x + du, d.y + dv)).unwrap().0;
    // The waviness, at most 0.075 µm either way over features 7.5 µm and
    // more across, changes less than 0.02 µm in a micrometre: each step is
    // checked across its edge.
    let step = |du: f64, dv: f64, along: (f64, f64)| {
        h(du + along.0 / 2.0, dv + along.1 / 2.0) - h(du - along.0 / 2.0, dv - along.1 / 2.0)
    };
    // The terraces' edges at 10, 25 and 40 µm into the quadrant, up 1 µm,
    // and at 60, 75 and 90, down.
    for (edge, rise) in [
        (10.0, 1.0),
        (25.0, 1.0),
        (40.0, 1.0),
        (60.0, -1.0),
        (75.0, -1.0),
        (90.0, -1.0),
    ] {
        let found = step(edge - 100.0, -50.0, (1.0, 0.0));
        assert!(
            (found - rise).abs() < 0.02,
            "the terrace at {edge}: {found}"
        );
    }
    // A pit's edge, 5 µm from its centre: 1.5 µm down from the film.
    assert!((step(55.0, 60.0, (1.0, 0.0)) + 1.5).abs() < 0.02);
    // The dome and the ridge, 3 and 4 µm above their feet, which are 40
    // and 50 µm away: within the waviness's range.
    assert!((h(50.0, -50.0) - h(90.0, -50.0) - 3.0).abs() < 0.16);
    assert!((h(-40.0, 50.0) - h(-90.0, 50.0) - 4.0).abs() < 0.16);
    // The tilt, over 3 mm: 0.6 µm along x, 0.3 along y.
    let along_x = h(2_950.0, -50.0) - h(-50.0, -50.0);
    let along_y = h(-50.0, 2_950.0) - h(-50.0, -50.0);
    assert!(
        (along_x - 0.6).abs() < 0.16 && (along_y - 0.3).abs() < 0.16,
        "{along_x} {along_y}"
    );
    // Off the chip, no surface.
    assert!(surface.height(Point2::new(CHIP.x - 10.0, CHIP.y)).is_none());
}

#[test]
fn metal_shines_and_the_chip_is_opaque() {
    let surface = Surface::new(7, CHIP);
    let (d, o, world) = (die(&surface), optics("20x"), world(&surface));
    let mean_green = |image: &Image| {
        let g: Vec<f64> = image
            .data()
            .iter()
            .skip(1)
            .step_by(3)
            .map(|v| f64::from(*v))
            .collect();
        g.iter().sum::<f64>() / g.len() as f64
    };
    let at = |du: f64, dv: f64, light| {
        let centre = Point2::new(d.x + du, d.y + dv);
        let z = surface.height(centre).unwrap().0;
        mean_green(&render(&world, centre, z, 32, &o, light))
    };
    // In focus, aluminium reflects about 0.92 of the light, and silicon
    // 0.37, each dimmed by the grain, by 0.6 at most; the slide's glass
    // adds 0.04.
    let (metal, silicon) = (
        at(50.0, 50.0, Light::Reflected),
        at(-50.0, -50.0, Light::Reflected),
    );
    assert!((0.04 + 0.6 * 0.92..0.04 + 0.92).contains(&metal), "{metal}");
    assert!(
        (0.04 + 0.6 * 0.367..0.04 + 0.367).contains(&silicon),
        "{silicon}"
    );
    // Transmitted light: the chip stops it; the bare slide beside it passes
    // it.
    assert!(at(-50.0, -50.0, Light::Transmitted) < 1e-3);
    let beside = Point2::new(CHIP.x - 100.0, CHIP.y + 1_000.0);
    assert!(mean_green(&render(&world, beside, 0.0, 32, &o, Light::Transmitted)) > 0.8);
}

#[test]
fn the_same_seed_gives_the_same_surface() {
    let o = optics("50x");
    let p = o.pixel_size();
    let layer = |seed: u64| {
        let surface = Surface::new(seed, CHIP);
        let d = die(&surface);
        // The slab nearest the terrace's height.
        let z = surface
            .height(Point2::new(d.x - 50.0, d.y - 50.0))
            .unwrap()
            .0;
        let index = 1 + (z / Surface::SLAB.0).round() as usize;
        let origin = Point2::new(d.x - 55.0, d.y - 55.0);
        surface.render(index, origin, p, 64, 64).data().to_vec()
    };
    assert_eq!(layer(7), layer(7));
    assert_ne!(layer(7), layer(8));
}
