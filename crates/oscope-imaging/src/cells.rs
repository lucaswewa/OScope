//! Cells (P46): thresholds, the distance transform, connected components,
//! a seeded watershed that splits touching nuclei, each object's measures,
//! and its outline.
//!
//! Images are a value a pixel, row by row; masks a byte a pixel, set when
//! not 0; labels an object's number a pixel, from 1, and 0 for none.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rayon::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The 1-D squared distance transform of `f` (Felzenszwalb and
/// Huttenlocher, 2012): `d(p) = min over q of (p − q)² + f(q)`, by the lower
/// envelope of the parabolas rooted at each `q`.
fn squared_1d(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    let (mut roots, mut bounds) = (Vec::new(), Vec::new());
    let Some(first) = (0..n).find(|&q| f[q].is_finite()) else {
        d.fill(f64::INFINITY);
        return;
    };
    roots.push(first);
    bounds.push(f64::NEG_INFINITY);
    for q in first + 1..n {
        if !f[q].is_finite() {
            continue;
        }
        loop {
            let v = roots[roots.len() - 1];
            let s =
                ((f[q] + (q * q) as f64) - (f[v] + (v * v) as f64)) / (2.0 * (q as f64 - v as f64));
            if s <= bounds[bounds.len() - 1] {
                roots.pop();
                bounds.pop();
                if roots.is_empty() {
                    roots.push(q);
                    bounds.push(f64::NEG_INFINITY);
                    break;
                }
            } else {
                roots.push(q);
                bounds.push(s);
                break;
            }
        }
    }
    let mut k = 0;
    for (p, out) in d.iter_mut().enumerate() {
        while k + 1 < roots.len() && bounds[k + 1] < p as f64 {
            k += 1;
        }
        let q = roots[k];
        *out = (p as f64 - q as f64).powi(2) + f[q];
    }
}

/// The Euclidean distance from each set pixel of `mask`, `width` pixels
/// wide, to the nearest pixel not set, in pixels, exactly: down columns,
/// then along rows. Pixels not set are 0; with none, every distance is the
/// image's diagonal.
pub fn distances(mask: &[u8], width: usize) -> Vec<f32> {
    let height = mask.len().checked_div(width).unwrap_or(0);
    let mut squared: Vec<f64> = mask
        .iter()
        .map(|&bit| if bit == 0 { 0.0 } else { f64::INFINITY })
        .collect();
    let columns: Vec<Vec<f64>> = (0..width)
        .into_par_iter()
        .map(|x| {
            let f: Vec<f64> = (0..height).map(|y| squared[y * width + x]).collect();
            let mut d = vec![0.0; height];
            squared_1d(&f, &mut d);
            d
        })
        .collect();
    for (x, column) in columns.iter().enumerate() {
        for (y, &v) in column.iter().enumerate() {
            squared[y * width + x] = v;
        }
    }
    let diagonal = ((width * width + height * height) as f64).sqrt();
    let mut out = vec![0.0_f32; width * height];
    if width == 0 {
        return out;
    }
    out.par_chunks_mut(width)
        .zip(squared.par_chunks(width))
        .for_each(|(row, f)| {
            let mut d = vec![0.0; width];
            squared_1d(f, &mut d);
            for (out, d) in row.iter_mut().zip(d) {
                *out = if d.is_finite() { d.sqrt() } else { diagonal } as f32;
            }
        });
    out
}

/// The root of `i` among `parents`, halving the path as it goes.
fn root(parents: &mut [u32], mut i: u32) -> u32 {
    while parents[i as usize] != i {
        parents[i as usize] = parents[parents[i as usize] as usize];
        i = parents[i as usize];
    }
    i
}

/// Each set pixel's component, from 1 in the order the components first
/// appear row by row, and 0 elsewhere, joined by sides, or by corners too
/// if `eight`: two passes with union-find (Rosenfeld and Pfaltz, 1966).
pub fn components(mask: &[u8], width: usize, eight: bool) -> (Vec<u32>, u32) {
    let height = mask.len().checked_div(width).unwrap_or(0);
    let mut labels = vec![0_u32; width * height];
    let mut parents = vec![0_u32];
    let before: &[(i64, i64)] = match eight {
        true => &[(-1, 0), (-1, -1), (0, -1), (1, -1)],
        false => &[(-1, 0), (0, -1)],
    };
    for y in 0..height {
        for x in 0..width {
            if mask[y * width + x] == 0 {
                continue;
            }
            let mut found = 0;
            for &(dx, dy) in before {
                let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                if xx < 0 || yy < 0 || xx >= width as i64 {
                    continue;
                }
                let theirs = labels[yy as usize * width + xx as usize];
                if theirs == 0 {
                    continue;
                }
                if found == 0 {
                    found = theirs;
                } else {
                    let (a, b) = (root(&mut parents, found), root(&mut parents, theirs));
                    parents[a.max(b) as usize] = a.min(b);
                }
            }
            if found == 0 {
                found = parents.len() as u32;
                parents.push(found);
            }
            labels[y * width + x] = found;
        }
    }
    let mut numbers = vec![0_u32; parents.len()];
    let mut count = 0;
    for label in labels.iter_mut().filter(|label| **label != 0) {
        let top = root(&mut parents, *label) as usize;
        if numbers[top] == 0 {
            count += 1;
            numbers[top] = count;
        }
        *label = numbers[top];
    }
    (labels, count)
}

/// Otsu's level (1979) over `values`' range, of a histogram of `bins`: the
/// split that maximises the variance between the two classes, the first of
/// a tie. Values at or above it are the upper class.
pub fn otsu(values: &[f32], bins: usize) -> f32 {
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (lo, hi) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    if lo >= hi {
        return lo;
    }
    let bins = bins.max(2);
    let scale = bins as f32 / (hi - lo);
    let mut histogram = vec![0_u64; bins];
    for &v in values.iter().filter(|v| v.is_finite()) {
        histogram[(((v - lo) * scale) as usize).min(bins - 1)] += 1;
    }
    let total: u64 = histogram.iter().sum();
    let sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(k, &n)| k as f64 * n as f64)
        .sum();
    let (mut below, mut below_sum, mut best, mut split) = (0_u64, 0.0, -1.0, 1);
    for (k, &n) in histogram.iter().enumerate().take(bins - 1) {
        below += n;
        below_sum += k as f64 * n as f64;
        let above = total - below;
        if below == 0 || above == 0 {
            continue;
        }
        let (m0, m1) = (below_sum / below as f64, (sum - below_sum) / above as f64);
        let between = below as f64 * above as f64 * (m0 - m1).powi(2);
        if between > best {
            (best, split) = (between, k + 1);
        }
    }
    lo + split as f32 / scale
}

/// The mask of `values`, `width` wide, that exceed the mean of the square
/// window of side 2 × `radius` + 1 about them, cut at the edges, by
/// `offset`: the means from a summed-area table (Crow, 1984).
pub fn adaptive(values: &[f32], width: usize, radius: usize, offset: f32) -> Vec<u8> {
    let height = values.len().checked_div(width).unwrap_or(0);
    let mut table = vec![0.0_f64; (width + 1) * (height + 1)];
    for y in 0..height {
        let mut row = 0.0;
        for x in 0..width {
            row += f64::from(values[y * width + x]);
            table[(y + 1) * (width + 1) + x + 1] = table[y * (width + 1) + x + 1] + row;
        }
    }
    let at = |x: usize, y: usize| table[y * (width + 1) + x];
    (0..width * height)
        .map(|i| {
            let (x, y) = (i % width, i / width);
            let (x0, y0) = (x.saturating_sub(radius), y.saturating_sub(radius));
            let (x1, y1) = ((x + radius + 1).min(width), (y + radius + 1).min(height));
            let sum = at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0);
            let mean = sum / ((x1 - x0) * (y1 - y0)) as f64;
            u8::from(f64::from(values[i]) > mean + f64::from(offset))
        })
        .collect()
}

/// `mask`, `width` wide, with its holes filled: the parts of its
/// background, joined by sides, that don't reach its edge.
pub fn fill_holes(mask: &[u8], width: usize) -> Vec<u8> {
    let background: Vec<u8> = mask.iter().map(|&b| u8::from(b == 0)).collect();
    let (parts, count) = components(&background, width, false);
    let height = mask.len().checked_div(width).unwrap_or(0);
    let mut edge = vec![false; count as usize + 1];
    for (i, &part) in parts.iter().enumerate() {
        let (x, y) = (i % width, i / width);
        if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
            edge[part as usize] = true;
        }
    }
    let filled = mask
        .iter()
        .zip(&parts)
        .map(|(&b, &part)| u8::from(b != 0 || !edge[part as usize]));
    filled.collect()
}

/// `mask`, `width` wide, split into objects by a seeded watershed (Beucher
/// and Meyer, 1993) on its `distance` transform: its seeds the peaks at
/// least `min_distance` deep with none higher within `seed_radius`, a
/// plateau's peaks one seed; flooded by Meyer's algorithm (1994), the
/// deepest pixels first, first come first served, through the mask's sides.
/// A part of the mask without a seed is an object of its own. The labels,
/// and how many.
pub fn watershed(
    mask: &[u8],
    distance: &[f32],
    width: usize,
    seed_radius: usize,
    min_distance: f32,
) -> (Vec<u32>, u32) {
    let height = mask.len().checked_div(width).unwrap_or(0);
    let r = seed_radius as i64;
    let peaks: Vec<u8> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let d = distance[i];
            if mask[i] == 0 || d < min_distance {
                return 0;
            }
            let (x, y) = ((i % width) as i64, (i / width) as i64);
            let higher = (-r..=r).any(|dy| {
                (-r..=r).any(|dx| {
                    let (xx, yy) = (x + dx, y + dy);
                    (0..width as i64).contains(&xx)
                        && (0..height as i64).contains(&yy)
                        && distance[yy as usize * width + xx as usize] > d
                })
            });
            u8::from(!higher)
        })
        .collect();
    let (mut labels, mut count) = components(&peaks, width, true);
    let mut queue = BinaryHeap::new();
    let mut order = 0_u64;
    for (i, &label) in labels.iter().enumerate() {
        if label != 0 {
            queue.push((distance[i].to_bits(), Reverse(order), i));
            order += 1;
        }
    }
    while let Some((_, _, i)) = queue.pop() {
        let (x, y) = (i % width, i / width);
        let sides = [
            (x > 0).then(|| i - 1),
            (x + 1 < width).then(|| i + 1),
            (y > 0).then(|| i - width),
            (y + 1 < height).then(|| i + width),
        ];
        for j in sides.into_iter().flatten() {
            if mask[j] != 0 && labels[j] == 0 {
                labels[j] = labels[i];
                queue.push((distance[j].to_bits(), Reverse(order), j));
                order += 1;
            }
        }
    }
    // What no seed reached: an object each part.
    let rest: Vec<u8> = mask
        .iter()
        .zip(&labels)
        .map(|(&b, &l)| u8::from(b != 0 && l == 0))
        .collect();
    let (parts, more) = components(&rest, width, false);
    for (label, &part) in labels.iter_mut().zip(&parts) {
        if part != 0 {
            *label = count + part;
        }
    }
    count += more;
    (labels, count)
}

/// How a channel is thresholded into a mask.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Threshold {
    /// Otsu's level, over the values thresholded.
    Otsu,
    /// A level given: the values at or above it.
    Level(f32),
    /// The values over their window's mean by `offset`, the window of side
    /// 2 × `radius` + 1.
    Adaptive {
        /// The window's half side, in pixels.
        radius: usize,
        /// How far over its window's mean a value must be.
        offset: f32,
    },
}

/// The objects of `values`, `width` wide: thresholded by `threshold`,
/// holes filled, and split by the seeded watershed with `seed_radius` and
/// `min_distance`. Their labels, and how many.
pub fn segment(
    values: &[f32],
    width: usize,
    threshold: Threshold,
    seed_radius: usize,
    min_distance: f32,
) -> (Vec<u32>, u32) {
    let at_least = |level: f32| values.iter().map(|&v| u8::from(v >= level)).collect();
    let mask: Vec<u8> = match threshold {
        Threshold::Otsu => at_least(otsu(values, 256)),
        Threshold::Level(level) => at_least(level),
        Threshold::Adaptive { radius, offset } => adaptive(values, width, radius, offset),
    };
    let mask = fill_holes(&mask, width);
    watershed(
        &mask,
        &distances(&mask, width),
        width,
        seed_radius,
        min_distance,
    )
}

/// An object's measures, in µm.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Its number in the labels.
    pub label: u32,
    /// Its pixels.
    pub pixels: u64,
    /// Its area, in µm².
    pub area: f64,
    /// Its perimeter, in µm: its pixels' edges with what isn't it, times
    /// π/4.
    pub perimeter: f64,
    /// 4π · area / perimeter²: 1 for a disc.
    pub circularity: f64,
    /// Its pixels' mean centre, in µm from the image's top-left corner.
    pub centroid: [f64; 2],
    /// Each channel's mean over it.
    pub means: Vec<f64>,
    /// Its leftmost, topmost pixel, where its outline starts.
    pub first: (usize, usize),
}

/// The measures of objects 1 to `count` of `labels`, `width` wide, of
/// pixels `pixel` µm across and down, and their means in each of
/// `channels`, images of the labels' size.
pub fn measure(
    labels: &[u32],
    width: usize,
    count: u32,
    pixel: [f64; 2],
    channels: &[&[f32]],
) -> Vec<Object> {
    let height = labels.len().checked_div(width).unwrap_or(0);
    let n = count as usize;
    let (mut pixels, mut edges) = (vec![0_u64; n + 1], vec![[0_u64; 2]; n + 1]);
    let mut sums = vec![[0.0_f64; 2]; n + 1];
    let mut means = vec![vec![0.0_f64; channels.len()]; n + 1];
    let mut first = vec![None; n + 1];
    for (i, &l) in labels.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let (x, y) = (i % width, i / width);
        let l = l as usize;
        pixels[l] += 1;
        sums[l][0] += x as f64 + 0.5;
        sums[l][1] += y as f64 + 0.5;
        for (m, c) in means[l].iter_mut().zip(channels) {
            *m += f64::from(c[i]);
        }
        first[l].get_or_insert((x, y));
        // Its sides with what isn't it: left and right edges are pixel[1]
        // long, top and bottom ones pixel[0].
        let other = |j: Option<usize>| j.is_none_or(|j| labels[j] as usize != l);
        edges[l][1] += u64::from(other((x > 0).then(|| i - 1)))
            + u64::from(other((x + 1 < width).then(|| i + 1)));
        edges[l][0] += u64::from(other((y > 0).then(|| i - width)))
            + u64::from(other((y + 1 < height).then(|| i + width)));
    }
    (1..=n)
        .filter(|&l| pixels[l] > 0)
        .map(|l| {
            let p = pixels[l] as f64;
            let area = p * pixel[0] * pixel[1];
            let crack = edges[l][0] as f64 * pixel[0] + edges[l][1] as f64 * pixel[1];
            let perimeter = crack * std::f64::consts::FRAC_PI_4;
            Object {
                label: l as u32,
                pixels: pixels[l],
                area,
                perimeter,
                circularity: 4.0 * std::f64::consts::PI * area / (perimeter * perimeter),
                centroid: [sums[l][0] / p * pixel[0], sums[l][1] / p * pixel[1]],
                means: means[l].iter().map(|m| m / p).collect(),
                first: first[l].unwrap_or_default(),
            }
        })
        .collect()
}

/// Which objects to keep: by area, in µm², and circularity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filters {
    /// The least area: 10 µm².
    pub min_area: f64,
    /// The greatest: 200 µm².
    pub max_area: f64,
    /// The least circularity: 0.4.
    pub min_circularity: f64,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            min_area: 10.0,
            max_area: 200.0,
            min_circularity: 0.4,
        }
    }
}

impl Filters {
    /// Whether `object` is kept.
    pub fn keep(&self, object: &Object) -> bool {
        (self.min_area..=self.max_area).contains(&object.area)
            && object.circularity >= self.min_circularity
    }
}

/// The outer boundary of `object` in `labels`, `width` wide, along its
/// pixels' edges, clockwise on the screen from its first pixel's top-left
/// corner: its corners, in pixels from the image's top-left corner, the
/// last the first. Straight runs are one edge; holes are left out.
pub fn outline(labels: &[u32], width: usize, object: &Object) -> Vec<[f64; 2]> {
    let height = labels.len().checked_div(width).unwrap_or(0) as i64;
    let inside = |x: i64, y: i64| {
        (0..width as i64).contains(&x)
            && (0..height).contains(&y)
            && labels[y as usize * width + x as usize] == object.label
    };
    // East, south, west, north; the object on the right of each step, and
    // the pixels ahead to the left and to the right of a corner.
    const STEPS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let ahead = |(cx, cy): (i64, i64), d: usize| match d {
        0 => ((cx, cy - 1), (cx, cy)),
        1 => ((cx, cy), (cx - 1, cy)),
        2 => ((cx - 1, cy), (cx - 1, cy - 1)),
        _ => ((cx - 1, cy - 1), (cx, cy - 1)),
    };
    let start = (object.first.0 as i64, object.first.1 as i64);
    let (mut at, mut d) = (start, 0);
    let mut corners = vec![[start.0 as f64, start.1 as f64]];
    // A boundary has four edges a pixel at most: a bound on the walk.
    for _ in 0..4 * (object.pixels + 1) {
        at = (at.0 + STEPS[d].0, at.1 + STEPS[d].1);
        let (left, right) = ahead(at, d);
        let turn = match (inside(left.0, left.1), inside(right.0, right.1)) {
            (_, false) => (d + 1) % 4,
            (true, true) => (d + 3) % 4,
            (false, true) => d,
        };
        if at == start && turn == 0 {
            break;
        }
        if turn != d {
            corners.push([at.0 as f64, at.1 as f64]);
        }
        d = turn;
    }
    corners.push(corners[0]);
    corners
}
