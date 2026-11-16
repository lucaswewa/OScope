//! Cells (P46): Otsu's and the adaptive threshold, holes filled, the
//! seeded watershed on discs that touch, a dumbbell and plateaus, and the
//! measures and outlines of shapes of known size.

use oscope_imaging::cells::{
    Filters, Threshold, adaptive, components, distances, fill_holes, measure, otsu, outline,
    segment, watershed,
};
use oscope_imaging::shapes::polygon_area;

/// A mask, `w` × `h`, of the pixels whose centres `inside` holds.
fn mask(w: usize, h: usize, inside: impl Fn(f64, f64) -> bool) -> Vec<u8> {
    (0..w * h)
        .map(|i| u8::from(inside((i % w) as f64 + 0.5, (i / w) as f64 + 0.5)))
        .collect()
}

fn disc(cx: f64, cy: f64, r: f64) -> impl Fn(f64, f64) -> bool {
    move |x, y| (x - cx).hypot(y - cy) <= r
}

#[test]
fn thresholds_find_their_objects() {
    // Two populations: Otsu's level between them.
    let values: Vec<f32> = (0..1_000)
        .map(|i| {
            if i % 3 == 0 {
                0.8 + (i % 7) as f32 * 0.01
            } else {
                0.2 + (i % 5) as f32 * 0.01
            }
        })
        .collect();
    // The first split across the gap: every value of one population below
    // it, and of the other at or above it.
    let level = otsu(&values, 256);
    let below = values.iter().filter(|&&v| v < level).count();
    assert!(
        values.iter().all(|&v| (v < level) == (v < 0.5)) && below == 666 && level < 0.3,
        "{level}"
    );
    // Spots on a slope: a global level takes the slope's high half, the
    // adaptive one the spots alone.
    let (w, h) = (120, 60);
    let spots = [(20.0, 30.0), (60.0, 30.0), (100.0, 30.0)];
    let values: Vec<f32> = (0..w * h)
        .map(|i| {
            let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
            let spot = spots.iter().any(|&(cx, cy)| (x - cx).hypot(y - cy) <= 4.0);
            (x / w as f64 + if spot { 0.3 } else { 0.0 }) as f32
        })
        .collect();
    let global: Vec<u8> = values
        .iter()
        .map(|&v| u8::from(v >= otsu(&values, 256)))
        .collect();
    assert!(components(&global, w, true).1 != 3);
    let found = adaptive(&values, w, 10, 0.1);
    let (labels, count) = components(&found, w, true);
    assert_eq!(count, 3);
    for (k, &(cx, cy)) in spots.iter().enumerate() {
        let i = cy as usize * w + cx as usize;
        assert_eq!(labels[i], k as u32 + 1);
    }
    let want = mask(w, h, |x, y| {
        spots.iter().any(|&(cx, cy)| (x - cx).hypot(y - cy) <= 4.0)
    });
    let wrong = found.iter().zip(&want).filter(|(a, b)| a != b).count();
    assert!(wrong < 6, "{wrong} pixels wrong");
    // A ring's hole filled; the background at the edge left.
    let ring = mask(40, 40, |x, y| {
        (6.0..=12.0).contains(&(x - 20.0).hypot(y - 20.0))
    });
    let filled = fill_holes(&ring, 40);
    assert_eq!(
        filled,
        mask(40, 40, |x, y| (x - 20.0).hypot(y - 20.0) <= 12.0)
    );
    // A cup open only at the bottom edge isn't a hole.
    let cup = mask(40, 40, |x, y| {
        (10.0..30.0).contains(&x) && y > 10.0 && !((14.0..26.0).contains(&x) && y > 14.0)
    });
    assert_eq!(fill_holes(&cup, 40), cup);
    // A square's border without its corner: the inside meets the outside at
    // a corner only, so it is still a hole.
    let border = mask(40, 40, |x, y| {
        let on = (5.0..15.0).contains(&x) && (5.0..15.0).contains(&y);
        let inside = (6.0..14.0).contains(&x) && (6.0..14.0).contains(&y);
        on && !inside && !(x < 6.0 && y < 6.0)
    });
    let filled = fill_holes(&border, 40);
    assert_eq!(filled[10 * 40 + 10], 1);
    // A line up and to the right: one component by corners, three by sides.
    let rising = mask(10, 10, |x, y| (x + y - 10.0).abs() < 0.5);
    assert_eq!(
        (
            components(&rising, 10, true).1,
            components(&rising, 10, false).1
        ),
        (1, 10)
    );
}

#[test]
fn the_watershed_splits_what_touches_and_keeps_what_doesn_t() {
    let (w, h) = (80, 50);
    // Two discs overlapping: two objects, each about its own disc.
    let both = mask(w, h, |x, y| {
        disc(25.0, 25.0, 11.0)(x, y) || disc(43.0, 25.0, 11.0)(x, y)
    });
    let (labels, count) = watershed(&both, &distances(&both, w), w, 3, 2.0);
    assert_eq!(count, 2);
    let objects = measure(&labels, w, count, [1.0, 1.0], &[]);
    for o in &objects {
        let full = std::f64::consts::PI * 121.0;
        assert!(o.area > 0.75 * full && o.area < full, "{o:?}");
    }
    assert!((objects[0].centroid[0] - objects[1].centroid[0]).abs() > 14.0);
    // Nothing leaves the mask, and all of it is labelled.
    assert!(
        labels
            .iter()
            .zip(&both)
            .all(|(&l, &b)| (l != 0) == (b != 0))
    );
    // A dumbbell: split at its neck.
    let bell = mask(w, h, |x, y| {
        disc(20.0, 25.0, 9.0)(x, y)
            || disc(58.0, 25.0, 9.0)(x, y)
            || ((20.0..58.0).contains(&x) && (y - 25.0).abs() < 1.5)
    });
    let (labels, count) = watershed(&bell, &distances(&bell, w), w, 3, 2.0);
    assert_eq!(count, 2);
    let side = |x: usize| labels[25 * w + x];
    assert!(side(15) != side(60) && side(15) == side(30) && side(60) == side(48));
    // Two squares of even side on a neck: each peak a plateau, one seed.
    let squares = mask(w, h, |x, y| {
        ((8.0..20.0).contains(&x) || (40.0..52.0).contains(&x)) && (19.0..31.0).contains(&y)
            || (20.0..40.0).contains(&x) && (y - 25.0).abs() < 1.0
    });
    assert_eq!(watershed(&squares, &distances(&squares, w), w, 3, 2.0).1, 2);
    // Unequal discs: split where they're narrowest, deepest first, not
    // half-way between their seeds.
    let unequal = mask(w, h, |x, y| {
        disc(25.0, 25.0, 13.0)(x, y) || disc(40.0, 25.0, 7.0)(x, y)
    });
    let (labels, count) = watershed(&unequal, &distances(&unequal, w), w, 3, 2.0);
    let areas = measure(&labels, w, count, [1.0, 1.0], &[]);
    let small = areas.iter().map(|o| o.area).fold(f64::INFINITY, f64::min);
    let own = std::f64::consts::PI * 49.0;
    assert!(count == 2 && small < own, "{count}: {small} of {own}");
    // At their waist, where their circles cross, x = 36.5: not half-way.
    let row: Vec<u32> = (0..w).map(|x| labels[25 * w + x]).collect();
    let split = (1..w).find(|&x| row[x] != row[x - 1] && row[x - 1] != 0 && row[x] != 0);
    assert!(split.is_some_and(|x| (35..=38).contains(&x)), "{split:?}");
    // One disc, and a bar whose ridge is a plateau: one object each.
    for shape in [
        mask(w, h, disc(40.0, 25.0, 12.0)),
        mask(w, h, |x, y| {
            (10.0..70.0).contains(&x) && (18.0..30.0).contains(&y)
        }),
    ] {
        assert_eq!(watershed(&shape, &distances(&shape, w), w, 3, 2.0).1, 1);
    }
    // A line too thin to seed is an object of its own.
    let line = mask(w, h, |x, y| {
        (10.0..70.0).contains(&x) && (y - 10.5).abs() < 0.5
    });
    let (labels, count) = watershed(&line, &distances(&line, w), w, 3, 2.0);
    assert_eq!(count, 1);
    assert!(
        labels
            .iter()
            .zip(&line)
            .all(|(&l, &b)| (l != 0) == (b != 0))
    );
}

#[test]
fn segmenting_thresholds_fills_and_splits() {
    let (w, h) = (80, 50);
    // A hollow nucleus and a touching pair, bright on a dim ground.
    let on = |x: f64, y: f64| {
        (6.0..=11.0).contains(&(x - 20.0).hypot(y - 25.0))
            || disc(50.0, 25.0, 8.0)(x, y)
            || disc(63.0, 25.0, 8.0)(x, y)
    };
    let values: Vec<f32> = (0..w * h)
        .map(|i| {
            if on((i % w) as f64 + 0.5, (i / w) as f64 + 0.5) {
                0.9
            } else {
                0.1
            }
        })
        .collect();
    let thresholds = [
        Threshold::Otsu,
        Threshold::Level(0.5),
        Threshold::Adaptive {
            radius: 15,
            offset: 0.1,
        },
    ];
    for threshold in thresholds {
        let (labels, count) = segment(&values, w, threshold, 3, 2.0);
        assert_eq!(count, 3, "{threshold:?}");
        // The hollow one's middle is its own.
        assert_ne!(labels[25 * w + 20], 0, "{threshold:?}");
    }
}

#[test]
fn objects_measure_as_drawn() {
    let (w, h) = (120, 100);
    for (r, pixel) in [(10.0, 0.5), (25.0, 0.5), (20.0, 0.25)] {
        let m = mask(w, h, disc(60.0, 50.0, r));
        let (labels, count) = components(&m, w, true);
        let o = &measure(&labels, w, count, [pixel, pixel], &[])[0];
        let (area, round) = (
            std::f64::consts::PI * (r * pixel).powi(2),
            2.0 * std::f64::consts::PI * r * pixel,
        );
        assert!(
            (o.area / area - 1.0).abs() < 0.02,
            "r {r}: {} of {area}",
            o.area
        );
        assert!(
            (o.perimeter / round - 1.0).abs() < 0.05,
            "r {r}: {} of {round}",
            o.perimeter
        );
        assert!(
            (o.circularity - 1.0).abs() < 0.1,
            "r {r}: {}",
            o.circularity
        );
        assert!(
            (o.centroid[0] - 60.0 * pixel).abs() < 1e-9
                && (o.centroid[1] - 50.0 * pixel).abs() < 1e-9
        );
    }
    // A rectangle on pixels of 0.5 × 0.25 µm: its area, centre and means.
    let m = mask(w, h, |x, y| {
        (10.0..30.0).contains(&x) && (20.0..30.0).contains(&y)
    });
    let ramp: Vec<f32> = (0..w * h).map(|i| (i % w) as f32).collect();
    let flat = vec![2.5_f32; w * h];
    let (labels, count) = components(&m, w, true);
    let o = &measure(&labels, w, count, [0.5, 0.25], &[&ramp, &flat])[0];
    assert_eq!((o.pixels, o.area, o.centroid), (200, 25.0, [10.0, 6.25]));
    assert_eq!(o.means, vec![19.5, 2.5]);
    // Its crack, 2 × (20 × 0.5 + 10 × 0.25) µm, times π/4.
    assert!((o.perimeter - 25.0 * std::f64::consts::FRAC_PI_4).abs() < 1e-9);
    // The filters.
    let filters = Filters::default();
    assert!(filters.keep(o));
    let small = Filters {
        min_area: 30.0,
        ..filters
    };
    let round = Filters {
        min_circularity: 0.9,
        ..filters
    };
    let big = Filters {
        max_area: 20.0,
        ..filters
    };
    assert!(!small.keep(o) && !round.keep(o) && !big.keep(o));
}

#[test]
fn an_outline_follows_its_object_s_edges() {
    let (w, h) = (40, 30);
    let m = mask(w, h, |x, y| {
        (2.0..6.0).contains(&x) && (1.0..4.0).contains(&y)
    });
    let (labels, count) = components(&m, w, true);
    let o = &measure(&labels, w, count, [1.0, 1.0], &[])[0];
    assert_eq!(
        outline(&labels, w, o),
        vec![[2.0, 1.0], [6.0, 1.0], [6.0, 4.0], [2.0, 4.0], [2.0, 1.0]]
    );
    // A disc, an L and a shape touching the edge: each outline encloses
    // its pixels exactly.
    let shapes = [
        mask(w, h, disc(20.0, 15.0, 9.0)),
        mask(w, h, |x, y| {
            (5.0..25.0).contains(&x) && (5.0..9.0).contains(&y)
                || (5.0..9.0).contains(&x) && (5.0..25.0).contains(&y)
        }),
        mask(w, h, |x, y| x < 7.0 && y < 12.0),
    ];
    for m in shapes {
        let (labels, count) = components(&m, w, false);
        assert_eq!(count, 1);
        let o = &measure(&labels, w, count, [1.0, 1.0], &[])[0];
        let ring = outline(&labels, w, o);
        assert_eq!(ring.first(), ring.last());
        assert_eq!(polygon_area(&[ring]), o.pixels as f64);
    }
}
