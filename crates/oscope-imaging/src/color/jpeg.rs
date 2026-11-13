//! A baseline JPEG encoder, written from the standard (ITU-T T.81, and JFIF
//! for the colour space): the preview's codec (P17).
//!
//! - **Colour:** RGB to YCbCr (JFIF), with the chroma averaged over 2 × 2
//!   pixels (4:2:0). A minimum coded unit is 16 × 16 pixels: four
//!   luminance blocks and one of each chroma.
//! - **Transform:** each 8 × 8 block, less 128, through the separable
//!   discrete cosine transform of T.81 A.3.3, then divided by the
//!   quantisation tables of Annex K.1 and K.2, scaled for the quality.
//! - **Coding:** the Huffman tables of Annex K.3, with the coefficients in
//!   zig-zag order (Annex F.1.2).
//! - **Speed:** each row of units is a restart interval (T.81 B.2.4.4), so
//!   the rows are encoded in parallel and joined with restart markers.

use rayon::prelude::*;

use super::ColorError;

/// Annex K.1: luminance quantisation, in natural order.
const LUMINANCE: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// Annex K.2: chrominance quantisation.
const CHROMINANCE: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

/// Annex F.1.2: the natural index of each coefficient, in zig-zag order.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Annex K.3: a Huffman table, as the number of codes of each length from
/// 1 to 16 bits, and the symbols in order.
struct Spec {
    counts: [u8; 16],
    symbols: &'static [u8],
}

const DC_LUMINANCE: Spec = Spec {
    counts: [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
    symbols: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
};

const DC_CHROMINANCE: Spec = Spec {
    counts: [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0],
    symbols: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
};

const AC_LUMINANCE: Spec = Spec {
    counts: [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d],
    symbols: &[
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61,
        0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52,
        0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25,
        0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45,
        0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64,
        0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83,
        0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
        0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3,
        0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8,
        0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ],
};

const AC_CHROMINANCE: Spec = Spec {
    counts: [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77],
    symbols: &[
        0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61,
        0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33,
        0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18,
        0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44,
        0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63,
        0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a,
        0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
        0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
        0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca,
        0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7,
        0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ],
};

/// A Huffman table's code for each symbol: its bits and their length
/// (Annex C: codes of each length counted up, in symbol order).
struct Codes([(u16, u8); 256]);

impl Codes {
    fn new(spec: &Spec) -> Self {
        let mut codes = [(0, 0); 256];
        let (mut code, mut next) = (0_u16, 0);
        for (length, count) in spec.counts.iter().enumerate() {
            for _ in 0..*count {
                codes[usize::from(spec.symbols[next])] = (code, length as u8 + 1);
                code += 1;
                next += 1;
            }
            code <<= 1;
        }
        Self(codes)
    }
}

/// Bits into bytes, a 0 after every 0xff (T.81 F.1.2.3).
struct Bits {
    out: Vec<u8>,
    held: u32,
    count: u32,
}

impl Bits {
    fn put(&mut self, bits: u16, length: u8) {
        self.held = (self.held << length) | u32::from(bits) & ((1 << length) - 1);
        self.count += u32::from(length);
        while self.count >= 8 {
            self.count -= 8;
            let byte = (self.held >> self.count) as u8;
            self.out.push(byte);
            if byte == 0xff {
                self.out.push(0);
            }
        }
        // Only the bits not yet written are kept.
        self.held &= (1 << self.count) - 1;
    }

    /// Ends a segment: its last byte filled with ones.
    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            let pad = 8 - self.count as u8;
            self.put((1 << pad) - 1, pad);
        }
        self.out
    }
}

/// A coefficient's size category, and the bits that place it in it (T.81
/// F.1.2.1): a negative value is sent as itself less one.
fn category(value: i32) -> (u8, u16) {
    let size = (32 - value.unsigned_abs().leading_zeros()) as u8;
    let bits = if value < 0 { value - 1 } else { value };
    (size, bits as u16)
}

/// The tables a block is coded with.
struct Tables<'a> {
    quantisation: &'a [f32; 64],
    dc: &'a Codes,
    ac: &'a Codes,
}

/// Transforms, quantises and codes one block, from `predictor`, the
/// previous block's DC of this component; returns this block's.
fn block(
    samples: &[f32; 64],
    cosines: &[[f32; 8]; 8],
    tables: &Tables,
    predictor: i32,
    bits: &mut Bits,
) -> i32 {
    // The separable DCT: along rows, then columns.
    let mut rows = [0.0_f32; 64];
    for y in 0..8 {
        for u in 0..8 {
            rows[y * 8 + u] = (0..8).map(|x| cosines[u][x] * samples[y * 8 + x]).sum();
        }
    }
    let mut quantised = [0_i32; 64];
    for v in 0..8 {
        for u in 0..8 {
            let f: f32 = (0..8).map(|y| cosines[v][y] * rows[y * 8 + u]).sum();
            quantised[v * 8 + u] = (f / tables.quantisation[v * 8 + u]).round() as i32;
        }
    }
    let dc = quantised[0];
    let (size, extra) = category(dc - predictor);
    let (code, length) = tables.dc.0[usize::from(size)];
    bits.put(code, length);
    bits.put(extra, size);
    let mut run = 0;
    for &at in &ZIGZAG[1..] {
        let value = quantised[at];
        if value == 0 {
            run += 1;
            continue;
        }
        while run > 15 {
            let (code, length) = tables.ac.0[0xf0];
            bits.put(code, length);
            run -= 16;
        }
        let (size, extra) = category(value);
        let (code, length) = tables.ac.0[(run << 4) | usize::from(size)];
        bits.put(code, length);
        bits.put(extra, size);
        run = 0;
    }
    if run > 0 {
        let (code, length) = tables.ac.0[0x00];
        bits.put(code, length);
    }
    dc
}

/// A quantisation table for `quality` from 1 to 100: the Annex K table
/// scaled by 5000 / quality below 50, and 200 − 2 quality from 50, each
/// entry kept from 1 to 255.
fn scaled(table: &[u16; 64], quality: u8) -> [u8; 64] {
    let q = u32::from(quality.clamp(1, 100));
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    table.map(|t| ((u32::from(t) * scale + 50) / 100).clamp(1, 255) as u8)
}

/// Encodes 8-bit RGB, `width` × `height` pixels, as a baseline JPEG at
/// `quality`, from 1 to 100.
pub fn encode_jpeg(
    rgb8: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, ColorError> {
    let (w, h) = (width as usize, height as usize);
    if rgb8.len() != w * h * 3 || w == 0 || h == 0 || w > 65_535 || h > 65_535 {
        return Err(ColorError::Setting(format!(
            "{} bytes for a {width} × {height} JPEG",
            rgb8.len()
        )));
    }
    let luma_table = scaled(&LUMINANCE, quality);
    let chroma_table = scaled(&CHROMINANCE, quality);
    let to_f32 = |t: &[u8; 64]| t.map(f32::from);
    let (luma_q, chroma_q) = (to_f32(&luma_table), to_f32(&chroma_table));
    let codes = [
        Codes::new(&DC_LUMINANCE),
        Codes::new(&AC_LUMINANCE),
        Codes::new(&DC_CHROMINANCE),
        Codes::new(&AC_CHROMINANCE),
    ];
    // C(u)/2 · cos((2x + 1) u π / 16), with C(0) = 1/√2.
    let mut cosines = [[0.0_f32; 8]; 8];
    for (u, row) in cosines.iter_mut().enumerate() {
        let c = if u == 0 {
            std::f32::consts::FRAC_1_SQRT_2
        } else {
            1.0
        };
        for (x, value) in row.iter_mut().enumerate() {
            let angle = (2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0;
            *value = c / 2.0 * angle.cos();
        }
    }
    let luma = Tables {
        quantisation: &luma_q,
        dc: &codes[0],
        ac: &codes[1],
    };
    let chroma = Tables {
        quantisation: &chroma_q,
        dc: &codes[2],
        ac: &codes[3],
    };
    let (columns, rows) = (w.div_ceil(16), h.div_ceil(16));
    // A pixel's Y, Cb and Cr, the edge pixels repeated past the image.
    let ycc = |x: usize, y: usize| {
        let at = (y.min(h - 1) * w + x.min(w - 1)) * 3;
        let [r, g, b] = [rgb8[at], rgb8[at + 1], rgb8[at + 2]].map(f32::from);
        [
            0.299 * r + 0.587 * g + 0.114 * b,
            -0.168_736 * r - 0.331_264 * g + 0.5 * b + 128.0,
            0.5 * r - 0.418_688 * g - 0.081_312 * b + 128.0,
        ]
    };
    let segments: Vec<Vec<u8>> = (0..rows)
        .into_par_iter()
        .map(|row| {
            let mut bits = Bits {
                out: Vec::new(),
                held: 0,
                count: 0,
            };
            let mut predictors = [0_i32; 3];
            for column in 0..columns {
                let (x0, y0) = (column * 16, row * 16);
                let mut unit = [[0.0_f32; 3]; 256];
                for (i, px) in unit.iter_mut().enumerate() {
                    *px = ycc(x0 + i % 16, y0 + i / 16);
                }
                // Four luminance blocks, then each chroma averaged 2 × 2.
                for (bx, by) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
                    let samples: [f32; 64] =
                        std::array::from_fn(|i| unit[(by + i / 8) * 16 + bx + i % 8][0] - 128.0);
                    predictors[0] = block(&samples, &cosines, &luma, predictors[0], &mut bits);
                }
                for c in 1..3 {
                    let samples: [f32; 64] = std::array::from_fn(|i| {
                        let (x, y) = (2 * (i % 8), 2 * (i / 8));
                        let at = |dx: usize, dy: usize| unit[(y + dy) * 16 + x + dx][c];
                        (at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4.0 - 128.0
                    });
                    predictors[c] = block(&samples, &cosines, &chroma, predictors[c], &mut bits);
                }
            }
            bits.finish()
        })
        .collect();

    let mut out = Vec::with_capacity(segments.iter().map(Vec::len).sum::<usize>() + 1024);
    let marker = |out: &mut Vec<u8>, code: u8, body: &[u8]| {
        out.extend_from_slice(&[0xff, code]);
        out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(body);
    };
    out.extend_from_slice(&[0xff, 0xd8]);
    marker(&mut out, 0xe0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    for (id, table) in [(0_u8, &luma_table), (1, &chroma_table)] {
        let mut body = vec![id];
        body.extend(ZIGZAG.iter().map(|&at| table[at]));
        marker(&mut out, 0xdb, &body);
    }
    let (wb, hb) = ((width as u16).to_be_bytes(), (height as u16).to_be_bytes());
    marker(
        &mut out,
        0xc0,
        &[
            8, hb[0], hb[1], wb[0], wb[1], 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1,
        ],
    );
    for (class, spec) in [
        (0x00_u8, &DC_LUMINANCE),
        (0x10, &AC_LUMINANCE),
        (0x01, &DC_CHROMINANCE),
        (0x11, &AC_CHROMINANCE),
    ] {
        let mut body = vec![class];
        body.extend_from_slice(&spec.counts);
        body.extend_from_slice(spec.symbols);
        marker(&mut out, 0xc4, &body);
    }
    marker(&mut out, 0xdd, &(columns as u16).to_be_bytes());
    marker(&mut out, 0xda, &[3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(&[0xff, 0xd0 + ((i - 1) % 8) as u8]);
        }
        out.extend_from_slice(segment);
    }
    out.extend_from_slice(&[0xff, 0xd9]);
    Ok(out)
}
