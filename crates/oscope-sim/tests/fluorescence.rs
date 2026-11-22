//! Simulated fluorescence: excitation and the dyes' colours, widefield
//! against confocal sectioning on a thin sheet and on the cell culture,
//! and each light seeing only its own layers.

use std::sync::Arc;

use oscope_compute::Image;
use oscope_core::{Point2, Rect, Um};
use oscope_hal::ObjectiveCatalogue;
use oscope_sim::fluorescence::{optical_section, sectioning};
use oscope_sim::sample::render_pixels;
use oscope_sim::{
    CellCulture, Dye, Excitation, Field, Layer, LayerKind, Light, Optics, Sample, Slide,
    TestTarget, World,
};

const FOCUS: f64 = 1_000.0;

/// A thin sheet of one dye, `amount` everywhere, or left of `edge` only,
/// `z` above the slide.
struct Sheet {
    layers: Vec<Layer>,
    amount: f32,
    edge: Option<f64>,
}

impl Sheet {
    fn new(dye: Dye, z: f64, amount: f32) -> Self {
        Self {
            layers: vec![Layer {
                name: "sheet".into(),
                z: Um(z),
                thickness: Um(0.1),
                kind: LayerKind::Fluorescent(dye),
            }],
            amount,
            edge: None,
        }
    }
}

impl Sample for Sheet {
    fn name(&self) -> &str {
        "sheet"
    }
    fn layers(&self) -> &[Layer] {
        &self.layers
    }
    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, 75_000.0, 25_000.0)
    }
    fn render(&self, _: usize, origin: Point2, pixel: Um, width: u32, height: u32) -> Image {
        // Each pixel holds the share of it left of the edge.
        let p = pixel.0;
        render_pixels(origin, pixel, width, height, |x0, _| {
            let share = self.edge.map_or(1.0, |e| ((e - x0) / p).clamp(0.0, 1.0));
            [self.amount * share as f32; 3]
        })
    }
}

fn optics(name: &str) -> Optics {
    let catalogue = ObjectiveCatalogue::mitutoyo();
    Optics::new(&catalogue, catalogue.get(name).unwrap(), Um(2.4))
}

fn world(sample: impl Sample + 'static) -> World {
    World::with_slide(
        Slide::new(Arc::new(sample), Point2::new(0.0, 0.0)),
        Um(FOCUS),
    )
}

/// The mean of each channel over a field 32 pixels square at `centre`,
/// with the stage at `z` above focus.
fn mean(
    world: &World,
    centre: Point2,
    z: f64,
    size: u32,
    optics: &Optics,
    light: Light,
) -> [f64; 3] {
    let field = Field {
        centre,
        z: Um(FOCUS + z),
        width: size,
        height: size,
    };
    let image = world.render(&field, optics, light);
    let mut sums = [0.0; 3];
    for (i, v) in image.data().iter().enumerate() {
        sums[i % 3] += f64::from(*v);
    }
    sums.map(|s| s / f64::from(size * size))
}

fn widefield(line: Excitation) -> Light {
    Light::Fluorescence {
        line,
        confocal: false,
    }
}

fn confocal(line: Excitation) -> Light {
    Light::Fluorescence {
        line,
        confocal: true,
    }
}

#[test]
fn each_dye_shines_in_its_colour_under_its_line() {
    let o = optics("20x");
    let at = Point2::new(40_000.0, 12_000.0);
    // Each dye under the line that excites it best: the colour it gives on
    // the sensor, brightest in the channel of its emission.
    let cases = [
        (Dye::Dapi, Excitation::Violet, 2),
        (Dye::Green, Excitation::Blue, 1),
        (Dye::Orange, Excitation::Green, 0),
        (Dye::FarRed, Excitation::Red, 0),
    ];
    for (dye, line, brightest) in cases {
        let rgb = mean(
            &world(Sheet::new(dye, 2.0, 0.5)),
            at,
            2.0,
            32,
            &o,
            widefield(line),
        );
        let expected = dye.colour().map(|c| 0.5 * c * dye.excited_by(line));
        for c in 0..3 {
            assert!(
                (rgb[c] - expected[c]).abs() < 1e-5,
                "{dye:?}: {rgb:?} against {expected:?}"
            );
        }
        let top = (0..3).max_by(|a, b| rgb[*a].total_cmp(&rgb[*b])).unwrap();
        assert_eq!(top, brightest, "{dye:?} under {line:?}: {rgb:?}");
    }
    // A line far from a dye's excitation hardly excites it.
    assert!(Dye::Dapi.excited_by(Excitation::Green) < 1e-3);
    assert!(Dye::Orange.excited_by(Excitation::Violet) < 1e-3);
    assert!(Dye::Green.excited_by(Excitation::Blue) > 0.9);
    // The far red sits past the infrared-cut filter: dim on a colour sensor.
    assert!(Dye::FarRed.colour()[0] < 0.25 * Dye::Orange.colour()[0]);
}

#[test]
fn each_dye_blurs_at_its_own_wavelength() {
    // A straight edge of each dye, in focus at 50×: diffraction blurs it
    // by a Gaussian of 0.21 λ / NA at the dye's emission, which rises from
    // 10% to 90% over 2.563 σ. The far red's edge is 1.44 times DAPI's.
    let o = optics("50x");
    let p = o.pixel_size().0;
    let edge = 40_000.0;
    for (dye, line) in [
        (Dye::Dapi, Excitation::Violet),
        (Dye::FarRed, Excitation::Red),
    ] {
        let sheet = Sheet {
            edge: Some(edge),
            ..Sheet::new(dye, 2.0, 0.5)
        };
        let field = Field {
            centre: Point2::new(edge, 12_000.0),
            z: Um(FOCUS + 2.0),
            width: 96,
            height: 8,
        };
        let image = world(sheet).render(&field, &o, widefield(line));
        let c = dye
            .colour()
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        let row: Vec<f64> = (0..96)
            .map(|x| f64::from(image.data()[(4 * 96 + x) * 3 + c]))
            .collect();
        let full = row[0];
        // Where the falling edge crosses a share of the full level.
        let crossing = |share: f64| {
            let level = share * full;
            let i = row.iter().position(|v| *v < level).unwrap();
            (i - 1) as f64 + (row[i - 1] - level) / (row[i - 1] - row[i])
        };
        let width = (crossing(0.1) - crossing(0.9)) * p;
        let expected = 2.563 * 0.21 * dye.peaks().1.0 / o.na;
        assert!(
            (width / expected - 1.0).abs() < 0.03,
            "{dye:?}: {width} µm against {expected}"
        );
    }
}

#[test]
fn widefield_keeps_a_sheet_s_light_and_a_confocal_sections_it() {
    // A uniform sheet: widefield blurs it, and loses none of it, at any
    // distance from focus; a confocal passes 1 / (1 + (2 dz / section)²).
    let o = optics("50x");
    let section = optical_section(&o, Dye::Green.peaks().1).0;
    let sheet = world(Sheet::new(Dye::Green, 2.0, 0.5));
    let at = Point2::new(40_000.0, 12_000.0);
    let line = Excitation::Blue;
    let in_focus = mean(&sheet, at, 2.0, 32, &o, widefield(line))[1];
    for dz in [0.0, 1.0, section / 2.0, 5.0, 3.0 * section, 30.0] {
        let wide = mean(&sheet, at, 2.0 + dz, 32, &o, widefield(line))[1];
        let conf = mean(&sheet, at, 2.0 + dz, 32, &o, confocal(line))[1];
        assert!(
            (wide / in_focus - 1.0).abs() < 1e-5,
            "dz {dz}: widefield {wide}"
        );
        let kept = 1.0 / (1.0 + (2.0 * dz / section).powi(2));
        assert!(
            (conf / wide - kept).abs() < 1e-5,
            "dz {dz}: {conf} / {wide}"
        );
    }
    // Half the light at half the section; a tenth at 1.5 sections.
    assert!((sectioning(Um(section / 2.0), Um(section)) - 0.5).abs() < 1e-12);
    assert!((sectioning(Um(1.5 * section), Um(section)) - 0.1).abs() < 1e-12);
}

#[test]
fn the_optical_section_follows_the_aperture() {
    // About 4 µm through the instrument's 50×, 0.55 in air; under 1 µm
    // through an oil objective of 1.4, as confocal microscopes use.
    let dry = optical_section(&optics("50x"), Um(0.52)).0;
    assert!((3.5..4.5).contains(&dry), "{dry}");
    let oil = Optics {
        na: 1.4,
        medium: 1.518,
        ..optics("50x")
    };
    let wet = optical_section(&oil, Um(0.52)).0;
    assert!((0.5..1.0).contains(&wet), "{wet}");
}

#[test]
fn a_confocal_cuts_the_light_from_out_of_focus_cells() {
    // A field over the culture, in focus through the cells' middles, and
    // focused 8 µm above them: widefield still collects most of their
    // light, blurred; the confocal very little of it.
    let culture = CellCulture::new(3, Rect::new(20_000.0, 5_000.0, 1_000.0, 1_000.0));
    let centre = culture.cells(Rect::new(20_400.0, 5_400.0, 200.0, 200.0))[0].centre;
    let world = world(culture);
    let o = optics("50x");
    let line = Excitation::Blue;
    let [wide_in, wide_out] =
        [2.5, 14.0].map(|z| mean(&world, centre, z, 128, &o, widefield(line))[1]);
    let [conf_in, conf_out] =
        [2.5, 14.0].map(|z| mean(&world, centre, z, 128, &o, confocal(line))[1]);
    assert!(
        wide_in > 0.0 && conf_in > 0.5 * wide_in,
        "{conf_in} {wide_in}"
    );
    assert!(
        wide_out > 0.7 * wide_in,
        "widefield: {wide_out} out of focus, {wide_in} in"
    );
    assert!(
        conf_out < 0.1 * conf_in,
        "confocal: {conf_out} out of focus, {conf_in} in"
    );
    assert!(conf_out < 0.1 * wide_out);
}

#[test]
fn each_light_sees_only_its_own_layers() {
    let o = optics("20x");
    let at = Point2::new(30_100.0, 10_100.0);
    // The test target's stains and bars give no fluorescence.
    let target = world(TestTarget::new(Point2::new(30_000.0, 10_000.0), 0.5, 0.5));
    for light in [widefield(Excitation::Blue), confocal(Excitation::Violet)] {
        assert_eq!(mean(&target, at, 0.0, 32, &o, light), [0.0; 3]);
    }
    // A dye neither absorbs transmitted light nor reflects: the slide's
    // glass alone, 0.96 for each of its four surfaces under the coverslip.
    let sheet = world(Sheet::new(Dye::Orange, 2.0, 0.5));
    let glass = 0.96_f64.powi(4);
    let seen = mean(
        &sheet,
        Point2::new(40_000.0, 12_000.0),
        2.0,
        32,
        &o,
        Light::Transmitted,
    );
    assert!(seen.iter().all(|v| (v - glass).abs() < 1e-5), "{seen:?}");
    let reflected = mean(
        &sheet,
        Point2::new(40_000.0, 12_000.0),
        2.0,
        32,
        &o,
        Light::Reflected,
    );
    assert!(
        reflected.iter().all(|v| (v - 0.04).abs() < 1e-5),
        "{reflected:?}"
    );
}
