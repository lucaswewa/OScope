//! Stain analysis (P45): colour deconvolution of pixels that Beer–Lambert
//! made from known amounts; refusals; each stain alone; Macenko's estimate
//! of synthetic stains; DAB's bins and scores.

use oscope_imaging::stain::{
    DAB, DabCounts, DabThresholds, EOSIN, HEMATOXYLIN, StainChoice, StainError, Stains, TISSUE,
    Vector, dab_class, dab_classes, macenko, optical_density,
};

fn unit(v: Vector) -> Vector {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    v.map(|x| x / n)
}

/// The light a pixel passes of `white`, with `amounts` of stains of
/// `vectors`: Beer and Lambert's law.
fn pixel(vectors: &[Vector], amounts: &[f64], white: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        let od: f64 = vectors
            .iter()
            .zip(amounts)
            .map(|(v, a)| a * unit(*v)[c])
            .sum();
        white[c] * 10f64.powf(-od) as f32
    })
}

/// Amounts of two stains from 0 to `top`, every 0.05 of `top` / 1.5.
fn grid(top: f64) -> Vec<[f64; 2]> {
    let steps = (0..=30).map(|k| k as f64 * top / 30.0);
    steps
        .clone()
        .flat_map(|a| steps.clone().map(move |b| [a, b]))
        .collect()
}

#[test]
fn deconvolution_gives_back_the_amounts_beer_and_lambert_made() {
    let white = [0.9, 0.8, 0.85];
    for (stains, vectors) in [
        (Stains::he(), [HEMATOXYLIN, EOSIN]),
        (Stains::hdab(), [HEMATOXYLIN, DAB]),
    ] {
        let made = grid(1.5);
        let rgb: Vec<f32> = made
            .iter()
            .flat_map(|a| pixel(&vectors, a, white))
            .collect();
        let amounts = stains.deconvolve(&rgb, white);
        for (a, got) in made.iter().zip(amounts.as_chunks::<3>().0) {
            let off = [got[0] as f64 - a[0], got[1] as f64 - a[1], got[2] as f64];
            assert!(off.iter().all(|d| d.abs() < 1e-5), "{a:?}: {got:?}");
        }
    }
    // Three stains given: all three back.
    let three = Stains::new(HEMATOXYLIN, EOSIN, Some(DAB)).unwrap();
    let rgb = pixel(&[HEMATOXYLIN, EOSIN, DAB], &[0.3, 0.7, 0.4], white);
    let got = three.deconvolve(&rgb, white);
    for (g, want) in got.iter().zip([0.3, 0.7, 0.4]) {
        assert!((f64::from(*g) - want).abs() < 1e-5, "{got:?}");
    }
    // Quantised to 8 bits, within 0.01 while the densities are under 1:
    // green, the densest, at 0.93.
    let made = grid(0.55);
    let rgb: Vec<f32> = made
        .iter()
        .flat_map(|a| pixel(&[HEMATOXYLIN, EOSIN], a, [1.0; 3]))
        .map(|v| (v * 255.0).round() / 255.0)
        .collect();
    let amounts = Stains::he().deconvolve(&rgb, [1.0; 3]);
    for (a, got) in made.iter().zip(amounts.as_chunks::<3>().0) {
        let off = (f64::from(got[0]) - a[0])
            .abs()
            .max((f64::from(got[1]) - a[1]).abs());
        assert!(off < 1e-2, "{a:?}: {got:?}");
    }
    // The vectors normalised; the residual square to both, the third a pixel
    // of a stain the two don't know.
    let v = Stains::he().vectors();
    let dot = |a: Vector, b: Vector| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    assert!((dot(v[0], v[0]) - 1.0).abs() < 1e-12 && (dot(v[2], v[2]) - 1.0).abs() < 1e-12);
    assert!(dot(v[2], v[0]).abs() < 1e-12 && dot(v[2], v[1]).abs() < 1e-12);
    let brown = pixel(&[DAB], &[0.8], white);
    assert!(Stains::he().deconvolve(&brown, white)[2].abs() > 0.1);
    // No light at all is held at the floor: an optical density of 3.01.
    assert_eq!(optical_density(0.0, 2.0), 1024f32.log10());
    assert_eq!(optical_density(2.0, 2.0), 0.0);
}

#[test]
fn vectors_that_aren_t_stains_are_refused() {
    let wrong: [(Vector, Vector, Option<Vector>); 5] = [
        (HEMATOXYLIN, HEMATOXYLIN, None),
        (HEMATOXYLIN, [0.5, -0.1, 0.3], None),
        ([f64::NAN, 1.0, 0.0], EOSIN, None),
        ([0.0; 3], EOSIN, None),
        (HEMATOXYLIN, EOSIN, Some(HEMATOXYLIN)),
    ];
    for (a, b, c) in wrong {
        let refused = Stains::new(a, b, c);
        assert!(
            matches!(refused, Err(StainError::Vectors(_))),
            "{a:?} {b:?} {c:?}"
        );
    }
}

#[test]
fn each_stain_alone_passes_its_share_of_the_light() {
    let white = [0.9, 0.8, 0.85];
    let stains = Stains::he();
    let rgb = pixel(&[HEMATOXYLIN, EOSIN], &[0.6, 0.9], white);
    let amounts = stains.deconvolve(&rgb, white);
    let (h, e) = (
        stains.alone(0, &amounts, white),
        stains.alone(1, &amounts, white),
    );
    let want = pixel(&[HEMATOXYLIN], &[0.6], white);
    for c in 0..3 {
        // Hematoxylin alone, as it would look; with eosin, the pixel.
        assert!((h[c] - want[c]).abs() < 1e-5, "{h:?} {want:?}");
        assert!((h[c] * e[c] / white[c] - rgb[c]).abs() < 1e-5);
    }
}

/// The angle between two vectors, in degrees.
fn degrees(a: Vector, b: Vector) -> f64 {
    let (a, b) = (unit(a), unit(b));
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// A number from 0 to 1, from `k`: a fixed sequence, the same each run.
fn spread(k: u64) -> f64 {
    let h = k.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(17) ^ 0x5851_f42d_4c95_7f2d;
    (h.wrapping_mul(0xd6e8_feb8_6659_fd93) >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn macenko_finds_two_stains_vectors_from_their_mixtures() {
    let white = [0.95, 0.9, 0.92];
    for second in [EOSIN, DAB] {
        // A fifth of the pixels glass, a fifth each stain alone, and the rest
        // mixtures, amounts from 0.3 to 1.2.
        let rgb: Vec<f32> = (0..5_000_u64)
            .flat_map(|k| {
                let (a, b) = (0.3 + 0.9 * spread(2 * k), 0.3 + 0.9 * spread(2 * k + 1));
                let amounts = match k % 5 {
                    0 => [0.0, 0.0],
                    1 => [a, 0.0],
                    2 => [0.0, b],
                    _ => [a, b],
                };
                pixel(&[HEMATOXYLIN, second], &amounts, white)
            })
            .collect();
        let [h, s] = macenko(&rgb, white, 0.15, 1.0).unwrap();
        assert!(degrees(h, HEMATOXYLIN) < 2.0, "{h:?}");
        assert!(degrees(s, second) < 2.0, "{s:?} {second:?}");
        // Outliers, 20 pixels of a red dye, are left out by the percentile,
        // which the extremes aren't.
        let mut outlying = rgb.clone();
        for k in 0..20 {
            let p = pixel(&[[0.9, 0.1, 0.1]], &[0.8], white);
            outlying[15 * k + 3..15 * k + 6].copy_from_slice(&p);
        }
        let [h, _] = macenko(&outlying, white, 0.15, 1.0).unwrap();
        assert!(degrees(h, HEMATOXYLIN) < 2.0, "{h:?}");
        let [h, _] = macenko(&outlying, white, 0.15, 0.0).unwrap();
        assert!(degrees(h, HEMATOXYLIN) > 5.0, "{h:?}");
    }
    // The stains chosen.
    let three = StainChoice::Vectors(vec![HEMATOXYLIN, EOSIN, DAB]);
    let given = Stains::new(HEMATOXYLIN, EOSIN, Some(DAB)).unwrap();
    assert_eq!(three.stains().unwrap(), given);
    let two = StainChoice::Vectors(vec![HEMATOXYLIN, DAB]);
    assert_eq!(two.stains().unwrap(), Stains::hdab());
    assert_eq!(StainChoice::He.stains().unwrap(), Stains::he());
    assert_eq!(StainChoice::Hdab.stains().unwrap(), Stains::hdab());
    assert!(StainChoice::Vectors(vec![HEMATOXYLIN]).stains().is_err());
    // Glass, and a few stained pixels, aren't enough.
    let mut rgb = vec![0.95_f32; 3 * 1_000];
    for k in 0..99 {
        let p = pixel(&[HEMATOXYLIN, EOSIN], &[0.5, 0.5], white);
        rgb[3 * k..3 * k + 3].copy_from_slice(&p);
    }
    let few = macenko(&rgb, white, 0.15, 1.0);
    assert!(matches!(few, Err(StainError::TooFew(_))), "{few:?}");
}

#[test]
fn dab_bins_count_the_tissue_and_score_it() {
    let t = DabThresholds::default();
    assert_eq!(dab_class(TISSUE - 1e-3, 2.0, &t), 0);
    assert_eq!(dab_class(TISSUE, 0.0, &t), 1);
    let at = [
        (0.149, 1),
        (0.15, 2),
        (0.349, 2),
        (0.35, 3),
        (0.599, 3),
        (0.60, 4),
    ];
    for (dab, class) in at {
        assert_eq!(dab_class(1.0, dab, &t), class, "{dab}");
    }
    // A patch: 100 pixels of glass, then tissue of hematoxylin 0.4 with DAB
    // in the middle of each bin, 500 negative, 200 weak, 200 moderate and
    // 100 strong.
    let white = [0.9, 0.85, 0.8];
    let mut rgb = Vec::new();
    for (n, dab) in [
        (100, -1.0),
        (500, 0.05),
        (200, 0.25),
        (200, 0.45),
        (100, 0.9),
    ] {
        for _ in 0..n {
            let p = match dab {
                d if d < 0.0 => white,
                d => pixel(&[HEMATOXYLIN, DAB], &[0.4, d], white),
            };
            rgb.extend(p);
        }
    }
    let mut counts = DabCounts::default();
    for class in dab_classes(&Stains::hdab(), &rgb, white, &t) {
        counts.add(class);
    }
    let want = DabCounts {
        glass: 100,
        negative: 500,
        weak: 200,
        moderate: 200,
        strong: 100,
    };
    assert_eq!(counts, want);
    assert_eq!(
        (counts.tissue(), counts.positive_percent(), counts.h_score()),
        (1_000, 50.0, 90.0)
    );
    // No tissue: nothing positive, and no score.
    let glass = DabCounts {
        glass: 9,
        ..DabCounts::default()
    };
    assert_eq!((glass.positive_percent(), glass.h_score()), (0.0, 0.0));
    // Thresholds given move the bins.
    let strict = DabThresholds {
        weak: 0.3,
        moderate: 0.5,
        strong: 1.0,
    };
    let mut counts = DabCounts::default();
    for class in dab_classes(&Stains::hdab(), &rgb, white, &strict) {
        counts.add(class);
    }
    assert_eq!(
        (counts.negative, counts.weak, counts.moderate),
        (700, 200, 100)
    );
}
