//! Plane geometry, stage positions, and affine maps between planes.
//!
//! [`Point2`], [`Size2`] and [`Rect`] are in the unit the API that uses them
//! names: image pixels or micrometres. In an image, x runs right and y runs
//! down from the top-left corner of the top-left pixel, so pixel `(i, j)`
//! covers `[i, i + 1) × [j, j + 1)` (ADR-0007). [`Xyz`] is a stage position in
//! micrometres, and [`Affine2`] maps one plane to another, such as image
//! pixels to stage micrometres.

use std::ops::{Add, Sub};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::units::Um;

/// A point, or a vector, in a plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point2 {
    /// Across.
    pub x: f64,
    /// Down, in an image.
    pub y: f64,
}

impl Point2 {
    /// A point from its coordinates.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The distance to another point.
    pub fn distance(self, other: Point2) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

impl Add for Point2 {
    type Output = Point2;
    fn add(self, other: Point2) -> Point2 {
        Point2::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point2 {
    type Output = Point2;
    fn sub(self, other: Point2) -> Point2 {
        Point2::new(self.x - other.x, self.y - other.y)
    }
}

/// A width and a height, never negative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Size2 {
    /// Along x.
    pub width: f64,
    /// Along y.
    pub height: f64,
}

impl Size2 {
    /// A size from its width and height.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle: its corner of least x and y, and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    /// The corner of least x and y: the top left, in an image.
    pub origin: Point2,
    /// Its size.
    pub size: Size2,
}

impl Rect {
    /// A rectangle from its corner of least x and y, and its size.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            origin: Point2::new(x, y),
            size: Size2::new(width, height),
        }
    }

    /// The corner of greatest x and y.
    pub fn end(&self) -> Point2 {
        Point2::new(
            self.origin.x + self.size.width,
            self.origin.y + self.size.height,
        )
    }

    /// The centre.
    pub fn centre(&self) -> Point2 {
        Point2::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    /// Whether `point` is inside. The edges of least x and y are inside; the
    /// others aren't, so that tiled rectangles don't share points.
    pub fn contains(&self, point: Point2) -> bool {
        let end = self.end();
        (self.origin.x..end.x).contains(&point.x) && (self.origin.y..end.y).contains(&point.y)
    }

    /// The overlap of two rectangles, or `None` if it is empty.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let (a, b) = (self.end(), other.end());
        let origin = Point2::new(
            self.origin.x.max(other.origin.x),
            self.origin.y.max(other.origin.y),
        );
        let end = Point2::new(a.x.min(b.x), a.y.min(b.y));
        (end.x > origin.x && end.y > origin.y)
            .then(|| Rect::new(origin.x, origin.y, end.x - origin.x, end.y - origin.y))
    }
}

/// A rectangle of whole pixels: the column and row of its top-left pixel, and
/// its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PixelRect {
    /// The column of its left edge.
    pub x: u32,
    /// The row of its top edge.
    pub y: u32,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
}

impl PixelRect {
    /// A rectangle from its top-left pixel and its size.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether `other` lies wholly inside this rectangle.
    pub fn contains(&self, other: &PixelRect) -> bool {
        let end = |start: u32, len: u32| u64::from(start) + u64::from(len);
        other.x >= self.x
            && other.y >= self.y
            && end(other.x, other.width) <= end(self.x, self.width)
            && end(other.y, other.height) <= end(self.y, self.height)
    }
}

/// A stage position, or a displacement, in micrometres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Xyz {
    /// The stage's x axis.
    pub x: Um,
    /// The stage's y axis.
    pub y: Um,
    /// The focus axis.
    pub z: Um,
}

impl Xyz {
    /// A position from its coordinates.
    pub const fn new(x: Um, y: Um, z: Um) -> Self {
        Self { x, y, z }
    }

    /// x and y as a point in micrometres, for an [`Affine2`].
    pub fn xy(self) -> Point2 {
        Point2::new(self.x.0, self.y.0)
    }
}

impl Add for Xyz {
    type Output = Xyz;
    fn add(self, other: Xyz) -> Xyz {
        Xyz::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Xyz {
    type Output = Xyz;
    fn sub(self, other: Xyz) -> Xyz {
        Xyz::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// An affine map of the plane, `p ↦ M p + t`: a 2 × 2 matrix `m` (rows), then
/// a translation `t`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Affine2 {
    /// The matrix, row by row: `x' = m[0][0] x + m[0][1] y + t[0]`.
    pub m: [[f64; 2]; 2],
    /// The translation.
    pub t: [f64; 2],
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2 {
    /// The map that changes nothing.
    pub const IDENTITY: Self = Self {
        m: [[1.0, 0.0], [0.0, 1.0]],
        t: [0.0, 0.0],
    };

    /// A shift by `(dx, dy)`.
    pub const fn translation(dx: f64, dy: f64) -> Self {
        Self {
            m: [[1.0, 0.0], [0.0, 1.0]],
            t: [dx, dy],
        }
    }

    /// A scaling of each axis about the origin; a negative factor flips it.
    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self {
            m: [[sx, 0.0], [0.0, sy]],
            t: [0.0, 0.0],
        }
    }

    /// A rotation about the origin by `radians`, from +x towards +y: clockwise
    /// on a screen, where y runs down.
    pub fn rotation(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            m: [[cos, -sin], [sin, cos]],
            t: [0.0, 0.0],
        }
    }

    /// Where the map takes `point`.
    pub fn apply(&self, point: Point2) -> Point2 {
        let [[a, b], [c, d]] = self.m;
        Point2::new(
            a * point.x + b * point.y + self.t[0],
            c * point.x + d * point.y + self.t[1],
        )
    }

    /// This map, then `next`: `a.then(&b).apply(p) == b.apply(a.apply(p))`.
    pub fn then(&self, next: &Affine2) -> Affine2 {
        let [[a, b], [c, d]] = next.m;
        let [[e, f], [g, h]] = self.m;
        let t = next.apply(Point2::new(self.t[0], self.t[1]));
        Affine2 {
            m: [
                [a * e + b * g, a * f + b * h],
                [c * e + d * g, c * f + d * h],
            ],
            t: [t.x, t.y],
        }
    }

    /// The determinant of the matrix: the factor by which areas change, negative
    /// if the map flips.
    pub fn determinant(&self) -> f64 {
        self.m[0][0] * self.m[1][1] - self.m[0][1] * self.m[1][0]
    }

    /// The map that undoes this one.
    pub fn inverse(&self) -> Result<Affine2> {
        let determinant = self.determinant();
        let size: f64 = self.m.iter().flatten().map(|v| v * v).sum();
        // Relative to the matrix's size, so the test doesn't depend on the unit.
        if determinant.is_nan() || determinant.abs() <= 1e-12 * size {
            return Err(Error::Singular { determinant });
        }
        let [[a, b], [c, d]] = self.m;
        let m = [
            [d / determinant, -b / determinant],
            [-c / determinant, a / determinant],
        ];
        let inverse = Affine2 { m, t: [0.0, 0.0] };
        let t = inverse.apply(Point2::new(-self.t[0], -self.t[1]));
        Ok(Affine2 { m, t: [t.x, t.y] })
    }

    /// The map that best takes each pair's first point to its second, in the
    /// least-squares sense. It needs three pairs or more, not all on one line.
    ///
    /// The points are centred on their means first, which keeps the normal
    /// equations well conditioned at stage distances; the translation then
    /// maps one mean to the other.
    pub fn fit(pairs: &[(Point2, Point2)]) -> Result<Affine2> {
        if pairs.len() < 3 {
            return Err(Error::TooFewPoints {
                needed: 3,
                got: pairs.len(),
            });
        }
        let n = pairs.len() as f64;
        let mean = |pick: fn(&(Point2, Point2)) -> Point2| {
            let sum = pairs.iter().map(pick).fold(Point2::default(), |s, p| s + p);
            Point2::new(sum.x / n, sum.y / n)
        };
        let (from_mean, to_mean) = (mean(|p| p.0), mean(|p| p.1));
        // Sums of products of the centred coordinates: s_xy = Σ x·y, and so on,
        // where u and v are the centred targets.
        let [
            mut sxx,
            mut sxy,
            mut syy,
            mut sux,
            mut suy,
            mut svx,
            mut svy,
        ] = [0.0; 7];
        for (from, to) in pairs {
            let (p, q) = (*from - from_mean, *to - to_mean);
            sxx += p.x * p.x;
            sxy += p.x * p.y;
            syy += p.y * p.y;
            sux += q.x * p.x;
            suy += q.x * p.y;
            svx += q.y * p.x;
            svy += q.y * p.y;
        }
        let det = sxx * syy - sxy * sxy;
        if det.is_nan() || det <= 1e-12 * (sxx + syy).powi(2) {
            return Err(Error::Collinear);
        }
        let m = [
            [(sux * syy - suy * sxy) / det, (suy * sxx - sux * sxy) / det],
            [(svx * syy - svy * sxy) / det, (svy * sxx - svx * sxy) / det],
        ];
        let linear = Affine2 { m, t: [0.0, 0.0] };
        let t = to_mean - linear.apply(from_mean);
        Ok(Affine2 { m, t: [t.x, t.y] })
    }

    /// The root mean square of the distances between where the map takes each
    /// pair's first point and its second: how well a fit fits.
    pub fn rms_error(&self, pairs: &[(Point2, Point2)]) -> f64 {
        let sum: f64 = pairs
            .iter()
            .map(|(from, to)| self.apply(*from).distance(*to).powi(2))
            .sum();
        (sum / pairs.len() as f64).sqrt()
    }
}
