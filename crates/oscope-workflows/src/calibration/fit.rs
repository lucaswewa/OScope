//! The camera-to-stage matrix: fitted to stage moves and the image shifts
//! they make, and decomposed into a turn, a mirror, two scales and a skew
//! (P32's spec; Björck, 1996; Golub and Van Loan, 2013, §5.2).

/// A 2 × 2 matrix, by rows.
pub type Matrix = [[f64; 2]; 2];

fn det(m: Matrix) -> f64 {
    m[0][0] * m[1][1] - m[0][1] * m[1][0]
}

/// `m`'s inverse, if it has one.
pub fn inverse(m: Matrix) -> Option<Matrix> {
    let d = det(m);
    let [[a, b], [c, e]] = m;
    (d != 0.0 && d.is_finite()).then(|| [[e / d, -b / d], [-c / d, a / d]])
}

/// `m · v`.
pub fn apply(m: Matrix, v: (f64, f64)) -> (f64, f64) {
    (m[0][0] * v.0 + m[0][1] * v.1, m[1][0] * v.0 + m[1][1] * v.1)
}

/// The matrix M fitted to stage `moves` (µm) and the image `shifts` (px)
/// they made: N, in pixels per µm, by least squares to `d = −N · s`, the
/// moves being exact and the shifts measured; and M = N⁻¹. With the fit's
/// root-mean-square error, in pixels. `None` if the moves don't span the
/// plane.
pub fn fit_matrix(moves: &[(f64, f64)], shifts: &[(f64, f64)]) -> Option<(Matrix, f64)> {
    if moves.len() != shifts.len() || moves.len() < 2 {
        return None;
    }
    // The normal equations: N = −(Σ d sᵀ)(Σ s sᵀ)⁻¹.
    let (mut ds, mut ss) = ([[0.0; 2]; 2], [[0.0; 2]; 2]);
    for (s, d) in moves.iter().zip(shifts) {
        let (s, d) = ([s.0, s.1], [d.0, d.1]);
        for r in 0..2 {
            for c in 0..2 {
                ds[r][c] += d[r] * s[c];
                ss[r][c] += s[r] * s[c];
            }
        }
    }
    let ss = inverse(ss)?;
    let mut n = [[0.0; 2]; 2];
    for r in 0..2 {
        for c in 0..2 {
            n[r][c] = -(ds[r][0] * ss[0][c] + ds[r][1] * ss[1][c]);
        }
    }
    let squares: f64 = moves
        .iter()
        .zip(shifts)
        .map(|(s, d)| {
            let ns = apply(n, *s);
            (d.0 + ns.0).powi(2) + (d.1 + ns.1).powi(2)
        })
        .sum();
    Some((inverse(n)?, (squares / moves.len() as f64).sqrt()))
}

/// Each move's residual under `matrix`, M (W8.2): `d + M⁻¹ · s`, the shift
/// the fit leaves unexplained, in pixels, for each move `s` and its shift
/// `d`. `None` if M can't be inverted.
pub fn residuals(
    matrix: Matrix,
    moves: &[(f64, f64)],
    shifts: &[(f64, f64)],
) -> Option<Vec<[f64; 2]>> {
    let n = inverse(matrix)?;
    let each = |(s, d): (&(f64, f64), &(f64, f64))| {
        let ns = apply(n, *s);
        [d.0 + ns.0, d.1 + ns.1]
    };
    Some(moves.iter().zip(shifts).map(each).collect())
}

/// What M decomposes into: M = R(θ) · [[s_x, k], [0, s_y]] · diag(m, 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decomposition {
    /// θ, in degrees, from −180 to 180.
    pub rotation_deg: f64,
    /// m = −1: the image mirrored, its x reversed.
    pub mirrored: bool,
    /// s_x, in µm.
    pub sx: f64,
    /// s_y, in µm.
    pub sy: f64,
    /// k, in µm.
    pub k: f64,
    /// The pixel size, √(s_x s_y).
    pub pixel: f64,
    /// s_x / s_y.
    pub aspect: f64,
    /// k over the pixel size.
    pub skew: f64,
}

/// `m`'s decomposition, from the QR factorisation of `m · diag(m, 1)`, whose
/// determinant is positive, by Gram and Schmidt's steps. `None` if `m`
/// can't be inverted.
pub fn decompose(m: Matrix) -> Option<Decomposition> {
    let d = det(m);
    if d == 0.0 || !d.is_finite() {
        return None;
    }
    let flip = d.signum();
    let c1 = (flip * m[0][0], flip * m[1][0]);
    let c2 = (m[0][1], m[1][1]);
    let sx = c1.0.hypot(c1.1);
    let q1 = (c1.0 / sx, c1.1 / sx);
    let k = q1.0 * c2.0 + q1.1 * c2.1;
    let rest = (c2.0 - k * q1.0, c2.1 - k * q1.1);
    let sy = rest.0.hypot(rest.1);
    let pixel = (sx * sy).sqrt();
    Some(Decomposition {
        rotation_deg: q1.1.atan2(q1.0).to_degrees(),
        mirrored: flip < 0.0,
        sx,
        sy,
        k,
        pixel,
        aspect: sx / sy,
        skew: k / pixel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M = R(θ) · [[sx, k], [0, sy]] · diag(m, 1).
    fn compose(theta_deg: f64, mirrored: bool, sx: f64, sy: f64, k: f64) -> Matrix {
        let (sin, cos) = theta_deg.to_radians().sin_cos();
        let u = [[sx, k], [0.0, sy]];
        let m = if mirrored { -1.0 } else { 1.0 };
        let r = [[cos, -sin], [sin, cos]];
        let mut out = [[0.0; 2]; 2];
        for i in 0..2 {
            for j in 0..2 {
                let ru = r[i][0] * u[0][j] + r[i][1] * u[1][j];
                out[i][j] = ru * if j == 0 { m } else { 1.0 };
            }
        }
        out
    }

    #[test]
    fn a_decomposition_gives_back_what_made_the_matrix() {
        for theta in [-179.0, -90.0, -30.0, 0.0, 12.5, 90.0, 150.0, 180.0] {
            for mirrored in [false, true] {
                for (sx, sy, k) in [(0.24, 0.24, 0.0), (0.5, 0.45, 0.01), (0.048, 0.05, -0.002)] {
                    let found = decompose(compose(theta, mirrored, sx, sy, k)).unwrap();
                    let turn = (found.rotation_deg - theta + 540.0).rem_euclid(360.0) - 180.0;
                    assert!(turn.abs() < 1e-9, "{theta}° {mirrored}: {found:?}");
                    assert_eq!(found.mirrored, mirrored);
                    let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
                    assert!(close(found.sx, sx) && close(found.sy, sy) && close(found.k, k));
                    assert!(close(found.pixel, (sx * sy).sqrt()));
                }
            }
        }
        assert_eq!(decompose([[1.0, 2.0], [2.0, 4.0]]), None);
    }

    #[test]
    fn the_fit_recovers_a_matrix_from_exact_shifts() {
        let m = compose(30.0, true, 0.2331, 0.2331, 0.0);
        let n = inverse(m).unwrap();
        let moves: Vec<(f64, f64)> = (-1..=1)
            .flat_map(|j| (-1..=1).map(move |i| (f64::from(i) * 123.0, f64::from(j) * 123.0)))
            .filter(|s| *s != (0.0, 0.0))
            .collect();
        // d = −N s.
        let shifts: Vec<(f64, f64)> = moves
            .iter()
            .map(|s| {
                let d = apply(n, *s);
                (-d.0, -d.1)
            })
            .collect();
        let (found, residual) = fit_matrix(&moves, &shifts).unwrap();
        for (row, want) in found.iter().zip(&m) {
            for (a, b) in row.iter().zip(want) {
                assert!((a - b).abs() < 1e-12, "{found:?} against {m:?}");
            }
        }
        assert!(residual < 1e-9);
        // Each move's residual: none, exactly; one shift nudged, theirs
        // the fit's RMS over the moves, the nudged one's the largest.
        let exact = residuals(found, &moves, &shifts).unwrap();
        assert_eq!(exact.len(), moves.len());
        assert!(exact.iter().flatten().all(|r| r.abs() < 1e-9));
        let mut nudged = shifts.clone();
        nudged[2] = (nudged[2].0 + 0.6, nudged[2].1 - 0.3);
        let (fitted, rms) = fit_matrix(&moves, &nudged).unwrap();
        let each = residuals(fitted, &moves, &nudged).unwrap();
        let mean_square =
            each.iter().map(|r| r[0] * r[0] + r[1] * r[1]).sum::<f64>() / each.len() as f64;
        assert!(
            (mean_square.sqrt() - rms).abs() < 1e-12,
            "{each:?} against {rms}"
        );
        let size = |r: &[f64; 2]| r[0].hypot(r[1]);
        let largest = (0..each.len()).max_by(|&a, &b| size(&each[a]).total_cmp(&size(&each[b])));
        assert_eq!(largest, Some(2));
        // One move, or moves along one line, don't span the plane.
        assert_eq!(fit_matrix(&moves[..1], &shifts[..1]), None);
        let line = [(1.0, 0.0), (2.0, 0.0)];
        assert_eq!(fit_matrix(&line, &line), None);
    }
}
