//! Imported images and stacks: where they land and at what scale, what
//! their values mean, their pages' depths, and what is refused.

use std::path::Path;
use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_core::{Point2, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_imaging::color::encode_png;
use oscope_sim::{
    Dye, Excitation, Field, ImportLight, Imported, Light, Optics, Sample, Slide, World,
};
use tiff::encoder::{TiffEncoder, colortype};

const FOCUS: f64 = 1_000.0;
/// The glass under the coverslip: four surfaces, each passing 96%.
const GLASS: f64 = 0.849_346_56;

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

fn world(sample: Imported) -> World {
    World::with_slide(
        Slide::new(Arc::new(sample), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

fn render(world: &World, centre: Point2, z: f64, size: (u32, u32), light: Light) -> Image {
    let field = Field {
        centre,
        z: Um(FOCUS + z),
        width: size.0,
        height: size.1,
    };
    world.render(&field, &optics("20x"), light)
}

/// Writes a white 8-bit PNG, `w` × `h`, black where `dark` says.
fn write_png(path: &Path, w: u32, h: u32, dark: impl Fn(u32, u32) -> bool) {
    let rgb: Vec<u8> = (0..w * h)
        .flat_map(|i| [if dark(i % w, i / w) { 0 } else { 255 }; 3])
        .collect();
    let mut file = Vec::new();
    encode_png(&mut file, &rgb, w, h).unwrap();
    std::fs::write(path, file).unwrap();
}

/// Where a row crosses halfway between its darkest and brightest, in
/// order, each to a fraction of a pixel.
fn crossings(row: &[f64]) -> Vec<f64> {
    let (lo, hi) = row
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    let level = (lo + hi) / 2.0;
    (1..row.len())
        .filter(|i| (row[*i] >= level) != (row[i - 1] >= level))
        .map(|i| (i - 1) as f64 + (level - row[i - 1]) / (row[i] - row[i - 1]))
        .collect()
}

#[test]
fn an_image_lands_at_its_corner_and_scale() {
    // 100 × 60 pixels of 0.5 µm from (40,000, 12,000) µm, black from pixel
    // 20 to 40 across: from 40,010 to 40,020 µm on the slide.
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("rectangle.png");
    write_png(&path, 100, 60, |x, y| {
        (20..40).contains(&x) && (10..30).contains(&y)
    });
    let corner = Point2::new(40_000.0, 12_000.0);
    let imported =
        Imported::read(&path, Um(0.5), Um(0.0), corner, ImportLight::Transmitted).unwrap();
    let bounds = imported.bounds();
    assert_eq!(
        (bounds.origin, bounds.size.width, bounds.size.height),
        (corner, 50.0, 30.0)
    );
    // At 20×, in focus, across the rectangle's middle row.
    let p = optics("20x").pixel_size().0;
    let centre = Point2::new(40_015.0, 12_010.0);
    let image = render(&world(imported), centre, 0.0, (192, 16), Light::Transmitted);
    let row: Vec<f64> = (0..192)
        .map(|x| f64::from(image.data()[(8 * 192 + x) * 3 + 1]))
        .collect();
    let found = crossings(&row);
    assert_eq!(found.len(), 2, "{found:?}");
    let at = |column: f64| centre.x - 96.0 * p + (column + 0.5) * p;
    let (left, right) = (at(found[0]), at(found[1]));
    assert!((left - 40_010.0).abs() < 0.05, "{left}");
    assert!((right - 40_020.0).abs() < 0.05, "{right}");
}

#[test]
fn a_coarse_field_of_an_import_is_a_fine_one_averaged_down() {
    // Each output pixel is the mean of the image pixels under it: 1 µm
    // pixels are the means of four by four 0.25 µm ones, at any offset.
    let pattern: Vec<f32> = (0..40 * 40 * 3)
        .map(|i| ((i * 37) % 101) as f32 / 100.0)
        .collect();
    let page = Image::from_vec(Shape::new(40, 40, 3), pattern).unwrap();
    let corner = Point2::new(100.0, 200.0);
    let imported = Imported::new(
        "pattern",
        vec![page],
        Um(0.7),
        Um(0.0),
        corner,
        ImportLight::Reflected,
    )
    .unwrap();
    let origin = Point2::new(103.3, 201.9);
    let fine = imported.render(0, origin, Um(0.25), 64, 64);
    let coarse = imported.render(0, origin, Um(1.0), 16, 16);
    for (i, v) in coarse.data().iter().enumerate() {
        let (x, y, c) = ((i / 3) % 16, i / 48, i % 3);
        let mean: f32 = (0..16)
            .map(|k| fine.data()[((y * 4 + k / 4) * 64 + x * 4 + k % 4) * 3 + c])
            .sum::<f32>()
            / 16.0;
        assert!((v - mean).abs() < 1e-5, "({x}, {y}): {v} against {mean}");
    }
}

#[test]
fn an_import_s_values_mean_what_its_light_says() {
    // A grey of 0.25 everywhere, in each of the three meanings.
    let grey =
        || vec![Image::from_vec(Shape::new(200, 200, 3), vec![0.25; 200 * 200 * 3]).unwrap()];
    let corner = Point2::new(40_000.0, 12_000.0);
    let centre = Point2::new(40_100.0, 12_100.0);
    let mean = |light: ImportLight, seen: Light| {
        let imported = Imported::new("grey", grey(), Um(1.0), Um(0.0), corner, light).unwrap();
        let image = render(&world(imported), centre, 0.0, (32, 32), seen);
        image
            .data()
            .iter()
            .skip(1)
            .step_by(3)
            .map(|v| f64::from(*v))
            .sum::<f64>()
            / 1024.0
    };
    // Brightfield passes a quarter, through the glass.
    let transmitted = mean(ImportLight::Transmitted, Light::Transmitted);
    assert!((transmitted - 0.25 * GLASS).abs() < 1e-4, "{transmitted}");
    // Reflected light: a quarter, and the glass's 4%.
    let reflected = mean(ImportLight::Reflected, Light::Reflected);
    assert!((reflected - 0.29).abs() < 1e-4, "{reflected}");
    // Fluorescence: a quarter of full brightness's amount of the dye.
    let line = Excitation::Blue;
    let fluorescent = mean(
        ImportLight::Fluorescent(Dye::Green),
        Light::Fluorescence {
            line,
            confocal: false,
        },
    );
    let expected = 0.25
        * f64::from(Imported::FLUORESCENCE)
        * Dye::Green.colour()[1]
        * Dye::Green.excited_by(line);
    assert!(
        (fluorescent - expected).abs() < 1e-5,
        "{fluorescent} against {expected}"
    );
}

#[test]
fn a_stack_s_pages_are_its_z_step_apart() {
    // Three pages 2 µm apart, each with a bright square in its own place:
    // focused on a page's depth, its square's edge is the sharpest.
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("stack.tif");
    let (w, h) = (120_u32, 40_u32);
    let square =
        |k: u32, x: u32, y: u32| (10 + 40 * k..30 + 40 * k).contains(&x) && (10..30).contains(&y);
    {
        let mut file = std::fs::File::create(&path).unwrap();
        let mut encoder = TiffEncoder::new(&mut file).unwrap();
        for k in 0..3 {
            let page: Vec<u16> = (0..w * h)
                .map(|i| if square(k, i % w, i / w) { 60_000 } else { 0 })
                .collect();
            encoder
                .write_image::<colortype::Gray16>(w, h, &page)
                .unwrap();
        }
    }
    let corner = Point2::new(40_000.0, 12_000.0);
    let light = ImportLight::Fluorescent(Dye::Green);
    let stack = Imported::read(&path, Um(0.25), Um(2.0), corner, light).unwrap();
    let depths: Vec<f64> = stack.layers().iter().map(|l| l.z.0).collect();
    assert_eq!(depths, [0.0, 2.0, 4.0]);
    let world = world(stack);
    let p = optics("20x").pixel_size().0;
    let seen = Light::Fluorescence {
        line: Excitation::Blue,
        confocal: false,
    };
    // Each square's left edge, 10 pixels of 0.25 µm into its 40.
    let edges: Vec<f64> = (0..3)
        .map(|k| corner.x + f64::from(10 + 40 * k) * 0.25)
        .collect();
    for focus in 0..3 {
        let image = render(
            &world,
            Point2::new(40_015.0, 12_005.0),
            2.0 * f64::from(focus),
            (256, 8),
            seen,
        );
        let steepest = |edge: f64| {
            let x = ((edge - (40_015.0 - 128.0 * p)) / p) as usize;
            (x - 10..x + 10)
                .map(|i| {
                    (image.data()[(4 * 256 + i + 1) * 3 + 1] - image.data()[(4 * 256 + i) * 3 + 1])
                        .abs()
                })
                .fold(0.0_f32, f32::max)
        };
        let slopes: Vec<f32> = edges.iter().map(|e| steepest(*e)).collect();
        let sharpest = (0..3)
            .max_by(|a, b| slopes[*a].total_cmp(&slopes[*b]))
            .unwrap();
        assert_eq!(
            sharpest,
            focus as usize,
            "focused at {} µm: {slopes:?}",
            2 * focus
        );
    }
}

#[test]
fn bad_imports_are_refused() {
    let page = |w, h| Image::zeros(Shape::new(w, h, 3));
    let corner = Point2::new(0.0, 0.0);
    let light = ImportLight::Transmitted;
    assert!(Imported::new("none", vec![], Um(1.0), Um(0.0), corner, light).is_err());
    assert!(
        Imported::new(
            "mixed",
            vec![page(4, 4), page(4, 5)],
            Um(1.0),
            Um(1.0),
            corner,
            light
        )
        .is_err()
    );
    assert!(
        Imported::new(
            "no scale",
            vec![page(4, 4)],
            Um(0.0),
            Um(0.0),
            corner,
            light
        )
        .is_err()
    );
    let missing = Path::new("no-such-file.png");
    let error = Imported::read(missing, Um(1.0), Um(0.0), corner, light).unwrap_err();
    assert!(error.contains("no-such-file.png"), "{error}");
}
