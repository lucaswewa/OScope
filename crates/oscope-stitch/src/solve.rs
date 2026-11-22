//! The global solve (P39's spec, 5 and 6): each tile's offset, from the
//! pairs' measured differences and, weakly, the stage's; outliers by
//! iterative reweighting (Holland and Welsch, 1977), then rejected.

use serde::{Deserialize, Serialize};

/// A pair's measured difference of offsets, `x_b − x_a`, and its weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    /// The first tile.
    pub a: usize,
    /// The second.
    pub b: usize,
    /// `x_b − x_a`, in pixels.
    pub shift: [f64; 2],
    /// How much it counts: its registration's confidence.
    pub weight: f64,
}

/// How the solve weighs and rejects, each with a default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SolveOptions {
    /// How much each tile is tied to the stage's offset, against the links'
    /// mean weight: 0.01.
    pub prior: f64,
    /// Huber's k, in pixels: a link missed by more has its weight scaled by
    /// k over the miss. 1.
    pub huber_px: f64,
    /// A link missed by more than this, in pixels, once reweighted, is
    /// rejected: 3.
    pub reject_px: f64,
    /// The most rounds of reweighting: 10.
    pub rounds: u32,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            prior: 0.01,
            huber_px: 1.0,
            reject_px: 3.0,
            rounds: 10,
        }
    }
}

/// The offsets solved, and how well the links fit them.
#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    /// Each tile's offset, in pixels.
    pub offsets: Vec<[f64; 2]>,
    /// The links kept, by index.
    pub kept: Vec<usize>,
    /// The links rejected, by index.
    pub rejected: Vec<usize>,
    /// The kept links' misses' root mean square, in pixels.
    pub rms_px: f64,
}

/// The offsets that best fit `links`, each tile tied to its `nominal`
/// offset by the prior. See [the module](self).
pub fn solve(nominal: &[[f64; 2]], links: &[Link], options: &SolveOptions) -> Solution {
    let mut active: Vec<usize> = (0..links.len())
        .filter(|&k| links[k].weight > 0.0)
        .collect();
    let mut rejected = Vec::new();
    loop {
        let offsets = reweighted(nominal, links, &active, options);
        let miss = |k: usize| distance(&offsets, &links[k]);
        let worst = active
            .iter()
            .copied()
            .filter(|&k| miss(k) > options.reject_px)
            .max_by(|&a, &b| miss(a).total_cmp(&miss(b)));
        // The worst miss goes, one at a time: an outlier drags its
        // neighbours' links off too, until it is gone.
        if let Some(worst) = worst {
            active.retain(|&k| k != worst);
            rejected.push(worst);
            continue;
        }
        let rms_px = match active.len() {
            0 => 0.0,
            n => (active.iter().map(|&k| miss(k).powi(2)).sum::<f64>() / n as f64).sqrt(),
        };
        rejected.sort_unstable();
        return Solution {
            offsets,
            kept: active,
            rejected,
            rms_px,
        };
    }
}

/// How far `offsets` miss `link`, in pixels.
fn distance(offsets: &[[f64; 2]], link: &Link) -> f64 {
    let (a, b) = (offsets[link.a], offsets[link.b]);
    (b[0] - a[0] - link.shift[0]).hypot(b[1] - a[1] - link.shift[1])
}

/// The offsets that fit the `active` links, reweighted by Huber's weight
/// until the weights settle.
fn reweighted(
    nominal: &[[f64; 2]],
    links: &[Link],
    active: &[usize],
    options: &SolveOptions,
) -> Vec<[f64; 2]> {
    let mut scale = vec![1.0_f64; links.len()];
    let mut offsets = nominal.to_vec();
    for _ in 0..options.rounds.max(1) {
        let weighted: Vec<(usize, f64)> = active
            .iter()
            .map(|&k| (k, links[k].weight * scale[k]))
            .collect();
        offsets = least_squares(nominal, links, &weighted, options.prior, &offsets);
        let mut settled = true;
        for &k in active {
            let miss = distance(&offsets, &links[k]);
            let new = if miss > options.huber_px {
                options.huber_px / miss
            } else {
                1.0
            };
            settled &= (new - scale[k]).abs() <= 0.01 * scale[k];
            scale[k] = new;
        }
        if settled {
            break;
        }
    }
    offsets
}

/// The least-squares offsets for the links `weighted` and the prior,
/// along each axis by conjugate gradients with Jacobi's preconditioner,
/// from `start`.
fn least_squares(
    nominal: &[[f64; 2]],
    links: &[Link],
    weighted: &[(usize, f64)],
    prior: f64,
    start: &[[f64; 2]],
) -> Vec<[f64; 2]> {
    let n = nominal.len();
    let mean = match weighted.len() {
        0 => 1.0,
        m => weighted.iter().map(|(_, w)| w).sum::<f64>() / m as f64,
    };
    let lambda = prior * mean;
    // A v = λ v + Σ w (v_b − v_a)(e_b − e_a); its diagonal for Jacobi.
    let apply = |v: &[f64], out: &mut [f64]| {
        for (o, x) in out.iter_mut().zip(v) {
            *o = lambda * x;
        }
        for &(k, w) in weighted {
            let (a, b) = (links[k].a, links[k].b);
            let d = w * (v[b] - v[a]);
            out[b] += d;
            out[a] -= d;
        }
    };
    let mut diagonal = vec![lambda; n];
    for &(k, w) in weighted {
        diagonal[links[k].a] += w;
        diagonal[links[k].b] += w;
    }
    let mut offsets = start.to_vec();
    for axis in 0..2 {
        // b = λ o + Σ w s (e_b − e_a).
        let mut rhs: Vec<f64> = nominal.iter().map(|o| lambda * o[axis]).collect();
        for &(k, w) in weighted {
            rhs[links[k].b] += w * links[k].shift[axis];
            rhs[links[k].a] -= w * links[k].shift[axis];
        }
        let mut x: Vec<f64> = start.iter().map(|o| o[axis]).collect();
        let mut product = vec![0.0; n];
        apply(&x, &mut product);
        let mut r: Vec<f64> = rhs.iter().zip(&product).map(|(b, ax)| b - ax).collect();
        let mut z: Vec<f64> = r.iter().zip(&diagonal).map(|(r, d)| r / d).collect();
        let mut p = z.clone();
        let mut rz: f64 = r.iter().zip(&z).map(|(r, z)| r * z).sum();
        for _ in 0..10 * n.max(10) {
            if r.iter().map(|v| v * v).sum::<f64>().sqrt() < 1e-6 {
                break;
            }
            apply(&p, &mut product);
            let pap: f64 = p.iter().zip(&product).map(|(p, ap)| p * ap).sum();
            if pap <= 0.0 {
                break;
            }
            let alpha = rz / pap;
            for i in 0..n {
                x[i] += alpha * p[i];
                r[i] -= alpha * product[i];
            }
            for (z, (r, d)) in z.iter_mut().zip(r.iter().zip(&diagonal)) {
                *z = r / d;
            }
            let next: f64 = r.iter().zip(&z).map(|(r, z)| r * z).sum();
            let beta = next / rz;
            rz = next;
            for (p, z) in p.iter_mut().zip(&z) {
                *p = z + beta * *p;
            }
        }
        for (o, v) in offsets.iter_mut().zip(x) {
            o[axis] = v;
        }
    }
    offsets
}
