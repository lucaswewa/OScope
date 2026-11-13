//! The H&E tissue: the same from the same seed however it is rendered, its
//! levels of detail, its nuclei's ground truth, and islands with background
//! between them.

use std::sync::Arc;

use oscope_compute::{Image, cpu};
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::{Field, HeTissue, Light, Optics, Sample, Slide, World};

const FOCUS: f64 = 1_000.0;

fn tissue(seed: u64) -> HeTissue {
    HeTissue::new(seed, Slide::COVERSLIP)
}

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

/// A world whose slide and stage coordinates agree.
fn world(tissue: HeTissue) -> World {
    World::with_slide(
        Slide::new(Arc::new(tissue), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

/// A field at 20×, focused on the section's middle, 2 µm up.
fn field(centre: Point2, width: u32, height: u32) -> Field {
    Field {
        centre,
        z: Um(FOCUS + 2.0),
        width,
        height,
    }
}

fn red(image: &Image) -> Vec<f32> {
    image.data().iter().step_by(3).copied().collect()
}

#[test]
fn the_same_seed_gives_the_same_image() {
    let centre = tissue(7).islands()[0];
    let render = |seed| {
        world(tissue(seed)).render(&field(centre, 96, 64), &optics("20x"), Light::Transmitted)
    };
    let first = render(7);
    assert_eq!(first.data(), render(7).data());
    let other = render(8);
    let differs = first
        .data()
        .iter()
        .zip(other.data())
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>();
    assert!(differs / first.data().len() as f32 > 0.01);
}

#[test]
fn a_field_s_half_is_the_field_of_its_half() {
    let t = tissue(3);
    let centre = t.islands()[0];
    let world = world(t);
    let o = optics("20x");
    let p = o.pixel_size().0;
    let whole = world.render(&field(centre, 128, 48), &o, Light::Transmitted);
    let left_centre = Point2::new(centre.x - 32.0 * p, centre.y);
    let left = world.render(&field(left_centre, 64, 48), &o, Light::Transmitted);
    for y in 0..48 {
        let (a, b) = (&whole.row(y)[..64 * 3], left.row(y));
        let worst = a
            .iter()
            .zip(b)
            .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(worst < 1e-5, "row {y}: {worst}");
    }
}

#[test]
fn a_coarse_rendering_is_close_to_a_fine_one_averaged_down() {
    let t = tissue(11);
    let centre = t.islands()[1];
    let origin = Point2::new(centre.x - 60.0, centre.y - 60.0);
    for layer in 0..2 {
        let fine = t.render(layer, origin, Um(0.12), 1000, 1000);
        let coarse = t.render(layer, origin, Um(0.48), 250, 250);
        let averaged = cpu::downsample(&fine, 4).unwrap();
        let (mut difference, mut total) = (0.0_f64, 0.0_f64);
        for (a, c) in averaged.data().iter().zip(coarse.data()) {
            difference += f64::from((a - c).abs());
            total += f64::from(*a);
        }
        let relative = difference / total;
        // Seeds 1, 5, 11, 42 and 99 measured 0.5–0.7%: nuclei and noise
        // are taken at the pixel's centre, with edges anti-aliased and
        // detail under half a pixel faded out, which is close to the
        // pixel's mean but not equal to it.
        assert!(relative < 0.015, "layer {layer}: {relative}");
    }
}

#[test]
fn each_nucleus_is_a_dark_spot_where_the_ground_truth_says() {
    let t = tissue(5);
    let o = optics("20x");
    let p = o.pixel_size().0;
    let centre = t.islands()[2];
    let (w, h) = (500_u32, 500_u32);
    let corner = Point2::new(
        centre.x - f64::from(w) * p / 2.0,
        centre.y - f64::from(h) * p / 2.0,
    );
    let region = Rect::new(corner.x, corner.y, f64::from(w) * p, f64::from(h) * p);
    let nuclei = t.nuclei(region);
    let image = world(t.clone()).render(&field(centre, w, h), &o, Light::Transmitted);
    let reds = red(&image);
    let mut sorted = reds.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];

    // About 60 µm square: between 2,000 and 9,000 nuclei a mm².
    let per_mm2 = nuclei.len() as f64 / (region.size.width * region.size.height / 1e6);
    assert!((2_000.0..9_000.0).contains(&per_mm2), "{per_mm2} a mm²");
    assert!(nuclei.iter().any(|n| n.layer == 0) && nuclei.iter().any(|n| n.layer == 1));
    let dark = nuclei
        .iter()
        .filter(|n| {
            let x = ((n.centre.x - corner.x) / p) as usize;
            let y = ((n.centre.y - corner.y) / p) as usize;
            reds[y * w as usize + x] < 0.6 * median
        })
        .count();
    assert!(
        dark as f64 >= 0.9 * nuclei.len() as f64,
        "{dark} of {} dark",
        nuclei.len()
    );
}

#[test]
fn the_tissue_lies_in_islands_with_background_between() {
    let t = tissue(1);
    let b = t.bounds();
    let pixel = 25.0;
    let (w, h) = (
        (b.size.width / pixel) as u32,
        (b.size.height / pixel) as u32,
    );
    let overview = t.render(0, b.origin, Um(pixel), w, h);
    let stained = overview
        .data()
        .iter()
        .step_by(3)
        .filter(|v| **v > 0.002)
        .count();
    let share = stained as f64 / f64::from(w * h);
    assert!(
        (0.05..0.6).contains(&share),
        "{share} of the coverslip is tissue"
    );
    assert!((3..=5).contains(&t.islands().len()));
}
