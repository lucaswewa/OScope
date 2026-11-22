//! Seeded hashing and value noise, so that a sample is the same however it
//! is rendered.
//!
//! **Every random choice is a hash** of the seed, of what it is for (a
//! salt), and of where it is (a grid cell), never a draw from a generator.
//! So a region renders the same alone or as part of a larger one, in any
//! order, on any thread (ADR-0013).

/// SplitMix64's finaliser: mixes 64 bits so that each input bit changes
/// about half the output bits (Steele, Lea and Flood, 2014).
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A hash of a seed, a salt and a grid cell `(i, j)`.
pub fn hash(seed: u64, salt: u64, i: i64, j: i64) -> u64 {
    let s = mix(seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    mix(mix(s ^ i as u64) ^ j as u64)
}

/// A number from 0 up to 1 from a hash's top 53 bits.
pub fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1_u64 << 53) as f64
}

/// The `n`-th of several numbers from 0 up to 1 drawn from one hash.
pub fn nth(h: u64, n: u64) -> f64 {
    unit(mix(h ^ n.wrapping_mul(0xd6e8_feb8_6659_fd93)))
}

/// Perlin's quintic fade: 0 at 0, 1 at 1, with flat first and second
/// derivatives at both ends (Perlin, 2002).
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Smooth value noise from 0 to 1, with features about `wavelength` apart:
/// a random value at each corner of a square grid, blended with the fade.
pub fn value(seed: u64, salt: u64, x: f64, y: f64, wavelength: f64) -> f64 {
    let (u, v) = (x / wavelength, y / wavelength);
    let (i, j) = (u.floor(), v.floor());
    let (fx, fy) = (fade(u - i), fade(v - j));
    let (i, j) = (i as i64, j as i64);
    let corner = |di, dj| unit(hash(seed, salt, i + di, j + dj));
    lerp(
        lerp(corner(0, 0), corner(1, 0), fx),
        lerp(corner(0, 1), corner(1, 1), fx),
        fy,
    )
}

/// Fractal noise from 0 to 1: `octaves` layers of value noise from
/// `wavelength` down, each half the wavelength and half the weight of the
/// one before (Ebert et al., *Texturing and Modeling*).
///
/// **The level of detail:** an octave shorter than half a pixel of side
/// `pixel` is replaced by its mean, 0.5, fading in up to a whole pixel.
/// Value noise is smooth within each cell of its grid, so a longer octave,
/// taken at the pixel's centre, is already close to its mean over the
/// pixel; a much shorter one, taken there, would only add speckle. Measured
/// against fine renderings averaged down, this came closer than fading from
/// two or four pixels, or not at all (P10's notes).
pub fn fractal(
    seed: u64,
    salt: u64,
    x: f64,
    y: f64,
    wavelength: f64,
    octaves: u32,
    pixel: f64,
) -> f64 {
    let (mut sum, mut total, mut weight, mut w) = (0.0, 0.0, 1.0, wavelength);
    for octave in 0..octaves {
        let shown = (2.0 * w / pixel - 1.0).clamp(0.0, 1.0);
        if shown > 0.0 {
            let n = value(seed, salt.wrapping_add(u64::from(octave)), x, y, w);
            sum += weight * (shown * n + (1.0 - shown) * 0.5);
        } else {
            sum += weight * 0.5;
        }
        total += weight;
        weight *= 0.5;
        w *= 0.5;
    }
    sum / total
}

/// How much of a pixel of side `pixel` a shape covers, at signed distance
/// `distance` from the shape's edge (negative inside): a box filter across
/// a straight edge.
pub fn coverage(distance: f64, pixel: f64) -> f64 {
    (0.5 - distance / pixel).clamp(0.0, 1.0)
}

/// The mean over `[from, to)` of a pattern that is 1 on `[k·period,
/// k·period + width)` for every whole `k`, and 0 elsewhere, within `[0,
/// extent)`; 0 outside it. Exact, for any interval.
pub fn stripes(from: f64, to: f64, period: f64, width: f64, extent: f64) -> f64 {
    // The pattern's integral from 0 to t.
    let integral = |t: f64| {
        let t = t.clamp(0.0, extent);
        let k = (t / period).floor();
        k * width + (t - k * period).min(width)
    };
    if to <= from {
        return 0.0;
    }
    (integral(to) - integral(from)) / (to - from)
}
