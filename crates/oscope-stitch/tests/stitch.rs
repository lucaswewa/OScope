//! Post-acquisition stitching's parts (P39): tiles cut from a known image,
//! their recorded places off by up to 15 pixels, registered over their
//! overlaps and solved back to where they were cut; blank tiles left where
//! the stage put them; a wrong link rejected; and blending, placing and the
//! flat field.

use oscope_compute::{Image, Shape, cpu};
use oscope_imaging::registration::{RegisterOptions, register_batch};
use oscope_stitch::{Link, Part, PixelBox, Placed, SolveOptions, Tiles, blend, flat_field, solve};

/// A slide of `w` × `h` pixels of texture: noise blurred by about 3 pixels.
fn slide(w: usize, h: usize, seed: u64) -> Vec<f32> {
    let mut state = seed;
    let mut noise: Vec<f32> = (0..w * h)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 40) as f32 / (1u64 << 24) as f32
        })
        .collect();
    for _ in 0..3 {
        noise = blur(&noise, w, h, 3);
    }
    noise
}

/// A box blur of radius `r`, along rows then columns.
fn blur(image: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let along = |src: &[f32], at: &dyn Fn(usize, usize) -> usize, len: usize, count: usize| {
        let mut out = src.to_vec();
        for n in 0..count {
            for i in 0..len {
                let (lo, hi) = (i.saturating_sub(r), (i + r).min(len - 1));
                let sum: f32 = (lo..=hi).map(|j| src[at(n, j)]).sum();
                out[at(n, i)] = sum / (hi - lo + 1) as f32;
            }
        }
        out
    };
    let rows = along(image, &|y, x| y * w + x, w, h);
    along(&rows, &|x, y| y * w + x, h, w)
}

/// A random number from -1 to 1, by `state`.
fn jitter(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

const TILE: (usize, usize) = (240, 200);

/// A grid of 4 × 3 tiles, 170 pixels apart across and 130 down, so that
/// neighbours overlap by 70 pixels, cut from the slide where they truly are,
/// and recorded up to 15 pixels off.
struct Scan {
    tiles: Tiles,
    truth: Vec<[f64; 2]>,
    images: Vec<Vec<f32>>,
}

fn scan(blank: &[usize]) -> Scan {
    let (w, h) = (900, 700);
    let image = slide(w, h, 7);
    let mut state = 42_u64;
    let (mut truth, mut nominal, mut images) = (Vec::new(), Vec::new(), Vec::new());
    for row in 0..3 {
        for col in 0..4 {
            let at = (20 + 170 * col + (row * 3) % 7, 30 + 130 * row + col % 5);
            truth.push([at.0 as f64, at.1 as f64]);
            nominal.push([
                at.0 as f64 + 15.0 * jitter(&mut state),
                at.1 as f64 + 15.0 * jitter(&mut state),
            ]);
            let k = images.len();
            let tile: Vec<f32> = (0..TILE.1)
                .flat_map(|y| {
                    let row = &image[(at.1 + y) * w + at.0..][..TILE.0];
                    row.iter()
                        .map(move |&v| if blank.contains(&k) { 0.5 } else { v })
                })
                .collect();
            images.push(tile);
        }
    }
    Scan {
        tiles: Tiles {
            width: TILE.0 as u32,
            height: TILE.1 as u32,
            nominal,
            raw: false,
        },
        truth,
        images,
    }
}

/// `tile`'s part at `corner`, `size`, binned by `bin`.
fn part(tile: &[f32], corner: (u32, u32), size: (u32, u32), bin: u32) -> Image {
    let data: Vec<f32> = (0..size.1 as usize)
        .flat_map(|y| {
            let start = (corner.1 as usize + y) * TILE.0 + corner.0 as usize;
            tile[start..start + size.0 as usize].iter().copied()
        })
        .collect();
    let image = Image::from_vec(Shape::new(size.0, size.1, 1), data).unwrap();
    if bin > 1 {
        cpu::downsample(&image, bin).unwrap()
    } else {
        image
    }
}

/// The links of `scan`'s pairs, registered within 25 pixels of the stage's
/// shift, those under 0.5 confident dropped.
fn links(scan: &Scan) -> Vec<Link> {
    let pairs = scan.tiles.pairs(32);
    let crops: Vec<_> = pairs
        .iter()
        .filter_map(|p| Some((*p, scan.tiles.crops(*p)?)))
        .collect();
    let parts: Vec<(Image, Image)> = crops
        .iter()
        .map(|(p, c)| {
            let a = part(&scan.images[p.a], c.a, c.size, c.bin);
            (a, part(&scan.images[p.b], c.b, c.size, c.bin))
        })
        .collect();
    let mut found = Vec::new();
    for ((pair, crop), (a, b)) in crops.iter().zip(&parts) {
        let options = RegisterOptions {
            expected: Some(crop.expected),
            radius: 25,
            ..RegisterOptions::default()
        };
        let r = register_batch(&[(a, b)], &options).unwrap()[0];
        if r.confidence >= 0.5 {
            found.push(Link {
                a: pair.a,
                b: pair.b,
                shift: crop.measured((r.dx, r.dy)),
                weight: r.confidence,
            });
        }
    }
    found
}

/// The root mean square of `offsets` less `truth`, each less its mean,
/// over `which` tiles.
fn rms(offsets: &[[f64; 2]], truth: &[[f64; 2]], which: &[usize]) -> f64 {
    let n = which.len() as f64;
    let mean = |v: &[[f64; 2]], axis| which.iter().map(|&k| v[k][axis]).sum::<f64>() / n;
    let (mx, my) = (
        mean(offsets, 0) - mean(truth, 0),
        mean(offsets, 1) - mean(truth, 1),
    );
    let sum: f64 = which
        .iter()
        .map(|&k| {
            (offsets[k][0] - truth[k][0] - mx).powi(2) + (offsets[k][1] - truth[k][1] - my).powi(2)
        })
        .sum();
    (sum / n).sqrt()
}

#[test]
fn tiles_go_back_where_they_were_cut() {
    let scan = scan(&[]);
    let all: Vec<usize> = (0..12).collect();
    let before = rms(&scan.tiles.nominal, &scan.truth, &all);
    assert!(before > 5.0, "the stage is {before:.2} px off");
    let links = links(&scan);
    // Across and down: 3 × 3 + 4 × 2 neighbours.
    assert_eq!(scan.tiles.pairs(32).len(), 17);
    assert_eq!(links.len(), 17, "{links:?}");
    let solved = solve(&scan.tiles.nominal, &links, &SolveOptions::default());
    let after = rms(&solved.offsets, &scan.truth, &all);
    assert!(after < 0.3, "{after:.3} px off the truth, from {before:.2}");
    assert!(
        solved.rejected.is_empty() && solved.rms_px < 0.3,
        "{solved:?}"
    );
}

#[test]
fn blank_tiles_stay_where_the_stage_put_them() {
    let blank = [5, 6];
    let scan = scan(&blank);
    let links = links(&scan);
    assert!(
        links
            .iter()
            .all(|l| !blank.contains(&l.a) && !blank.contains(&l.b))
    );
    let solved = solve(&scan.tiles.nominal, &links, &SolveOptions::default());
    let textured: Vec<usize> = (0..12).filter(|k| !blank.contains(k)).collect();
    let after = rms(&solved.offsets, &scan.truth, &textured);
    assert!(after < 0.3, "{after:.3} px off the truth");
    for k in blank {
        let (o, n) = (solved.offsets[k], scan.tiles.nominal[k]);
        assert!(
            (o[0] - n[0]).hypot(o[1] - n[1]) < 1.0,
            "tile {k}: {o:?}, not {n:?}"
        );
    }
}

#[test]
fn a_wrong_link_is_rejected() {
    let scan = scan(&[]);
    let mut links = links(&scan);
    let wrong = links.len();
    let mut bad = links[3];
    bad.shift[0] += 25.0;
    links.push(bad);
    let solved = solve(&scan.tiles.nominal, &links, &SolveOptions::default());
    assert_eq!(solved.rejected, [wrong]);
    let all: Vec<usize> = (0..12).collect();
    assert!(rms(&solved.offsets, &scan.truth, &all) < 0.3);
}

#[test]
fn a_link_a_little_wrong_is_weighed_down_until_it_is_rejected() {
    // A link 4 pixels wrong between two inner tiles: plain least squares
    // spreads it over its neighbours' links, and each then misses by under
    // 3 pixels; Huber's weights leave it its own miss, and it is rejected.
    let scan = scan(&[]);
    let mut links = links(&scan);
    let wrong = links.iter().position(|l| (l.a, l.b) == (5, 6)).unwrap();
    links[wrong].shift[0] += 4.0;
    let plain = SolveOptions {
        huber_px: f64::INFINITY,
        ..SolveOptions::default()
    };
    let spread = solve(&scan.tiles.nominal, &links, &plain);
    assert!(spread.rejected.is_empty(), "{spread:?}");
    let solved = solve(&scan.tiles.nominal, &links, &SolveOptions::default());
    assert_eq!(solved.rejected, [wrong]);
    let all: Vec<usize> = (0..12).collect();
    assert!(rms(&solved.offsets, &scan.truth, &all) < 0.3);
}

#[test]
fn tiles_are_placed_on_whole_pixels_even_ones_raw_and_blended_without_seams() {
    let offsets = [[10.4, 3.6], [131.0, 4.5], [10.5, 100.49]];
    let placed = Placed::new(&offsets, (128, 100), false);
    assert_eq!(placed.at, [(0, 0), (121, 1), (1, 96)]);
    assert_eq!(placed.size, (249, 196));
    let raw = Placed::new(&offsets, (128, 100), true);
    assert!(
        raw.at.iter().all(|(x, y)| x % 2 == 0 && y % 2 == 0),
        "{:?}",
        raw.at
    );
    // Two tiles of one value meet seamlessly; one alone keeps its own.
    let (a, b) = (vec![0.25_f32; 128 * 100], vec![0.25_f32; 128 * 100]);
    let chunk = PixelBox {
        x0: 100,
        y0: 1,
        x1: 160,
        y1: 40,
    };
    let owned: Vec<(PixelBox, Vec<f32>, (u32, u32))> = placed
        .over(chunk)
        .into_iter()
        .filter(|(k, _)| *k < 2)
        .map(|(k, within)| {
            let tile = if k == 0 { &a } else { &b };
            let samples = (within.y0..within.y1)
                .flat_map(|y| {
                    tile[(y * 128 + within.x0) as usize..(y * 128 + within.x1) as usize].to_vec()
                })
                .collect();
            let (x, y) = placed.at[k];
            let to = (
                (x as i64 + within.x0 - chunk.x0) as u32,
                (y as i64 + within.y0 - chunk.y0) as u32,
            );
            (within, samples, to)
        })
        .collect();
    let parts: Vec<Part> = owned
        .iter()
        .map(|(within, samples, to)| Part {
            within: *within,
            samples,
            to: *to,
        })
        .collect();
    assert_eq!(parts.len(), 2);
    let out = blend((60, 39), 1, (128, 100), &parts, None);
    // Where only a lies, and where both do: every pixel 0.25.
    assert!(
        out.iter().all(|v| (v - 0.25).abs() < 1e-6),
        "{:?}",
        &out[..8]
    );
    // Where they differ, each pixel leans to the tile it is deeper in: on
    // row 40, a's column 125 is 2.5 from its edge, b's column 4 is 4.5.
    let (dark, light) = ([0.2_f32], [0.6_f32]);
    let pixel = |x0, x1| PixelBox {
        x0,
        y0: 40,
        x1,
        y1: 41,
    };
    let parts = [
        Part {
            within: pixel(125, 126),
            samples: &dark,
            to: (0, 0),
        },
        Part {
            within: pixel(4, 5),
            samples: &light,
            to: (0, 0),
        },
    ];
    let out = blend((1, 1), 1, (128, 100), &parts, None);
    let want = (2.5 * 0.2 + 4.5 * 0.6) / 7.0;
    assert!((out[0] - want).abs() < 1e-6, "{} not {want}", out[0]);
}

#[test]
fn pairs_need_their_least_overlap_and_long_ones_are_binned() {
    // 10 pixels side by side: under 32, no pair.
    let narrow = Tiles {
        width: 240,
        height: 200,
        nominal: vec![[0.0, 0.0], [230.0, 0.0]],
        raw: false,
    };
    assert!(narrow.pairs(32).is_empty());
    assert_eq!(narrow.pairs(8).len(), 1);
    // 5000 × 3000 tiles a tenth overlapping, the second 3.4 pixels off: a
    // part 3000 tall, binned by 2 to under 2048, cut at even pixels.
    let long = Tiles {
        width: 5000,
        height: 3000,
        nominal: vec![[0.0, 0.0], [4503.4, 3.0]],
        raw: false,
    };
    let crops = long.crops(long.pairs(32)[0]).unwrap();
    assert_eq!(crops.bin, 2);
    let (a, b, size) = (crops.a, crops.b, crops.size);
    assert!(
        [a.0, a.1, b.0, b.1, size.0, size.1]
            .iter()
            .all(|v| v % 2 == 0),
        "{crops:?}"
    );
    // The stage's shift, back from the binned parts' expected one.
    let back = crops.measured(crops.expected);
    assert!(
        (back[0] - 4503.4).abs() < 1e-9 && (back[1] - 3.0).abs() < 1e-9,
        "{back:?}"
    );
}

#[test]
fn the_flat_field_is_the_shading_the_tiles_share() {
    let (w, h) = (64_usize, 48_usize);
    let shading = |x: usize, y: usize| {
        let (dx, dy) = (x as f32 / w as f32 - 0.5, y as f32 / h as f32 - 0.5);
        1.0 - 0.6 * (dx * dx + dy * dy)
    };
    let mut state = 9_u64;
    let tiles: Vec<Vec<f32>> = (0..64)
        .map(|_| {
            (0..w * h)
                .map(|i| (0.5 + 0.2 * jitter(&mut state) as f32) * shading(i % w, i / w))
                .collect()
        })
        .collect();
    let views: Vec<&[f32]> = tiles.iter().map(Vec::as_slice).collect();
    let flat = flat_field(&views, (w as u32, h as u32), 1, 8);
    let mean = (0..w * h).map(|i| shading(i % w, i / w)).sum::<f32>() / (w * h) as f32;
    // Within 5 % of the shading, but at its corners, which the blur and
    // its repeated edges flatten.
    for y in 8..h - 8 {
        for x in 8..w - 8 {
            let (got, want) = (flat.gains[y * w + x], shading(x, y) / mean);
            assert!((got - want).abs() < 0.05, "({x}, {y}): {got} not {want}");
        }
    }
    assert_eq!(flat.scale, 8);
}
