//! Shapes in the plane (P44): the lengths, areas and centres of lines and
//! polygons, and the pixels a polygon covers, a row at a time.
//!
//! Points are `[x, y]`, x across and y down. A polygon is its rings, its
//! outer one and then its holes, each closing itself whether or not its
//! last point repeats its first, and its inside is by the even-odd rule. A
//! pixel `(i, j)` covers `[i, i + 1) × [j, j + 1)`, and is a polygon's when
//! its centre is inside: on a left or top edge counts, on a right or bottom
//! one doesn't, so polygons that share an edge share no pixel.

use std::ops::Range;

/// A point: x across, y down.
pub type Point = [f64; 2];

/// The edges of `ring`, closing it.
fn edges(ring: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    let next = ring.iter().cycle().skip(1);
    ring.iter().copied().zip(next.copied())
}

fn distance(a: Point, b: Point) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The length of the path through `points`, in order.
pub fn path_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| distance(w[0], w[1])).sum()
}

/// The centre of the path through `points`: its segments' middles,
/// weighted by their lengths; its first point if it has no length.
pub fn path_centroid(points: &[Point]) -> Point {
    let (mut length, mut sum) = (0.0, [0.0; 2]);
    for w in points.windows(2) {
        let l = distance(w[0], w[1]);
        length += l;
        sum[0] += l * (w[0][0] + w[1][0]) / 2.0;
        sum[1] += l * (w[0][1] + w[1][1]) / 2.0;
    }
    match points.first() {
        Some(&first) if length == 0.0 => first,
        _ => [sum[0] / length, sum[1] / length],
    }
}

/// `ring`'s signed area and its first moments, about `o`, by the shoelace
/// formula: measured from a point near it, to keep their precision.
fn moments(ring: &[Point], o: Point) -> [f64; 3] {
    let mut m = [0.0; 3];
    for (a, b) in edges(ring) {
        let (a, b) = ([a[0] - o[0], a[1] - o[1]], [b[0] - o[0], b[1] - o[1]]);
        let cross = a[0] * b[1] - b[0] * a[1];
        m[0] += cross / 2.0;
        m[1] += (a[0] + b[0]) * cross / 6.0;
        m[2] += (a[1] + b[1]) * cross / 6.0;
    }
    m
}

/// The polygon's area and first moments about its first point, the outer
/// ring's counted in and its holes' out, whichever way each turns.
fn polygon_moments(rings: &[Vec<Point>]) -> (Point, [f64; 3]) {
    let o = rings.first().and_then(|r| r.first()).copied();
    let o = o.unwrap_or_default();
    let mut total = [0.0; 3];
    for (k, ring) in rings.iter().enumerate() {
        let m = moments(ring, o);
        let sign = if k == 0 { 1.0 } else { -1.0 } * m[0].signum();
        for (t, v) in total.iter_mut().zip(m) {
            *t += sign * v;
        }
    }
    (o, total)
}

/// The area of the polygon `rings`: its outer ring's, less its holes'.
pub fn polygon_area(rings: &[Vec<Point>]) -> f64 {
    polygon_moments(rings).1[0]
}

/// The length of the polygon's boundary: its outer ring's and its holes'.
pub fn polygon_perimeter(rings: &[Vec<Point>]) -> f64 {
    rings
        .iter()
        .map(|ring| edges(ring).map(|(a, b)| distance(a, b)).sum::<f64>())
        .sum()
}

/// The centre of the polygon's area: of its outer ring's first point if it
/// has none.
pub fn polygon_centroid(rings: &[Vec<Point>]) -> Point {
    let (o, m) = polygon_moments(rings);
    if m[0] == 0.0 {
        return o;
    }
    [o[0] + m[1] / m[0], o[1] + m[2] / m[0]]
}

/// The first column whose pixel's centre is at `x` or right of it, or row
/// whose pixel's centre is at `y` or below it.
fn first_centre(x: f64) -> u64 {
    // `as` saturates: a point far left is column 0, and NaN is too.
    (x - 0.5).ceil() as u64
}

/// The columns and rows of the pixels whose centres the polygon `rings` may
/// hold: empty for a polygon of no points.
pub fn pixel_bounds(rings: &[Vec<Point>]) -> (Range<u64>, Range<u64>) {
    let (mut low, mut high) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in rings.iter().flatten() {
        for k in 0..2 {
            low[k] = low[k].min(p[k]);
            high[k] = high[k].max(p[k]);
        }
    }
    let range = |k: usize| first_centre(low[k])..first_centre(high[k]);
    (range(0), range(1))
}

/// The pixels of row `row` that the polygon `rings` holds, within
/// `columns`: runs of columns, left to right.
pub fn row_spans(rings: &[Vec<Point>], row: u64, columns: Range<u64>) -> Vec<Range<u64>> {
    let y = row as f64 + 0.5;
    // Where the row's centre line crosses each edge, an edge's lower end
    // counted and its upper end not, so that a vertex is crossed once.
    let mut crossings: Vec<f64> = rings
        .iter()
        .flat_map(|ring| edges(ring))
        .filter(|(a, b)| (a[1] <= y) != (b[1] <= y))
        .map(|(a, b)| a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]))
        .collect();
    crossings.sort_by(f64::total_cmp);
    let clip = |x: f64| first_centre(x).clamp(columns.start, columns.end);
    let (pairs, _) = crossings.as_chunks::<2>();
    pairs
        .iter()
        .map(|&[enter, leave]| clip(enter)..clip(leave))
        .filter(|run| !run.is_empty())
        .collect()
}
