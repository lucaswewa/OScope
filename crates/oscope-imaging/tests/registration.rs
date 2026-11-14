//! Registration on the CPU (P31): known shifts, sharp and blurred, with
//! noise, recovered within 0.1 px; low confidence for a featureless pair
//! and a barely overlapping one; the expected shift's search window; the
//! batch; and the refusals.

mod field;

use field::{Field, noisy};
use oscope_compute::{Image, Shape};
use oscope_imaging::registration::{
    RegisterOptions, Registration, RegistrationError, register, register_batch,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// The windows' size, and where A's lies in the 512-pixel field.
const SIZE: (usize, usize) = (256, 256);
const AT: (usize, usize) = (128, 128);

/// Shifts from none to a few dozen pixels, whole and sub-pixel.
const SHIFTS: [(f64, f64); 10] = [
    (0.0, 0.0),
    (1.0, -3.0),
    (0.1, -0.2),
    (0.25, 0.3),
    (0.4, -0.45),
    (0.5, 0.5),
    (-0.6, 0.65),
    (3.3, -2.7),
    (-7.8, 5.15),
    (12.45, -29.9),
];

/// A pair from `field`: A unshifted, B shifted by `shift`, each with its own
/// noise at `snr`.
fn pair(field: &Field, shift: (f64, f64), snr: f64, random: &mut StdRng) -> (Image, Image) {
    let a = noisy(field.window((0.0, 0.0), AT, SIZE), SIZE, snr, random);
    let b = noisy(field.window(shift, AT, SIZE), SIZE, snr, random);
    (a, b)
}

fn error(found: &Registration, shift: (f64, f64)) -> f64 {
    (found.dx - shift.0).abs().max((found.dy - shift.1).abs())
}

#[test]
fn known_shifts_are_recovered_within_a_tenth_of_a_pixel_sharp_or_blurred() {
    let mut random = StdRng::seed_from_u64(31);
    // Sharp; blurred as the 5× and 10× blur; and as the 20× blurs.
    for blur in [0.0, 1.7, 2.3] {
        let field = Field::new(512, blur, 7);
        for snr in [f64::INFINITY, 4.0] {
            for shift in SHIFTS {
                let (a, b) = pair(&field, shift, snr, &mut random);
                let found = register(&a, &b, &RegisterOptions::default()).unwrap();
                assert!(
                    error(&found, shift) <= 0.1,
                    "blur {blur}, {shift:?}: {found:?}"
                );
                assert!(found.confidence >= 0.9, "blur {blur}, {shift:?}: {found:?}");
            }
        }
    }
}

#[test]
fn a_featureless_pair_scores_low_and_a_good_pair_high() {
    let mut random = StdRng::seed_from_u64(32);
    for blur in [0.0, 1.7] {
        // Two unrelated fields: nothing in common but their statistics.
        let a = noisy(
            Field::new(256, blur, 1).window((0.0, 0.0), (0, 0), SIZE),
            SIZE,
            4.0,
            &mut random,
        );
        let b = noisy(
            Field::new(256, blur, 2).window((0.0, 0.0), (0, 0), SIZE),
            SIZE,
            4.0,
            &mut random,
        );
        let found = register(&a, &b, &RegisterOptions::default()).unwrap();
        assert!(
            found.confidence < 0.35 && found.peak < 0.15,
            "blur {blur}: {found:?}"
        );
        let (a, b) = pair(&Field::new(512, blur, 3), (4.5, -6.25), 4.0, &mut random);
        let good = register(&a, &b, &RegisterOptions::default()).unwrap();
        assert!(
            good.confidence > 0.9 && good.peak > 0.5,
            "blur {blur}: {good:?}"
        );
    }
}

#[test]
fn a_barely_overlapping_pair_scores_low() {
    let mut random = StdRng::seed_from_u64(33);
    let field = Field::new(512, 1.7, 4);
    // B's content lies 230 px to the right of A's: they share 26 columns,
    // at the edges the window tapers.
    for shift in [(230.0, 0.0), (0.0, -230.0), (200.0, 200.0)] {
        let (a, b) = pair(&field, shift, 4.0, &mut random);
        let found = register(&a, &b, &RegisterOptions::default()).unwrap();
        assert!(
            found.confidence < 0.35 && found.peak < 0.15,
            "{shift:?}: {found:?}"
        );
    }
}

#[test]
fn the_expected_shift_finds_the_true_peak_where_a_stronger_false_one_lies_outside() {
    let mut random = StdRng::seed_from_u64(34);
    let field = Field::new(512, 1.7, 5);
    let (truth, lure) = ((3.4, -2.2), (-40.0, 25.5));
    // B holds A's content twice: faintly at the true shift, strongly at
    // the false one.
    let a = noisy(field.window((0.0, 0.0), AT, SIZE), SIZE, 4.0, &mut random);
    let faint = field.window(truth, AT, SIZE);
    let strong = field.window(lure, AT, SIZE);
    let mixed = faint
        .iter()
        .zip(&strong)
        .map(|(f, s)| 0.35 * f + 0.65 * s)
        .collect();
    let b = noisy(mixed, SIZE, 4.0, &mut random);
    let anywhere = register(&a, &b, &RegisterOptions::default()).unwrap();
    assert!(error(&anywhere, lure) <= 0.1, "{anywhere:?}");
    let near = RegisterOptions {
        expected: Some((3.0, -2.0)),
        radius: 12,
        ..RegisterOptions::default()
    };
    let found = register(&a, &b, &near).unwrap();
    assert!(error(&found, truth) <= 0.1, "{found:?}");
    assert!(
        found.confidence > anywhere.confidence,
        "{found:?}, {anywhere:?}"
    );
}

#[test]
fn an_integer_shift_round_the_frame_is_exact_without_the_window() {
    let mut random = StdRng::seed_from_u64(35);
    let field = Field::new(256, 1.0, 6);
    let a = noisy(
        field.window((0.0, 0.0), (0, 0), SIZE),
        SIZE,
        f64::INFINITY,
        &mut random,
    );
    // The whole field, rolled: B's content wraps round, as the transform
    // assumes, so the cross-power spectrum is the shift's phase alone.
    let b = noisy(
        field.window((5.0, -7.0), (0, 0), SIZE),
        SIZE,
        f64::INFINITY,
        &mut random,
    );
    let options = RegisterOptions {
        window: false,
        ..RegisterOptions::default()
    };
    let found = register(&a, &b, &options).unwrap();
    assert!(error(&found, (5.0, -7.0)) < 1e-5, "{found:?}");
    assert!(found.confidence > 0.99 && found.peak > 0.99, "{found:?}");
}

#[test]
fn a_batch_registers_each_pair_as_if_alone() {
    let mut random = StdRng::seed_from_u64(36);
    let field = Field::new(512, 1.7, 8);
    let pairs: Vec<(Image, Image)> = [(2.5, -1.25), (-10.0, 3.75), (0.0, 0.0)]
        .into_iter()
        .map(|shift| pair(&field, shift, 4.0, &mut random))
        .collect();
    let refs: Vec<(&Image, &Image)> = pairs.iter().map(|(a, b)| (a, b)).collect();
    let options = RegisterOptions::default();
    let batch = register_batch(&refs, &options).unwrap();
    let alone: Vec<Registration> = refs
        .iter()
        .map(|(a, b)| register(a, b, &options).unwrap())
        .collect();
    assert_eq!(batch, alone);
}

#[test]
fn unsuitable_images_and_options_are_refused() {
    let grey = |w: u32, h: u32| Image::zeros(Shape::new(w, h, 1));
    let options = RegisterOptions::default();
    let refused = |result: Result<Vec<Registration>, RegistrationError>, says: &str| {
        let error = result.unwrap_err().to_string();
        assert!(error.contains(says), "{error:?} doesn't say {says:?}");
    };
    let (a, b) = (grey(64, 48), grey(64, 48));
    refused(register_batch(&[], &options), "no pairs");
    refused(
        register_batch(&[(&a, &grey(48, 64))], &options),
        "48 × 64 × 1 among",
    );
    let colour = Image::zeros(Shape::new(64, 48, 3));
    refused(
        register_batch(&[(&colour, &colour)], &options),
        "3 channels",
    );
    refused(
        register_batch(&[(&grey(7, 64), &grey(7, 64))], &options),
        "at least 8 × 8",
    );
    for sigma in [0.5, f64::NAN, f64::INFINITY] {
        let options = RegisterOptions { sigma, ..options };
        refused(register_batch(&[(&a, &b)], &options), "at least 1");
    }
    let nowhere = RegisterOptions {
        expected: Some((f64::NAN, 0.0)),
        ..options
    };
    refused(register_batch(&[(&a, &b)], &nowhere), "an expected shift");
    let mut data = vec![0.0; 64 * 48];
    data[100] = f32::NAN;
    let broken = Image::from_vec(Shape::new(64, 48, 1), data).unwrap();
    refused(register_batch(&[(&a, &broken)], &options), "isn't finite");
}
