//! A colour target (P33): a grid of patches of known colour, which the
//! simulator renders and calibration measures; and the colour science that
//! judges a measurement:
//!
//! - **CIELAB** from linear sRGB, through XYZ under D65 (IEC 61966-2-1;
//!   CIE 15:2004), white being full transmission;
//! - **ΔE\*ab,** CIE 1976's distance in CIELAB;
//! - **a colour matrix,** fitted by least squares (Hong, Luo and Rhodes,
//!   2001).

/// A patch: where its centre lies from the target's, in µm along the
/// stage's x and y, and its colour, as linear sRGB transmittance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    /// Its centre's x from the target's, in µm.
    pub x_um: f64,
    /// Its centre's y.
    pub y_um: f64,
    /// Its transmittance, linear sRGB, from 0 to 1.
    pub rgb: [f64; 3],
}

/// A colour target: square patches on a square grid, between opaque
/// borders, aligned with the stage.
#[derive(Clone, Debug, PartialEq)]
pub struct ColourTarget {
    /// Patches across.
    pub columns: u32,
    /// Patches down.
    pub rows: u32,
    /// From one patch's centre to the next, in µm.
    pub pitch_um: f64,
    /// A patch's side, in µm.
    pub side_um: f64,
    /// The borders' optical density.
    pub border_od: f64,
    colours: Vec<[f64; 3]>,
}

/// sRGB's transfer: an encoded value to linear light.
fn linear(encoded: f64) -> f64 {
    match encoded <= 0.04045 {
        true => encoded / 12.92,
        false => ((encoded + 0.055) / 1.055).powf(2.4),
    }
}

/// HSV, encoded, to linear sRGB.
fn hsv(h: f64, s: f64, v: f64) -> [f64; 3] {
    let c = v * s;
    let k = |n: f64| {
        let k = (n + h / 60.0).rem_euclid(6.0);
        v - c * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [k(5.0), k(3.0), k(1.0)].map(linear)
}

impl ColourTarget {
    /// OScope's: 6 × 4 patches of 100 µm on a 140 µm pitch, 0.84 × 0.56 mm,
    /// which the 5× sees whole, and the 10× too, however the camera is
    /// turned up to about 50°. Six greys, transmitting 90 % to 5 %; then
    /// eighteen colours at hues 20° apart, alternately strong and pale.
    pub fn oscope() -> Self {
        let greys = [0.9, 0.6, 0.36, 0.2, 0.1, 0.05].map(|g| [g; 3]);
        let colours = (0..18).map(|i| match i % 2 {
            0 => hsv(f64::from(i) * 20.0, 0.7, 0.8),
            _ => hsv(f64::from(i) * 20.0, 0.35, 0.9),
        });
        Self {
            columns: 6,
            rows: 4,
            pitch_um: 140.0,
            side_um: 100.0,
            border_od: 2.0,
            colours: greys.into_iter().chain(colours).collect(),
        }
    }

    /// Its width and height, in µm.
    pub fn size_um(&self) -> (f64, f64) {
        (
            f64::from(self.columns) * self.pitch_um,
            f64::from(self.rows) * self.pitch_um,
        )
    }

    /// Its patches, row by row from the corner of least x and y.
    pub fn patches(&self) -> Vec<Patch> {
        let (w, h) = self.size_um();
        let mut patches = Vec::with_capacity(self.colours.len());
        for (i, rgb) in self.colours.iter().enumerate() {
            let (column, row) = (i as u32 % self.columns, i as u32 / self.columns);
            patches.push(Patch {
                x_um: (f64::from(column) + 0.5) * self.pitch_um - w / 2.0,
                y_um: (f64::from(row) + 0.5) * self.pitch_um - h / 2.0,
                rgb: *rgb,
            });
        }
        patches
    }
}

/// Linear sRGB to XYZ, under D65 (IEC 61966-2-1).
const TO_XYZ: [[f64; 3]; 3] = [
    [0.4124, 0.3576, 0.1805],
    [0.2126, 0.7152, 0.0722],
    [0.0193, 0.1192, 0.9505],
];

/// `rgb`, linear sRGB, in CIELAB (CIE 15:2004), white being `[1, 1, 1]`.
pub fn lab(rgb: [f64; 3]) -> [f64; 3] {
    let xyz = |rgb: [f64; 3]| TO_XYZ.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]);
    let (c, white) = (xyz(rgb), xyz([1.0; 3]));
    let delta: f64 = 6.0 / 29.0;
    let f = |t: f64| match t > delta.powi(3) {
        true => t.cbrt(),
        false => t / (3.0 * delta * delta) + 4.0 / 29.0,
    };
    let [fx, fy, fz] = [0, 1, 2].map(|i| f(c[i] / white[i]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE 1976's colour difference, ΔE\*ab: the distance between two colours
/// in CIELAB.
pub fn delta_e76(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (p, q) = (lab(a), lab(b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

fn inverse3(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let c = |r: usize, k: usize| {
        let (r1, r2, k1, k2) = ((r + 1) % 3, (r + 2) % 3, (k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    // The adjugate is the cofactors' transpose.
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, v) in row.iter_mut().enumerate() {
            *v = c(k, r) / det;
        }
    }
    Some(out)
}

/// The 3 × 3 matrix M that takes `measured` colours nearest `target`'s, by
/// least squares, `target ≈ M · measured`: M = (Σ t mᵀ)(Σ m mᵀ)⁻¹. `None`
/// if the measured colours don't span the space.
pub fn fit_colour_matrix(measured: &[[f64; 3]], target: &[[f64; 3]]) -> Option<[[f64; 3]; 3]> {
    if measured.len() != target.len() || measured.len() < 3 {
        return None;
    }
    let (mut tm, mut mm) = ([[0.0; 3]; 3], [[0.0; 3]; 3]);
    for (m, t) in measured.iter().zip(target) {
        for r in 0..3 {
            for k in 0..3 {
                tm[r][k] += t[r] * m[k];
                mm[r][k] += m[r] * m[k];
            }
        }
    }
    let mm = inverse3(mm)?;
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|j| tm[r][j] * mm[j][k]).sum();
        }
    }
    Some(out)
}

/// `m · rgb`.
pub fn apply3(m: [[f64; 3]; 3], rgb: [f64; 3]) -> [f64; 3] {
    m.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_lightness_100_and_grey_neutral() {
        let white = lab([1.0; 3]);
        assert!((white[0] - 100.0).abs() < 1e-9 && white[1].abs() < 1e-9 && white[2].abs() < 1e-9);
        let grey = lab([0.2; 3]);
        assert!(grey[1].abs() < 1e-9 && grey[2].abs() < 1e-9 && grey[0] > 50.0);
        assert_eq!(delta_e76([0.3, 0.2, 0.1], [0.3, 0.2, 0.1]), 0.0);
        assert!(delta_e76([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]) > 100.0);
    }

    #[test]
    fn a_matrix_is_recovered_from_exact_colours() {
        let truth = [[0.8, 0.15, 0.05], [0.1, 0.85, 0.05], [0.02, 0.18, 0.8]];
        let target = ColourTarget::oscope();
        let colours: Vec<[f64; 3]> = target.patches().iter().map(|p| p.rgb).collect();
        // The camera sees `inverse · colour`; the fit gives the matrix back.
        let seen: Vec<[f64; 3]> = colours
            .iter()
            .map(|c| apply3(inverse3(truth).unwrap(), *c))
            .collect();
        let fitted = fit_colour_matrix(&seen, &colours).unwrap();
        for (row, want) in fitted.iter().zip(&truth) {
            for (a, b) in row.iter().zip(want) {
                assert!((a - b).abs() < 1e-9, "{fitted:?}");
            }
        }
        assert_eq!(fit_colour_matrix(&[[1.0; 3]; 4], &[[1.0; 3]; 4]), None);
    }

    #[test]
    fn the_target_s_patches_lie_on_its_grid_within_it() {
        let target = ColourTarget::oscope();
        let patches = target.patches();
        let (w, h) = target.size_um();
        assert_eq!((patches.len(), w, h), (24, 840.0, 560.0));
        assert_eq!((patches[0].x_um, patches[0].y_um), (-350.0, -210.0));
        assert_eq!((patches[23].x_um, patches[23].y_um), (350.0, 210.0));
        for p in &patches {
            assert!(p.rgb.iter().all(|c| *c > 0.0 && *c <= 1.0), "{p:?}");
        }
    }
}
