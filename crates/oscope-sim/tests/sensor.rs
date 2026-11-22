//! The sensor model: its noise, its saturation, and binned pixels as sums.

use oscope_sim::SensorModel;
use oscope_sim::sensor::{ADC_MAX, Random};

/// The mean and variance of `n` draws.
fn moments(n: usize, mut draw: impl FnMut() -> f64) -> (f64, f64) {
    let values: Vec<f64> = (0..n).map(|_| draw()).collect();
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    (mean, variance)
}

#[test]
fn the_full_well_spans_the_adc_above_black() {
    let s = SensorModel::default();
    let quiet = SensorModel { noise: false, ..s };
    let mut random = Random::new(1);
    assert_eq!(quiet.count(s.full_well, 0.0, 1, 0.0, &mut random), ADC_MAX);
    assert_eq!(quiet.count(0.0, 0.0, 1, 0.0, &mut random), s.black);
    // 24 dB of gain multiplies the counts by 10^1.2.
    let ratio = s.counts_per_electron(240.0) / s.counts_per_electron(0.0);
    assert!((ratio - 10f64.powf(1.2)).abs() < 1e-9);
}

#[test]
fn the_noise_is_shot_dark_and_read_noise() {
    let s = SensorModel::default();
    let mut random = Random::new(7);
    // 4,000 electrons in 10 ms: shot noise √4,000, read noise 2.5, dark
    // current negligible; then the counts' scale, and quantisation's 1/12.
    let cpe = s.counts_per_electron(0.0);
    let (mean, variance) = moments(200_000, || {
        f64::from(s.count(4_000.0, 0.01, 1, 0.0, &mut random))
    });
    let expected_mean = f64::from(s.black) + (4_000.0 + 0.005) * cpe;
    let expected_variance = (4_000.0 + 2.5 * 2.5) * cpe * cpe + 1.0 / 12.0;
    assert!(
        (mean - expected_mean).abs() < 0.2,
        "{mean} against {expected_mean}"
    );
    assert!(
        (variance / expected_variance - 1.0).abs() < 0.03,
        "{variance} against {expected_variance}"
    );
}

#[test]
fn binned_pixels_add_their_read_noise() {
    // No light and no dark current: four binned pixels have four times one
    // pixel's read noise variance. At 24 dB of gain the noise is about 8
    // counts, so rounding to whole counts adds just 1/12 to it.
    let s = SensorModel {
        dark_current: 0.0,
        black: 1_000,
        ..SensorModel::default()
    };
    let mut random = Random::new(11);
    let cpe = s.counts_per_electron(240.0);
    let (_, one) = moments(200_000, || {
        f64::from(s.count(0.0, 0.0, 1, 240.0, &mut random))
    });
    let (_, four) = moments(200_000, || {
        f64::from(s.count(0.0, 0.0, 4, 240.0, &mut random))
    });
    let read = 2.5 * 2.5 * cpe * cpe;
    assert!(((one - 1.0 / 12.0) / read - 1.0).abs() < 0.05, "{one}");
    assert!(
        ((four - 1.0 / 12.0) / (4.0 * read) - 1.0).abs() < 0.05,
        "{four}"
    );
}

#[test]
fn the_poisson_draws_have_their_mean_as_variance() {
    let mut random = Random::new(3);
    // Small means are counted; large ones drawn from a normal.
    for lambda in [0.5, 3.0, 20.0, 100.0, 5_000.0] {
        let (mean, variance) = moments(200_000, || random.poisson(lambda));
        assert!((mean / lambda - 1.0).abs() < 0.02, "{lambda}: mean {mean}");
        assert!(
            (variance / lambda - 1.0).abs() < 0.03,
            "{lambda}: variance {variance}"
        );
    }
}

#[test]
fn bright_light_saturates_at_the_full_well() {
    let s = SensorModel::default();
    let mut random = Random::new(5);
    for _ in 0..1_000 {
        assert_eq!(
            s.count(5.0 * s.full_well, 0.01, 1, 0.0, &mut random),
            ADC_MAX
        );
    }
    // Gain saturates the ADC before the well: at 24 dB, a sixteenth of it.
    assert_eq!(
        s.count(s.full_well / 10.0, 0.01, 1, 240.0, &mut random),
        ADC_MAX
    );
}

#[test]
fn binned_pixels_sum_their_electrons() {
    let quiet = SensorModel {
        noise: false,
        dark_current: 0.0,
        ..SensorModel::default()
    };
    let mut random = Random::new(9);
    let one = quiet.count(1_000.0, 0.0, 1, 0.0, &mut random) - quiet.black;
    let four = quiet.count(4_000.0, 0.0, 4, 0.0, &mut random) - quiet.black;
    let nine = quiet.count(9_000.0, 0.0, 9, 0.0, &mut random) - quiet.black;
    assert!(
        (i32::from(four) - 4 * i32::from(one)).abs() <= 2,
        "{one} and {four}"
    );
    assert!(
        (i32::from(nine) - 9 * i32::from(one)).abs() <= 4,
        "{one} and {nine}"
    );
    // The sum is digitised on the same scale, so the ADC clips it at about
    // one well's worth: a binned frame saturates in k² times dimmer light.
    let s = SensorModel::default();
    let full = s.count(1.1 * s.full_well, 0.0, 4, 0.0, &mut random);
    assert_eq!(full, ADC_MAX);
}
