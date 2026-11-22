//! The filters on the CPU, against small cases worked by hand: the
//! references their CUDA twins are compared with.

use oscope_compute::filters::{self, Clahe};
use oscope_compute::{ComputeError, Image, Shape};

fn image(width: u32, height: u32, channels: usize, data: Vec<f32>) -> Image {
    Image::from_vec(Shape::new(width, height, channels), data).unwrap()
}

fn close(a: &[f32], b: &[f32], within: f32) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= within)
}

#[test]
fn a_gaussian_spreads_a_point_by_its_weights_and_keeps_a_level() {
    let weights = filters::gaussian_weights(1.0).unwrap();
    assert_eq!(weights.len(), 7, "three sigmas each side");
    assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    assert!(
        (weights[3] / weights[4] - 0.5_f32.exp()).abs() < 1e-5,
        "exp(−k²/2σ²)"
    );
    // A point in the middle of 15 × 15 becomes the weights' outer product.
    let mut point = vec![0.0; 225];
    point[7 * 15 + 7] = 1.0;
    let spread = filters::gaussian(&image(15, 15, 1, point), 1.0).unwrap();
    for (dy, wy) in weights.iter().enumerate() {
        for (dx, wx) in weights.iter().enumerate() {
            let at = (4 + dy) * 15 + 4 + dx;
            assert!((spread.data()[at] - wy * wx).abs() < 1e-7);
        }
    }
    let level = filters::gaussian(&image(9, 4, 3, vec![0.25; 108]), 2.5).unwrap();
    assert!(close(level.data(), &[0.25; 108], 1e-6));
    for sigma in [0.0, -1.0, 33.0, f32::NAN] {
        assert!(matches!(
            filters::gaussian_weights(sigma),
            Err(ComputeError::Setting(_))
        ));
    }
}

#[test]
fn a_median_takes_each_window_s_middle_value() {
    // 3 × 3, radius 1, the edges repeated: worked by hand. The top-left
    // pixel's window is 1 1 9 / 1 1 9 / 8 8 3, whose middle value is 3.
    let picture = image(3, 3, 1, vec![1.0, 9.0, 2.0, 8.0, 3.0, 7.0, 4.0, 6.0, 5.0]);
    let middle = filters::median(&picture, 1).unwrap();
    assert_eq!(middle.data(), [3.0, 3.0, 3.0, 4.0, 5.0, 5.0, 4.0, 5.0, 5.0]);
    // A lone spike goes; a channel's own values are kept.
    let mut flat = vec![0.5; 5 * 5 * 2];
    flat[2 * 10 + 2 * 2] = 1.0;
    flat[2 * 10 + 2 * 2 + 1] = 0.25;
    let smoothed = filters::median(&image(5, 5, 2, flat), 1).unwrap();
    assert!(smoothed.data().iter().all(|&v| v == 0.5));
    assert!(matches!(
        filters::median(&picture, 8),
        Err(ComputeError::Setting(_))
    ));
}

#[test]
fn a_bilateral_filter_keeps_edges_a_gaussian_blurs() {
    // A step from 0 to 1: with a narrow range, the pixels beside it keep
    // their side's value, where a Gaussian of the same width mixes them.
    let step: Vec<f32> = (0..12).map(|x| if x < 6 { 0.0 } else { 1.0 }).collect();
    let row = image(12, 1, 1, step);
    let kept = filters::bilateral(&row, 2.0, 0.1).unwrap();
    assert!(
        kept.data()[5] < 1e-6 && kept.data()[6] > 1.0 - 1e-6,
        "{:?}",
        kept.data()
    );
    let blurred = filters::gaussian(&row, 2.0).unwrap();
    assert!(blurred.data()[5] > 0.2 && blurred.data()[6] < 0.8);
    // A level is kept, and a wide range is a Gaussian of the window.
    let level = filters::bilateral(&image(6, 5, 3, vec![0.4; 90]), 1.5, 0.2).unwrap();
    assert!(close(level.data(), &[0.4; 90], 1e-6));
    let wide = filters::bilateral(&row, 1.0, 1e6).unwrap();
    let (spatial, radius) = filters::bilateral_weights(1.0).unwrap();
    assert_eq!((spatial.len(), radius), (25, 2));
    let row_weights: Vec<f32> = spatial[10..15].to_vec();
    let total: f32 = spatial.iter().sum();
    // At x = 6 the window's columns are 4–8, each column's weights summing
    // as its spatial column does, as every row is the same.
    let column = |dx: usize| (0..5).map(|dy| spatial[dy * 5 + dx]).sum::<f32>();
    let expected: f32 = (0..5)
        .map(|dx| column(dx) * if dx + 4 >= 6 { 1.0 } else { 0.0 })
        .sum::<f32>()
        / total;
    assert!(
        (wide.data()[6] - expected).abs() < 1e-5,
        "{} {expected} {row_weights:?}",
        wide.data()[6]
    );
    assert!(matches!(
        filters::bilateral(&row, 9.0, 0.1),
        Err(ComputeError::Setting(_))
    ));
    assert!(matches!(
        filters::bilateral(&row, 1.0, 0.0),
        Err(ComputeError::Setting(_))
    ));
}

#[test]
fn clahe_clips_shares_and_interpolates_as_worked_by_hand() {
    // One tile, a level of 0.5, 4 bins: the histogram [0, 0, N, 0] is
    // clipped at 2N / 4, and the excess N / 2 shared, N / 8 a bin: its
    // cumulative share at bin 2 is 7 / 8.
    let level = image(8, 8, 1, vec![0.5; 64]);
    let one = Clahe {
        tiles: (1, 1),
        clip: 2.0,
        bins: 4,
    };
    let equalised = filters::clahe(&level, &one).unwrap();
    assert!(
        equalised.data().iter().all(|&v| v == 0.875),
        "{:?}",
        &equalised.data()[..4]
    );
    assert_eq!(
        filters::clahe_maps(&level, &one).unwrap(),
        [0.125, 0.25, 0.875, 1.0]
    );
    // Without clipping, a ramp's histogram is flat and maps to itself, to
    // within a bin.
    let ramp: Vec<f32> = (0..256).map(|i| (i as f32 + 0.5) / 256.0).collect();
    let plain = Clahe {
        tiles: (1, 1),
        clip: 1e6,
        bins: 256,
    };
    let mapped = filters::clahe(&image(256, 1, 1, ramp.clone()), &plain).unwrap();
    assert!(close(mapped.data(), &ramp, 1.0 / 256.0 + 1e-6));
    // Two tiles, dark and light: the pixels between their centres blend
    // their tables linearly, and those beyond take their own.
    let halves: Vec<f32> = (0..64)
        .map(|i| if i % 16 < 8 { 0.2 } else { 0.8 })
        .collect();
    let two = Clahe {
        tiles: (2, 1),
        clip: 1e6,
        bins: 4,
    };
    let blended = filters::clahe(&image(16, 4, 1, halves), &two).unwrap();
    let row = &blended.data()[..16];
    // Each tile holds one level, so its table jumps to 1 at that level's
    // bin: the dark tile maps 0.2 (bin 0) to 1 and 0.8 (bin 3) to 1; the
    // light tile maps 0.2 to 0 and 0.8 to 1.
    assert_eq!(
        row[..4],
        [1.0; 4],
        "beyond the dark tile's centre, its own table"
    );
    assert_eq!(
        row[12..],
        [1.0; 4],
        "beyond the light tile's centre, its own table"
    );
    // Between the centres (x 4 to 12), the dark pixels blend 1 and 0.
    let expected = |x: f32| 1.0 - (x + 0.5 - 4.0) / 8.0;
    for (x, value) in row.iter().enumerate().take(8).skip(4) {
        assert!((value - expected(x as f32)).abs() < 1e-6, "x {x}: {value}");
    }
    for bad in [
        Clahe {
            tiles: (0, 1),
            ..one
        },
        Clahe { clip: 0.5, ..one },
        Clahe { bins: 1, ..one },
    ] {
        assert!(matches!(
            filters::clahe(&level, &bad),
            Err(ComputeError::Setting(_))
        ));
    }
}
