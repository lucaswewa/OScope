//! Stain analysis (P45): optical density, stains' vectors, colour
//! deconvolution (Ruifrok and Johnston, 2001), Macenko's estimate of two
//! stains' vectors (2009), and DAB's bins.
//!
//! Pixels are interleaved red, green and blue: the light each passed, in
//! any unit, which a white, the light through glass, scales.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A stain's optical densities in red, green and blue, for a unit of it.
pub type Vector = [f64; 3];

/// Hematoxylin's vector (Ruifrok and Johnston, 2001).
pub const HEMATOXYLIN: Vector = [0.650, 0.704, 0.286];
/// Eosin's vector.
pub const EOSIN: Vector = [0.072, 0.990, 0.105];
/// DAB's vector.
pub const DAB: Vector = [0.268, 0.570, 0.776];

/// The least light a pixel is taken to pass, as a share of the white: an
/// optical density of 3.01 at most.
pub const FLOOR: f32 = 1.0 / 1024.0;

/// The optical density, summed over a pixel's channels, from which it is
/// tissue rather than glass.
pub const TISSUE: f32 = 0.05;

/// Why stains can't be separated.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StainError {
    /// Vectors that can't be stains', saying why.
    #[error("{0}")]
    Vectors(String),
    /// Too few pixels dense enough to estimate from.
    #[error("{0}")]
    TooFew(String),
}

/// The optical density of a channel that passed light `i` of white `w`.
pub fn optical_density(i: f32, w: f32) -> f32 {
    -(i.max(w * FLOOR) / w).log10()
}

/// A pixel's optical densities, against `white`.
fn densities(pixel: &[f32; 3], white: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|c| optical_density(pixel[c], white[c]))
}

fn norm(v: Vector) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn dot(a: Vector, b: Vector) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Which stains: H&E, H-DAB, or vectors given.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StainChoice {
    /// Hematoxylin and eosin.
    #[default]
    He,
    /// Hematoxylin and DAB.
    Hdab,
    /// Two vectors or three, the second the stain DAB is measured by.
    Vectors(Vec<Vector>),
}

impl StainChoice {
    /// Its stains, or why its vectors aren't.
    pub fn stains(&self) -> Result<Stains, StainError> {
        match self {
            Self::He => Ok(Stains::he()),
            Self::Hdab => Ok(Stains::hdab()),
            Self::Vectors(v) => match v.as_slice() {
                [a, b] => Stains::new(*a, *b, None),
                [a, b, c] => Stains::new(*a, *b, Some(*c)),
                _ => Err(StainError::Vectors(format!(
                    "two stains' vectors or three, not {}",
                    v.len()
                ))),
            },
        }
    }
}

/// Stains by their vectors, normalised: the rows of the matrix M of
/// Beer–Lambert's OD = a · M.
#[derive(Clone, Debug, PartialEq)]
pub struct Stains {
    vectors: [Vector; 3],
    inverse: [[f64; 3]; 3],
}

impl Stains {
    /// Stains of vectors `first`, `second`, and `third`, or with none the
    /// cross product of the two, normalised: a residual, what neither
    /// explains. Refused: vectors not independent, or a given one with a
    /// component negative or not finite.
    pub fn new(first: Vector, second: Vector, third: Option<Vector>) -> Result<Self, StainError> {
        let unit = |v: Vector| -> Result<Vector, StainError> {
            let n = norm(v);
            if v.iter().any(|x| !x.is_finite() || *x < 0.0) || n == 0.0 {
                return Err(StainError::Vectors(format!(
                    "a stain's vector has three finite components, none negative: {v:?}"
                )));
            }
            Ok(v.map(|x| x / n))
        };
        let (a, b) = (unit(first)?, unit(second)?);
        let c = match third {
            Some(v) => unit(v)?,
            None => {
                let cross = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                let n = norm(cross);
                cross.map(|x| if n > 0.0 { x / n } else { x })
            }
        };
        let m = [a, b, c];
        // The inverse by the adjugate: each entry a cofactor over det M.
        let cofactor = |r: usize, k: usize| {
            let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
            let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
            m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
        };
        let det: f64 = (0..3).map(|k| m[0][k] * cofactor(0, k)).sum();
        if det.abs() < 1e-3 {
            return Err(StainError::Vectors(format!(
                "the stains' vectors aren't independent (det {det:.2e}): {m:?}"
            )));
        }
        let inverse = [0, 1, 2].map(|i| [0, 1, 2].map(|j| cofactor(j, i) / det));
        Ok(Self {
            vectors: m,
            inverse,
        })
    }

    /// Hematoxylin and eosin.
    pub fn he() -> Self {
        Self::new(HEMATOXYLIN, EOSIN, None).expect("H&E's vectors are independent")
    }

    /// Hematoxylin and DAB.
    pub fn hdab() -> Self {
        Self::new(HEMATOXYLIN, DAB, None).expect("H-DAB's vectors are independent")
    }

    /// The vectors, normalised: the first stain's, the second's, and the
    /// third's or the residual's.
    pub fn vectors(&self) -> [Vector; 3] {
        self.vectors
    }

    /// The amounts of each stain that give optical densities `od`:
    /// a = OD · M⁻¹.
    pub fn amounts(&self, od: [f32; 3]) -> [f32; 3] {
        let od = od.map(f64::from);
        [0, 1, 2].map(|j| (0..3).map(|c| od[c] * self.inverse[c][j]).sum::<f64>() as f32)
    }

    /// Each pixel's amounts of each stain, interleaved, from `rgb`'s light
    /// against `white`.
    pub fn deconvolve(&self, rgb: &[f32], white: [f32; 3]) -> Vec<f32> {
        let (pixels, _) = rgb.as_chunks::<3>();
        pixels
            .iter()
            .flat_map(|p| self.amounts(densities(p, white)))
            .collect()
    }

    /// Stain `k` alone in each pixel of `amounts`: the light it passes, of
    /// `white`, interleaved RGB.
    pub fn alone(&self, k: usize, amounts: &[f32], white: [f32; 3]) -> Vec<f32> {
        let v = self.vectors[k];
        let (pixels, _) = amounts.as_chunks::<3>();
        pixels
            .iter()
            .flat_map(|a| [0, 1, 2].map(|c| white[c] * 10f32.powf(-a[k] * v[c] as f32)))
            .collect()
    }
}

/// The eigenvalues and eigenvectors of the symmetric `a` by Jacobi's
/// method (Golub and Van Loan, 2013, §8.5): each vector a column of the
/// second.
fn eigen(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..64 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        let diagonal = a[0][0].abs() + a[1][1].abs() + a[2][2].abs();
        if off <= 1e-15 * diagonal {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            // The rotation in the plane (p, q) that zeroes a[p][q].
            let tau = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = tau.signum() / (tau.abs() + (1.0 + tau * tau).sqrt());
            let c = 1.0 / (1.0 + t * t).sqrt();
            let s = t * c;
            // A ← Jᵀ A J, and V ← V J: columns p and q turned, then rows.
            let turn = |row: &mut [f64; 3]| {
                let (kp, kq) = (row[p], row[q]);
                (row[p], row[q]) = (c * kp - s * kq, s * kp + c * kq);
            };
            a.iter_mut().for_each(turn);
            let (rp, rq) = (a[p], a[q]);
            a[p] = [0, 1, 2].map(|k| c * rp[k] - s * rq[k]);
            a[q] = [0, 1, 2].map(|k| s * rp[k] + c * rq[k]);
            v.iter_mut().for_each(turn);
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

/// Macenko's estimate of two stains' vectors from the pixels of `rgb`
/// (2009): of the pixels whose optical densities, as a vector, are at
/// least `beta` long, the plane of their densities' two largest second
/// moments about the origin, and in it, the vectors at the `percentile`th
/// and the (100 − `percentile`)th of their angles. The one with more red
/// comes first. Refused from fewer than 100 such pixels.
pub fn macenko(
    rgb: &[f32],
    white: [f32; 3],
    beta: f32,
    percentile: f64,
) -> Result<[Vector; 2], StainError> {
    let (pixels, _) = rgb.as_chunks::<3>();
    let dense: Vec<Vector> = pixels
        .iter()
        .map(|p| densities(p, white))
        .map(|od| od.map(f64::from))
        .filter(|&od| norm(od) >= f64::from(beta))
        .collect();
    if dense.len() < 100 {
        return Err(StainError::TooFew(format!(
            "{} pixels are at least {beta} dense, of 100 needed",
            dense.len()
        )));
    }
    let mut moments = [[0.0; 3]; 3];
    for od in &dense {
        for (i, row) in moments.iter_mut().enumerate() {
            for (j, m) in row.iter_mut().enumerate() {
                *m += od[i] * od[j];
            }
        }
    }
    let (values, vectors) = eigen(moments);
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| values[j].total_cmp(&values[i]));
    let column = |k: usize| [vectors[0][k], vectors[1][k], vectors[2][k]];
    let (e1, e2) = (column(order[0]), column(order[1]));
    let mut angles: Vec<f64> = dense
        .iter()
        .map(|&od| dot(od, e2).atan2(dot(od, e1)))
        .collect();
    angles.sort_by(f64::total_cmp);
    let at = |p: f64| angles[((p / 100.0) * (angles.len() - 1) as f64).round() as usize];
    let vector = |phi: f64| {
        let v = [0, 1, 2].map(|c| phi.cos() * e1[c] + phi.sin() * e2[c]);
        let (n, sign) = (norm(v), v.iter().sum::<f64>().signum());
        v.map(|x| sign * x / n)
    };
    let (a, b) = (vector(at(percentile)), vector(at(100.0 - percentile)));
    Ok(if a[0] >= b[0] { [a, b] } else { [b, a] })
}

/// The DAB amounts from which a tissue pixel is weak, moderate and strong.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DabThresholds {
    /// From here, weak: 0.15.
    pub weak: f32,
    /// From here, moderate: 0.35.
    pub moderate: f32,
    /// From here, strong: 0.60.
    pub strong: f32,
}

impl Default for DabThresholds {
    fn default() -> Self {
        Self {
            weak: 0.15,
            moderate: 0.35,
            strong: 0.60,
        }
    }
}

/// A pixel's class: 0 glass, 1 negative, 2 weak, 3 moderate, 4 strong.
pub fn dab_class(od_sum: f32, dab: f32, thresholds: &DabThresholds) -> u8 {
    match od_sum {
        s if s < TISSUE => 0,
        _ if dab >= thresholds.strong => 4,
        _ if dab >= thresholds.moderate => 3,
        _ if dab >= thresholds.weak => 2,
        _ => 1,
    }
}

/// Each pixel of `rgb`'s class, by `stains`' second stain, DAB, against
/// `white`.
pub fn dab_classes(
    stains: &Stains,
    rgb: &[f32],
    white: [f32; 3],
    thresholds: &DabThresholds,
) -> Vec<u8> {
    let (pixels, _) = rgb.as_chunks::<3>();
    pixels
        .iter()
        .map(|p| {
            let od = densities(p, white);
            let dab = stains.amounts(od)[1];
            dab_class(od.iter().sum(), dab, thresholds)
        })
        .collect()
}

/// Pixels by class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DabCounts {
    /// Glass.
    pub glass: u64,
    /// Tissue without DAB enough to be weak.
    pub negative: u64,
    /// Weak.
    pub weak: u64,
    /// Moderate.
    pub moderate: u64,
    /// Strong.
    pub strong: u64,
}

impl DabCounts {
    /// Counts a pixel of class `class`.
    pub fn add(&mut self, class: u8) {
        match class {
            0 => self.glass += 1,
            1 => self.negative += 1,
            2 => self.weak += 1,
            3 => self.moderate += 1,
            _ => self.strong += 1,
        }
    }

    /// The tissue's pixels.
    pub fn tissue(&self) -> u64 {
        self.negative + self.weak + self.moderate + self.strong
    }

    /// The positive pixels' percentage of the tissue's: 0 without tissue.
    pub fn positive_percent(&self) -> f64 {
        self.percent(self.weak + self.moderate + self.strong)
    }

    /// The H-score, from 0 to 300: 1 × weak % + 2 × moderate % + 3 × strong %.
    pub fn h_score(&self) -> f64 {
        self.percent(self.weak + 2 * self.moderate + 3 * self.strong)
    }

    fn percent(&self, n: u64) -> f64 {
        match self.tissue() {
            0 => 0.0,
            t => 100.0 * n as f64 / t as f64,
        }
    }
}
