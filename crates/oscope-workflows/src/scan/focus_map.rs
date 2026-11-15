//! Focus maps: a plane or a thin-plate spline fitted to focus measured at
//! sample points, with outliers rejected one at a time.

use oscope_core::Point2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::region::Region;
use super::{ScanError, invalid};

/// Focus measured at a point: where the stage was sharpest there.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FocusPoint {
    /// Its x, in µm.
    pub x_um: f64,
    /// Its y.
    pub y_um: f64,
    /// The z found there.
    pub z_um: f64,
}

/// The surface a map fits. Each needs three points not in a line.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Surface {
    /// A plane: a slide lying flat, tilted.
    Plane,
    /// A thin-plate spline (Duchon, 1977; Bookstein, 1989): the surface of
    /// least bending through the points, or near them.
    ThinPlate {
        /// λ, what bending costs against missing the points: 0 passes
        /// through each (Wahba, 1990).
        smoothing: f64,
    },
}

/// How outliers are found.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FitOptions {
    /// A point is an outlier when the surface fitted to the others misses
    /// it by more than this many robust standard deviations of the misses.
    pub threshold: f64,
    /// The least robust standard deviation taken, in µm: about what focus
    /// can tell apart. Without it, points on an exact surface would make any
    /// other an outlier.
    pub floor_um: f64,
}

impl Default for FitOptions {
    /// Three robust standard deviations, of at least 1 µm.
    fn default() -> Self {
        Self {
            threshold: 3.0,
            floor_um: 1.0,
        }
    }
}

/// A focus map: a surface fitted to focus measured at sample points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FocusMap {
    /// The surface.
    pub surface: Surface,
    /// The points fitted.
    pub used: Vec<FocusPoint>,
    /// The points rejected as outliers, in the order they were.
    pub rejected: Vec<FocusPoint>,
    /// The root mean square of how far the surface fitted to the others
    /// misses each used point, in µm: how well the map foretells focus where
    /// it wasn't measured. `None` with only three points.
    pub rms_um: Option<f64>,
    /// Where coordinates are taken from, in µm: the used points' middle.
    /// They are taken in mm.
    pub centre: Point2,
    /// What [`FocusMap::z_at`] evaluates: a plane's z at the centre, then its
    /// slopes along x and y in µm a mm; or a spline's weight for each used
    /// point, then its plane's three.
    pub coefficients: Vec<f64>,
}

impl FocusMap {
    /// The map `surface` fits to `points`. While more than four, and more
    /// than half of those given, are left, the point the others' surface
    /// misses worst is rejected if it misses by more than `options` allow:
    /// the threshold times 1.4826 times the misses' median absolute
    /// deviation, the standard deviation for normal errors (Hampel, 1974;
    /// Rousseeuw and Croux, 1993). Each miss is a leave-one-out residual
    /// (Craven and Wahba, 1979), so an outlier can't hide by pulling the
    /// surface to it.
    pub fn fit(
        points: &[FocusPoint],
        surface: Surface,
        options: &FitOptions,
    ) -> Result<Self, ScanError> {
        let finite = |p: &FocusPoint| [p.x_um, p.y_um, p.z_um].iter().all(|v| v.is_finite());
        let smooth = match surface {
            Surface::Plane => 0.0,
            Surface::ThinPlate { smoothing } => smoothing,
        };
        let FitOptions {
            threshold,
            floor_um,
        } = *options;
        if !(points.iter().all(finite) && smooth >= 0.0 && smooth.is_finite())
            || !(threshold > 0.0 && floor_um >= 0.0 && floor_um.is_finite())
        {
            return Err(invalid(format!(
                "a fit to {} points, smoothing {smooth}, threshold {threshold}, floor {floor_um} \
                 µm: each finite, the smoothing and floor at least 0, the threshold more than 0, \
                 are needed",
                points.len()
            )));
        }
        let least = 4.max(points.len().div_ceil(2));
        let (mut used, mut rejected) = (points.to_vec(), Vec::new());
        let (centre, coefficients, known) = loop {
            let (centre, coefficients) = solve(&used, surface)?;
            let misses = leave_one_out(&used, surface);
            let known: Vec<f64> = misses.iter().flatten().copied().collect();
            let worst = (0..used.len())
                .filter_map(|i| misses.get(i).copied().flatten().map(|m| (i, m.abs())))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let scale = (1.4826 * median_deviation(&known)).max(floor_um);
            match worst {
                Some((i, miss)) if used.len() > least && miss > threshold * scale => {
                    rejected.push(used.remove(i));
                }
                _ => break (centre, coefficients, known),
            }
        };
        let rms_um = (!known.is_empty())
            .then(|| (known.iter().map(|m| m * m).sum::<f64>() / known.len() as f64).sqrt());
        Ok(Self {
            surface,
            used,
            rejected,
            rms_um,
            centre,
            coefficients,
        })
    }

    /// The map's z at `at`, in µm.
    pub fn z_at(&self, at: Point2) -> f64 {
        evaluate(
            self.surface,
            &self.used,
            self.centre,
            &self.coefficients,
            at,
        )
    }
}

/// Where to measure focus for a map over `region`: the middles of a
/// `columns` × `rows` grid over its bounds that are in it, row by row, each
/// row the other way from the last.
pub fn sample_points(region: &Region, columns: u32, rows: u32) -> Vec<Point2> {
    let b = region.bounds();
    let (dx, dy) = (
        b.size.width / f64::from(columns),
        b.size.height / f64::from(rows),
    );
    (0..rows)
        .flat_map(|j| {
            let along = (0..columns).map(move |i| match j % 2 {
                0 => i,
                _ => columns - 1 - i,
            });
            along.map(move |i| {
                Point2::new(
                    b.origin.x + (f64::from(i) + 0.5) * dx,
                    b.origin.y + (f64::from(j) + 0.5) * dy,
                )
            })
        })
        .filter(|p| region.contains(*p))
        .collect()
}

/// A point's coordinates from `centre`, in mm.
fn mm(x: f64, y: f64, centre: Point2) -> (f64, f64) {
    ((x - centre.x) / 1e3, (y - centre.y) / 1e3)
}

/// The thin plate's kernel, U(r) = r² ln r, 0 at 0.
fn kernel(du: f64, dv: f64) -> f64 {
    let r2 = du * du + dv * dv;
    match r2 > 0.0 {
        true => r2 * r2.ln() / 2.0,
        false => 0.0,
    }
}

/// The surface through or near `points`: its centre and coefficients.
fn solve(points: &[FocusPoint], surface: Surface) -> Result<(Point2, Vec<f64>), ScanError> {
    let n = points.len();
    let none = || {
        invalid(format!(
            "{n} points of focus: at least 3, not in a line, are needed for a surface"
        ))
    };
    if n < 3 {
        return Err(none());
    }
    let centre = Point2::new(
        points.iter().map(|p| p.x_um).sum::<f64>() / n as f64,
        points.iter().map(|p| p.y_um).sum::<f64>() / n as f64,
    );
    let uv: Vec<(f64, f64)> = points.iter().map(|p| mm(p.x_um, p.y_um, centre)).collect();
    let (size, smoothing) = match surface {
        Surface::Plane => (3, None),
        Surface::ThinPlate { smoothing } => (n + 3, Some(smoothing)),
    };
    let (mut a, mut b) = (vec![0.0; size * size], vec![0.0; size]);
    for (i, (&(u, v), p)) in uv.iter().zip(points).enumerate() {
        let row = [1.0, u, v];
        match smoothing {
            // The normal equations of z = c₀ + c₁ u + c₂ v.
            None => {
                for r in 0..3 {
                    b[r] += row[r] * p.z_um;
                    for c in 0..3 {
                        a[r * 3 + c] += row[r] * row[c];
                    }
                }
            }
            // [K + λI, P; Pᵀ, 0] [w; c] = [z; 0] (Bookstein, 1989).
            Some(lambda) => {
                for (j, &(uj, vj)) in uv.iter().enumerate() {
                    a[i * size + j] = kernel(u - uj, v - vj);
                }
                a[i * size + i] += lambda;
                for (k, value) in row.into_iter().enumerate() {
                    a[i * size + n + k] = value;
                    a[(n + k) * size + i] = value;
                }
                b[i] = p.z_um;
            }
        }
    }
    let coefficients = solve_linear(a, b, size).ok_or_else(none)?;
    Ok((centre, coefficients))
}

fn evaluate(surface: Surface, points: &[FocusPoint], centre: Point2, c: &[f64], at: Point2) -> f64 {
    let (u, v) = mm(at.x, at.y, centre);
    match surface {
        Surface::Plane => c[0] + c[1] * u + c[2] * v,
        Surface::ThinPlate { .. } => {
            let n = points.len();
            let bending: f64 = points
                .iter()
                .zip(c)
                .map(|(p, w)| {
                    let (pu, pv) = mm(p.x_um, p.y_um, centre);
                    w * kernel(u - pu, v - pv)
                })
                .sum();
            bending + c[n] + c[n + 1] * u + c[n + 2] * v
        }
    }
}

/// How far the surface fitted to the others misses each point; `None` for a
/// point without which the others fit no surface, or with only three.
fn leave_one_out(points: &[FocusPoint], surface: Surface) -> Vec<Option<f64>> {
    if points.len() < 4 {
        return vec![None; points.len()];
    }
    (0..points.len())
        .map(|i| {
            let mut others = points.to_vec();
            let p = others.remove(i);
            let (centre, c) = solve(&others, surface).ok()?;
            let at = Point2::new(p.x_um, p.y_um);
            Some(p.z_um - evaluate(surface, &others, centre, &c, at))
        })
        .collect()
}

/// The median absolute deviation from the median; 0 for none.
fn median_deviation(values: &[f64]) -> f64 {
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        match v.len() {
            0 => 0.0,
            n if n % 2 == 1 => v[n / 2],
            n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
        }
    };
    let m = median(&mut values.to_vec());
    median(&mut values.iter().map(|v| (v - m).abs()).collect())
}

/// `x` with `a x = b`, `a` being `n` × `n` by rows, by Gaussian elimination
/// with partial pivoting (Golub and Van Loan, 2013, §3.4); `None` if `a` is
/// singular, as far as its pivots can tell.
fn solve_linear(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    let scale = a.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    for k in 0..n {
        let p = (k..n).max_by(|&i, &j| a[i * n + k].abs().total_cmp(&a[j * n + k].abs()))?;
        if a[p * n + k].abs() <= 1e-12 * scale {
            return None;
        }
        if p != k {
            for j in 0..n {
                a.swap(k * n + j, p * n + j);
            }
            b.swap(k, p);
        }
        for i in k + 1..n {
            let f = a[i * n + k] / a[k * n + k];
            for j in k..n {
                a[i * n + j] -= f * a[k * n + j];
            }
            b[i] -= f * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let rest: f64 = (k + 1..n).map(|j| a[k * n + j] * x[j]).sum();
        x[k] = (b[k] - rest) / a[k * n + k];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `side` × `side` points 5 mm apart from (20, 10) mm, at `z`
    /// of their mm from there, each off by a fixed pattern of ±0.2 µm.
    fn grid(side: u32, z: impl Fn(f64, f64) -> f64) -> Vec<FocusPoint> {
        let noise = [0.2, -0.1, 0.0, 0.15, -0.2, 0.05, -0.05, 0.1, -0.15];
        (0..side * side)
            .map(|k| {
                let (u, v) = (f64::from(k % side) * 5.0, f64::from(k / side) * 5.0);
                FocusPoint {
                    x_um: 20_000.0 + u * 1e3,
                    y_um: 10_000.0 + v * 1e3,
                    z_um: z(u, v) + noise[k as usize % noise.len()],
                }
            })
            .collect()
    }

    #[test]
    fn a_tilted_plane_is_recovered_and_an_outlier_rejected() {
        let mut points = grid(3, |u, v| 5_000.0 + 2.0 * u - 1.0 * v);
        points[4].z_um += 12.0;
        let map = FocusMap::fit(&points, Surface::Plane, &FitOptions::default()).unwrap();
        assert_eq!(map.rejected, vec![points[4]]);
        assert_eq!(map.used.len(), 8);
        let c = &map.coefficients;
        assert!(
            (c[1] - 2.0).abs() < 0.1 && (c[2] + 1.0).abs() < 0.1,
            "{c:?}"
        );
        let at = Point2::new(23_000.0, 16_000.0);
        assert!((map.z_at(at) - (5_000.0 + 6.0 - 6.0)).abs() < 0.3);
        assert!(map.rms_um.unwrap() < 0.5);
        // Without the outlier, nothing is rejected.
        points[4].z_um -= 12.0;
        let map = FocusMap::fit(&points, Surface::Plane, &FitOptions::default()).unwrap();
        assert!(map.rejected.is_empty());
    }

    #[test]
    fn a_spline_follows_a_bowl_that_a_plane_cannot() {
        // A slide sagging 5.6 µm from its corners to its middle, 21 mm
        // across them.
        let bowl = |u: f64, v: f64| 5_000.0 + 0.05 * ((u - 7.5).powi(2) + (v - 7.5).powi(2));
        let mut points = grid(4, bowl);
        let spline = Surface::ThinPlate { smoothing: 0.0 };
        let options = FitOptions::default();
        let map = FocusMap::fit(&points, spline, &options).unwrap();
        let plane = FocusMap::fit(&points, Surface::Plane, &options).unwrap();
        let mut worst = (0.0_f64, 0.0_f64);
        for (u, v) in [(2.5, 2.5), (7.5, 7.5), (12.5, 5.0), (5.0, 10.0)] {
            let at = Point2::new(20_000.0 + u * 1e3, 10_000.0 + v * 1e3);
            worst.0 = worst.0.max((map.z_at(at) - bowl(u, v)).abs());
            worst.1 = worst.1.max((plane.z_at(at) - bowl(u, v)).abs());
        }
        assert!(worst.0 < 0.5 && worst.1 > 2.5, "{worst:?}");
        assert!(map.rejected.is_empty());
        // Through each point, with no smoothing.
        let p = points[5];
        assert!((map.z_at(Point2::new(p.x_um, p.y_um)) - p.z_um).abs() < 1e-6);
        // An outlier is rejected, by the others' spline.
        points[6].z_um -= 15.0;
        let map = FocusMap::fit(&points, spline, &options).unwrap();
        assert_eq!(map.rejected, vec![points[6]]);
    }

    #[test]
    fn points_that_fit_no_surface_are_refused() {
        let line: Vec<FocusPoint> = (0..4)
            .map(|k| FocusPoint {
                x_um: f64::from(k) * 1e3,
                y_um: f64::from(k) * 2e3,
                z_um: 5_000.0,
            })
            .collect();
        for surface in [Surface::Plane, Surface::ThinPlate { smoothing: 0.0 }] {
            assert!(FocusMap::fit(&line, surface, &FitOptions::default()).is_err());
            assert!(FocusMap::fit(&line[..2], surface, &FitOptions::default()).is_err());
        }
    }

    #[test]
    fn sample_points_snake_through_the_region() {
        let square = Region::Rect(oscope_core::Rect::new(0.0, 0.0, 3e3, 3e3));
        let points = sample_points(&square, 3, 3);
        let xs: Vec<f64> = points.iter().map(|p| p.x).collect();
        assert_eq!(
            xs,
            [
                500.0, 1500.0, 2500.0, 2500.0, 1500.0, 500.0, 500.0, 1500.0, 2500.0
            ]
        );
        assert!(points.iter().all(|p| square.contains(*p)));
    }
}
