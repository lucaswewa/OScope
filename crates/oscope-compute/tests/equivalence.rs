//! The equivalence helper: tolerances, and where the largest error is.

use oscope_compute::equivalence::{
    Position, Tolerance, assert_equivalent, compare, compare_images,
};
use oscope_compute::{ComputeError, Image, Shape};

fn image(width: u32, height: u32, channels: usize, data: Vec<f32>) -> Image {
    Image::from_vec(Shape::new(width, height, channels), data).unwrap()
}

#[test]
fn equal_images_pass_with_no_error() {
    let a = image(4, 3, 2, (0..24).map(|v| v as f32).collect());
    let comparison = compare_images(&a, &a.clone(), Tolerance::EXACT).unwrap();
    assert!(comparison.passed());
    assert_eq!(
        (comparison.compared, comparison.failed, comparison.max_error),
        (24, 0, 0.0)
    );
    assert_eq!(comparison.worst, None);
    assert!(comparison.to_string().contains("every sample is equal"));
}

#[test]
fn the_largest_error_is_reported_with_its_position_and_values() {
    let expected = image(4, 3, 2, vec![1.0; 24]);
    let mut actual = expected.clone();
    // x 3, y 2, channel 1 is the last sample; x 1, y 0, channel 0 is the third.
    actual.data_mut()[23] = 1.5;
    actual.data_mut()[2] = 1.25;
    let comparison = compare_images(&expected, &actual, Tolerance::abs(0.3)).unwrap();
    assert_eq!(comparison.failed, 1);
    assert_eq!(comparison.max_error, 0.5);
    let at = Position {
        x: 3,
        y: 2,
        channel: 1,
    };
    assert_eq!(comparison.worst, Some((at, 1.0, 1.5)));
    let text = comparison.to_string();
    assert!(text.starts_with("1 of 24 samples outside"), "{text}");
    assert!(
        text.contains("at x 3, y 2, channel 1: expected 1, got 1.5"),
        "{text}"
    );
}

#[test]
fn tolerances_are_absolute_plus_relative() {
    let shape = Shape::new(2, 1, 1);
    let check = |expected: [f64; 2], actual: [f64; 2], tolerance| {
        compare(&expected, &actual, shape, tolerance)
            .unwrap()
            .failed
    };
    assert_eq!(check([100.0, 0.0], [100.5, 0.0], Tolerance::rel(0.005)), 0);
    assert_eq!(check([100.0, 0.0], [100.6, 0.0], Tolerance::rel(0.005)), 1);
    assert_eq!(check([100.0, 0.0], [100.0, 0.25], Tolerance::abs(0.25)), 0);
    assert_eq!(
        check(
            [100.0, 0.0],
            [100.75, 0.25],
            Tolerance {
                abs: 0.25,
                rel: 0.005
            }
        ),
        0
    );
    assert_eq!(check([100.0, 0.0], [100.0, 1e-12], Tolerance::EXACT), 1);
}

#[test]
fn nan_matches_only_nan() {
    let shape = Shape::new(3, 1, 1);
    let expected = [f32::NAN, f32::NAN, 1.0];
    let comparison = compare(&expected, &[f32::NAN, 0.0, 1.0], shape, Tolerance::abs(1.0)).unwrap();
    assert_eq!(
        (comparison.failed, comparison.max_error),
        (1, f64::INFINITY)
    );
    assert_eq!(comparison.worst.unwrap().0.x, 1);
}

#[test]
fn integer_buffers_compare_too() {
    let shape = Shape::new(2, 2, 1);
    let comparison = compare(
        &[0_u16, 10, 20, 30],
        &[0, 10, 21, 30],
        shape,
        Tolerance::abs(1.0),
    )
    .unwrap();
    assert!(comparison.passed());
    assert_eq!(comparison.max_error, 1.0);
}

#[test]
fn different_shapes_or_sizes_are_errors() {
    let a = image(2, 2, 1, vec![0.0; 4]);
    let b = image(4, 1, 1, vec![0.0; 4]);
    let error = compare_images(&a, &b, Tolerance::EXACT).unwrap_err();
    assert!(matches!(error, ComputeError::Shape { .. }));
    let error = compare(
        &[0.0_f32; 3],
        &[0.0; 4],
        Shape::new(2, 2, 1),
        Tolerance::EXACT,
    );
    assert_eq!(
        error.unwrap_err(),
        ComputeError::Size {
            expected: 4,
            got: 3
        }
    );
    assert!(Image::from_vec(Shape::new(2, 2, 1), vec![0.0; 5]).is_err());
}

#[test]
#[should_panic(expected = "not equivalent: 1 of 4 samples outside")]
fn assert_equivalent_panics_with_the_report() {
    let a = image(2, 2, 1, vec![0.0; 4]);
    let b = image(2, 2, 1, vec![0.0, 0.0, 0.0, 0.1]);
    assert_equivalent(&a, &b, Tolerance::abs(0.01));
}
