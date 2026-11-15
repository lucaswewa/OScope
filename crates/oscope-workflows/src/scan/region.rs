//! Where a scan goes: a rectangle, a polygon or a mask, in stage µm.

use oscope_core::{Point2, Rect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ScanError, invalid};

/// A grid of cells over the stage, each in or out: where tissue is, as an
/// overview finds it (P37), or any other shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Mask {
    /// The corner of its first cell, of least x and y, in µm.
    pub origin: Point2,
    /// A cell's side, in µm.
    pub cell_um: f64,
    /// Its width, in cells.
    pub width: u32,
    /// Its height, in cells.
    pub height: u32,
    /// Whether each cell is in, by rows from the first.
    pub cells: Vec<bool>,
}

impl Mask {
    fn is_in(&self, column: i64, row: i64) -> bool {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        (0..w).contains(&column) && (0..h).contains(&row) && self.cells[(row * w + column) as usize]
    }

    /// The cell holding `at`, by its column and row.
    fn cell(&self, at: Point2) -> (i64, i64) {
        let c = self.cell_um;
        (
            ((at.x - self.origin.x) / c).floor() as i64,
            ((at.y - self.origin.y) / c).floor() as i64,
        )
    }
}

/// Where a scan goes, in stage µm. In JSON, an object of one key, its kind:
/// `{"rect": {"origin", "size"}}`, `{"polygon": {"corners"}}` or
/// `{"mask": {…}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    /// A rectangle, square with the stage, its edges inside.
    Rect(Rect),
    /// A polygon, by its corners in order: it closes itself. Its edges are
    /// inside, and its inside is by the even-odd rule, so a polygon that
    /// crosses itself leaves out what it goes round twice.
    Polygon {
        /// Its corners.
        corners: Vec<Point2>,
    },
    /// A mask's cells that are in.
    Mask(Mask),
}

impl Region {
    /// Checks that the region is one: finite, enclosing something.
    pub fn check(&self) -> Result<(), ScanError> {
        let finite = |p: &Point2| p.x.is_finite() && p.y.is_finite();
        match self {
            Region::Rect(r) => {
                let (w, h) = (r.size.width, r.size.height);
                if !(finite(&r.origin) && w > 0.0 && h > 0.0 && (w * h).is_finite()) {
                    return Err(invalid(format!(
                        "a rectangle of {w} × {h} µm at ({}, {}): finite, and more than 0 wide \
                         and high, is needed",
                        r.origin.x, r.origin.y
                    )));
                }
            }
            Region::Polygon { corners } => {
                if corners.len() < 3 || !corners.iter().all(finite) || area(corners) == 0.0 {
                    return Err(invalid(format!(
                        "a polygon of {} corners: at least 3, finite, enclosing something, are \
                         needed",
                        corners.len()
                    )));
                }
            }
            Region::Mask(m) => {
                let cells = u64::from(m.width) * u64::from(m.height);
                if !(m.cell_um > 0.0 && m.cell_um.is_finite() && finite(&m.origin)) {
                    return Err(invalid(format!(
                        "a mask of {} µm cells: finite, and more than 0, are needed",
                        m.cell_um
                    )));
                }
                if m.cells.len() as u64 != cells || !m.cells.contains(&true) {
                    return Err(invalid(format!(
                        "a mask of {} × {} cells, with {} given, {} of them in: one for each \
                         cell, and at least one in, are needed",
                        m.width,
                        m.height,
                        m.cells.len(),
                        m.cells.iter().filter(|&&c| c).count()
                    )));
                }
            }
        }
        Ok(())
    }

    /// The least rectangle holding it: for a mask, its cells that are in.
    pub fn bounds(&self) -> Rect {
        match self {
            Region::Rect(r) => *r,
            Region::Polygon { corners } => {
                let (x0, x1) = span(corners.iter().map(|p| p.x));
                let (y0, y1) = span(corners.iter().map(|p| p.y));
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            }
            Region::Mask(m) => {
                let w = m.width as usize;
                let at = |i: usize| ((i % w) as f64, (i / w) as f64);
                let ins = m.cells.iter().enumerate().filter(|(_, c)| **c);
                let (c0, c1) = span(ins.clone().map(|(i, _)| at(i).0));
                let (r0, r1) = span(ins.map(|(i, _)| at(i).1));
                let c = m.cell_um;
                Rect::new(
                    m.origin.x + c0 * c,
                    m.origin.y + r0 * c,
                    (c1 - c0 + 1.0) * c,
                    (r1 - r0 + 1.0) * c,
                )
            }
        }
    }

    /// Whether `at` is in it. In a mask, a point is in when the cell
    /// holding it is, a cell holding its edges of least x and y.
    pub fn contains(&self, at: Point2) -> bool {
        match self {
            Region::Rect(r) => holds(r, at),
            Region::Polygon { corners } => in_polygon(corners, at),
            Region::Mask(m) => {
                let (i, j) = m.cell(at);
                m.is_in(i, j)
            }
        }
    }

    /// Whether it shares a point with the rectangle `cell`, edges included:
    /// a tile's, which a plan keeps if it does.
    pub fn touches(&self, cell: &Rect) -> bool {
        match self {
            Region::Rect(r) => {
                let (a, b) = (r.end(), cell.end());
                r.origin.x <= b.x
                    && cell.origin.x <= a.x
                    && r.origin.y <= b.y
                    && cell.origin.y <= a.y
            }
            Region::Polygon { corners } => {
                let box_corners = rect_corners(cell);
                corners.iter().any(|&p| holds(cell, p))
                    || box_corners.iter().any(|&p| in_polygon(corners, p))
                    || edges(corners)
                        .any(|(a, b)| edges(&box_corners).any(|(c, d)| segments_meet(a, b, c, d)))
            }
            Region::Mask(m) => {
                let ((c0, r0), (c1, r1)) = (m.cell(cell.origin), m.cell(cell.end()));
                let (c1, r1) = (
                    c1.min(i64::from(m.width) - 1),
                    r1.min(i64::from(m.height) - 1),
                );
                (r0.max(0)..=r1).any(|j| (c0.max(0)..=c1).any(|i| m.is_in(i, j)))
            }
        }
    }
}

/// The least and greatest of `values`.
fn span(values: impl Iterator<Item = f64>) -> (f64, f64) {
    values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    })
}

/// Whether the rectangle `r` holds `p`, edges included.
fn holds(r: &Rect, p: Point2) -> bool {
    let end = r.end();
    (r.origin.x..=end.x).contains(&p.x) && (r.origin.y..=end.y).contains(&p.y)
}

fn rect_corners(r: &Rect) -> [Point2; 4] {
    let (a, b) = (r.origin, r.end());
    [a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)]
}

/// Each edge of a closed polygon.
fn edges(corners: &[Point2]) -> impl Iterator<Item = (Point2, Point2)> + '_ {
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
}

/// The polygon's signed area, by the shoelace formula.
fn area(corners: &[Point2]) -> f64 {
    edges(corners)
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        / 2.0
}

/// `(a − o) × (b − o)`: positive when `o`, `a`, `b` turn left.
fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether `p`, on the line through `a` and `b`, lies between them.
fn between(a: Point2, b: Point2, p: Point2) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}

/// Whether the segments `ab` and `cd` share a point, by the signs of the
/// cross products (Cormen et al., 2009, §33.1).
fn segments_meet(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    let apart = |u: f64, v: f64| (u > 0.0 && v < 0.0) || (u < 0.0 && v > 0.0);
    (apart(d1, d2) && apart(d3, d4))
        || (d1 == 0.0 && between(c, d, a))
        || (d2 == 0.0 && between(c, d, b))
        || (d3 == 0.0 && between(a, b, c))
        || (d4 == 0.0 && between(a, b, d))
}

/// Whether `p` is on the polygon's edges, or inside it by the even-odd rule:
/// a ray from it crosses its edges an odd number of times (Shimrat, 1962;
/// Haines, 1994).
fn in_polygon(corners: &[Point2], p: Point2) -> bool {
    let mut inside = false;
    for (a, b) in edges(corners) {
        if cross(a, b, p) == 0.0 && between(a, b, p) {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An L: a 4 × 4 square without its top-right 2 × 2.
    fn ell() -> Region {
        let p = |x, y| Point2::new(x, y);
        Region::Polygon {
            corners: vec![
                p(0.0, 0.0),
                p(4.0, 0.0),
                p(4.0, 2.0),
                p(2.0, 2.0),
                p(2.0, 4.0),
                p(0.0, 4.0),
            ],
        }
    }

    #[test]
    fn a_concave_polygon_holds_its_inside_and_edges_only() {
        let l = ell();
        assert!(l.check().is_ok());
        for (x, y, inside) in [
            (1.0, 1.0, true),
            (3.0, 1.0, true),
            (1.0, 3.0, true),
            (3.0, 3.0, false),
            (4.0, 1.0, true),
            (2.0, 3.0, true),
            (5.0, 1.0, false),
            (-1.0, 1.0, false),
            (-1.0, 3.0, false),
        ] {
            assert_eq!(l.contains(Point2::new(x, y)), inside, "({x}, {y})");
        }
        assert_eq!(l.bounds(), Rect::new(0.0, 0.0, 4.0, 4.0));
        // In its notch, a cell touches it only by reaching its edges.
        assert!(!l.touches(&Rect::new(2.5, 2.5, 1.0, 1.0)));
        assert!(l.touches(&Rect::new(2.0, 2.5, 1.0, 1.0)));
        // Across an edge, with no corner of either in the other.
        assert!(l.touches(&Rect::new(-1.0, 1.0, 6.0, 0.5)));
        // Holding it whole, and held by it.
        assert!(l.touches(&Rect::new(-1.0, -1.0, 6.0, 6.0)));
        assert!(l.touches(&Rect::new(0.5, 0.5, 0.5, 0.5)));
    }

    #[test]
    fn a_mask_is_its_cells_that_are_in() {
        // 3 × 2 cells of 10 µm from (100, 200): only the middle of the top
        // row and the right of the bottom one are in.
        let mask = Region::Mask(Mask {
            origin: Point2::new(100.0, 200.0),
            cell_um: 10.0,
            width: 3,
            height: 2,
            cells: vec![false, true, false, false, false, true],
        });
        assert!(mask.check().is_ok());
        assert_eq!(mask.bounds(), Rect::new(110.0, 200.0, 20.0, 20.0));
        assert!(mask.contains(Point2::new(115.0, 205.0)));
        assert!(!mask.contains(Point2::new(105.0, 215.0)));
        assert!(!mask.contains(Point2::new(95.0, 205.0)));
        assert!(mask.touches(&Rect::new(90.0, 190.0, 21.0, 11.0)));
        assert!(!mask.touches(&Rect::new(90.0, 210.0, 15.0, 8.0)));
    }

    #[test]
    fn regions_that_are_none_are_refused() {
        let p = |x, y| Point2::new(x, y);
        let flat = Region::Polygon {
            corners: vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0)],
        };
        assert!(flat.check().is_err());
        assert!(Region::Rect(Rect::new(0.0, 0.0, 0.0, 5.0)).check().is_err());
        let empty = Mask {
            origin: p(0.0, 0.0),
            cell_um: 10.0,
            width: 2,
            height: 1,
            cells: vec![false, false],
        };
        assert!(Region::Mask(empty.clone()).check().is_err());
        let short = Mask {
            cells: vec![true],
            ..empty
        };
        assert!(Region::Mask(short).check().is_err());
    }
}
