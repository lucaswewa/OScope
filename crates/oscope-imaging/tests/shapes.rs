//! Shapes' measures (P44): the lengths, areas and centres of synthetic
//! shapes, and the pixels a polygon holds, against every pixel tested.

use oscope_imaging::shapes::{
    Point, path_centroid, path_length, pixel_bounds, polygon_area, polygon_centroid,
    polygon_perimeter, row_spans,
};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]
}

#[test]
fn rectangles_triangles_and_holes_measure_as_drawn() {
    // A rectangle, whichever way it turns, closed or not.
    let r = rect(5.0, 7.0, 10.0, 20.0);
    let mut turned = r.clone();
    turned.reverse();
    for rings in [vec![r.clone()], vec![turned], vec![r[..4].to_vec()]] {
        assert!(close(polygon_area(&rings), 200.0));
        assert!(close(polygon_perimeter(&rings), 60.0));
        assert_eq!(polygon_centroid(&rings), [10.0, 17.0]);
    }
    let triangle = vec![vec![[0.0, 0.0], [4.0, 0.0], [0.0, 3.0]]];
    assert!(close(polygon_area(&triangle), 6.0));
    assert!(close(polygon_perimeter(&triangle), 12.0));
    let c = polygon_centroid(&triangle);
    assert!(close(c[0], 4.0 / 3.0) && close(c[1], 1.0), "{c:?}");
    // A square with a square hole: the hole's area out, its edges in, and
    // the centre pushed away from it.
    let holed = vec![rect(0.0, 0.0, 10.0, 10.0), rect(2.0, 2.0, 4.0, 4.0)];
    assert!(close(polygon_area(&holed), 84.0));
    assert!(close(polygon_perimeter(&holed), 56.0));
    let c = polygon_centroid(&holed);
    assert!(
        close(c[0], 436.0 / 84.0) && close(c[1], 436.0 / 84.0),
        "{c:?}"
    );
    // A ring that encloses nothing has its first point for its centre.
    let flat = vec![vec![[3.0, 4.0], [5.0, 4.0], [3.0, 4.0]]];
    assert_eq!(
        (polygon_area(&flat), polygon_centroid(&flat)),
        (0.0, [3.0, 4.0])
    );
    // Far from the origin, a slide's width away, as precise.
    let far = vec![rect(60_000.123, 40_000.456, 0.5, 0.25)];
    assert!((polygon_area(&far) - 0.125).abs() < 1e-9);
}

#[test]
fn a_path_is_as_long_as_its_segments() {
    let l = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]];
    assert!(close(path_length(&l), 7.0));
    let c = path_centroid(&l);
    assert!(close(c[0], 20.0 / 7.0) && close(c[1], 4.5 / 7.0), "{c:?}");
    assert_eq!(path_length(&[[1.0, 2.0]]), 0.0);
    assert_eq!(path_centroid(&[[1.0, 2.0], [1.0, 2.0]]), [1.0, 2.0]);
}

#[test]
fn a_pixel_is_inside_when_its_centre_is() {
    // Edges on pixels' edges: whole pixels.
    let r = vec![rect(10.0, 20.0, 20.0, 5.0)];
    assert_eq!(pixel_bounds(&r), (10..30, 20..25));
    for row in 20..25 {
        assert_eq!(row_spans(&r, row, 0..100), vec![10..30]);
    }
    assert!(row_spans(&r, 19, 0..100).is_empty() && row_spans(&r, 25, 0..100).is_empty());
    // Edges through pixels' centres: the left and top ones count, the right
    // and bottom ones don't.
    let r = vec![rect(10.5, 0.5, 2.0, 2.0)];
    assert_eq!(pixel_bounds(&r), (10..12, 0..2));
    assert_eq!(row_spans(&r, 0, 0..100), vec![10..12]);
    assert_eq!(row_spans(&r, 1, 0..100), vec![10..12]);
    assert!(row_spans(&r, 2, 0..100).is_empty());
    // Two polygons sharing an edge share no pixel, and leave none out.
    let (left, right) = (
        vec![rect(10.0, 0.0, 10.3, 9.0)],
        vec![rect(20.3, 0.0, 9.7, 9.0)],
    );
    for row in 0..9 {
        assert_eq!(row_spans(&left, row, 0..100), vec![10..20]);
        assert_eq!(row_spans(&right, row, 0..100), vec![20..30]);
    }
    // A hole leaves its pixels out; columns clip the runs.
    let holed = vec![rect(0.0, 0.0, 10.0, 10.0), rect(3.0, 3.0, 4.0, 4.0)];
    assert_eq!(row_spans(&holed, 5, 0..100), vec![0..3, 7..10]);
    assert_eq!(row_spans(&holed, 5, 2..8), vec![2..3, 7..8]);
    assert!(row_spans(&holed, 5, 3..7).is_empty());
    // Left of the image, nothing; points far left don't wrap.
    let off = vec![rect(-50.0, 0.0, 40.0, 2.0)];
    assert!(row_spans(&off, 0, 0..100).is_empty());
    assert_eq!(pixel_bounds(&off).0, 0..0);
    let (columns, rows) = pixel_bounds(&[]);
    assert!(columns.is_empty() && rows.is_empty());
}

/// Whether `p` is inside `rings` by the even-odd rule: W. R. Franklin's
/// crossing test, written apart from the code it checks.
fn inside(rings: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + n - 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                odd = !odd;
            }
        }
    }
    odd
}

#[test]
fn every_pixel_of_a_concave_polygon_with_a_hole_is_where_a_test_puts_it() {
    // A star of 9 points, with vertices off the grid, and a hole across a
    // point of it.
    let star: Vec<Point> = (0..18)
        .map(|k| {
            let a = k as f64 * std::f64::consts::PI / 9.0 + 0.1;
            let r = if k % 2 == 0 { 70.3 } else { 31.7 };
            [100.25 + r * a.cos(), 90.6 + r * a.sin()]
        })
        .collect();
    let hole = vec![[95.0, 70.0], [130.0, 82.0], [110.0, 120.0]];
    let rings = vec![star, hole];
    let (columns, rows) = pixel_bounds(&rings);
    let mut counted = 0;
    for y in 0..200 {
        let mut want = Vec::new();
        for x in 0..220 {
            if inside(&rings, [x as f64 + 0.5, y as f64 + 0.5]) {
                assert!(columns.contains(&x) && rows.contains(&y), "({x}, {y})");
                want.push(x);
            }
        }
        let got: Vec<u64> = row_spans(&rings, y, 0..220).into_iter().flatten().collect();
        assert_eq!(got, want, "row {y}");
        counted += got.len();
    }
    // As many pixels as its area, near enough.
    let area = polygon_area(&rings);
    assert!(
        (counted as f64 - area).abs() < 0.02 * area,
        "{counted} {area}"
    );
}
