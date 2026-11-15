//! The live mosaic's canvas (ADR-0026): frames land where their map puts
//! them, the newest wins or its edges are feathered, each level halves the
//! one below, memory stays within its budget, and viewers learn what
//! changed.

use oscope_stitch::canvas::{LEAST_TILES, TILE_BYTES};
use oscope_stitch::{Affine, Blend, Canvas, CanvasError, CanvasGeometry, Rgb, TILE, TileKey};

/// A canvas of `width` × `height` pixels of 1 µm from the stage's origin.
fn canvas(width: u32, height: u32, tiles: usize) -> (Canvas, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let geometry = CanvasGeometry {
        x_um: 0.0,
        y_um: 0.0,
        pixel_um: 1.0,
        width,
        height,
    };
    let canvas = Canvas::new(geometry, tiles * TILE_BYTES, folder.path().join("tiles")).unwrap();
    (canvas, folder)
}

/// An image whose red is 4 × x and green 5 × y at each pixel's centre, x
/// and y from 0: bilinear sampling gives back the same plane between them.
fn gradient(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend([(4 * x) as u8, (5 * y) as u8, 100]);
        }
    }
    pixels
}

fn flat(width: u32, height: u32, colour: [u8; 3]) -> Vec<u8> {
    colour.repeat((width * height) as usize)
}

/// Turns by `degrees`, scales by `scale`, then moves by `(x, y)`.
fn map(degrees: f64, scale: f64, (x, y): (f64, f64)) -> Affine {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Affine([
        [scale * cos, -scale * sin, x],
        [scale * sin, scale * cos, y],
    ])
}

/// The level-0 pixel at `(x, y)`.
fn pixel(canvas: &mut Canvas, x: u32, y: u32) -> [u8; 4] {
    let key = TileKey {
        level: 0,
        col: x / TILE,
        row: y / TILE,
    };
    let Some(tile) = canvas.tile(key).unwrap() else {
        return [0; 4];
    };
    let at = (((y % TILE) * TILE + x % TILE) * 4) as usize;
    tile[at..at + 4].try_into().unwrap()
}

#[test]
fn a_frame_lands_where_its_map_puts_it() {
    let (mut canvas, _folder) = canvas(1024, 1024, 64);
    let image = gradient(64, 48);
    let to_canvas = map(30.0, 1.5, (300.25, 400.75));
    let rgb = Rgb {
        pixels: &image,
        width: 64,
        height: 48,
    };
    let changed = canvas.draw(rgb, &to_canvas, Blend::Newest).unwrap();
    assert!(!changed.is_empty());
    let from = to_canvas.inverse().unwrap();
    let (mut inside, mut outside) = (0, 0);
    for y in (300..600).step_by(7) {
        for x in (200..450).step_by(7) {
            let got = pixel(&mut canvas, x, y);
            let (u, v) = from.apply(f64::from(x) + 0.5, f64::from(y) + 0.5);
            if u > 1.0 && v > 1.0 && u < 63.0 && v < 47.0 {
                // The plane at the image's point, its pixels' centres at
                // (i + 0.5, j + 0.5).
                let want = [4.0 * (u - 0.5), 5.0 * (v - 0.5), 100.0];
                for (g, w) in got.iter().zip(want) {
                    assert!(
                        (f64::from(*g) - w).abs() <= 1.0,
                        "({x}, {y}): {got:?}, {want:?}"
                    );
                }
                assert_eq!(got[3], 255);
                inside += 1;
            } else if u < -1.0 || v < -1.0 || u > 65.0 || v > 49.0 {
                assert_eq!(got, [0; 4], "({x}, {y}) is off the image");
                outside += 1;
            }
        }
    }
    assert!(
        inside > 100 && outside > 100,
        "{inside} inside, {outside} outside"
    );
}

#[test]
fn the_newest_wins_or_its_edges_are_feathered() {
    let (red, blue) = (flat(100, 100, [200, 0, 0]), flat(100, 100, [0, 0, 200]));
    let image = |pixels| Rgb {
        pixels,
        width: 100,
        height: 100,
    };
    for blend in [Blend::Newest, Blend::Feather { width_px: 20.0 }] {
        let (mut canvas, _folder) = canvas(512, 512, 64);
        canvas
            .draw(image(&red), &map(0.0, 1.0, (100.0, 100.0)), blend)
            .unwrap();
        canvas
            .draw(image(&blue), &map(0.0, 1.0, (150.0, 100.0)), blend)
            .unwrap();
        // Blue over red, 5 px in from blue's left edge, and 60 px in.
        let (edge, deep) = (pixel(&mut canvas, 155, 150), pixel(&mut canvas, 190, 150));
        // Blue's right edge, over nothing.
        let alone = pixel(&mut canvas, 248, 150);
        assert_eq!(deep, [0, 0, 200, 255], "{blend:?}");
        assert_eq!(alone, [0, 0, 200, 255], "{blend:?}");
        match blend {
            Blend::Newest => assert_eq!(edge, [0, 0, 200, 255]),
            // 5.5 px in, of 20: a quarter blue.
            _ => assert_eq!(edge, [145, 0, 55, 255]),
        }
    }
}

#[test]
fn each_level_halves_the_one_below() {
    let (mut canvas, _folder) = canvas(1000, 600, 64);
    let geometry = canvas.geometry();
    assert_eq!(geometry.levels(), 3);
    assert_eq!(
        (geometry.size(1), geometry.size(2)),
        ((500, 300), (250, 150))
    );
    assert_eq!((geometry.tiles(0), geometry.tiles(2)), ((4, 3), (1, 1)));
    let image = gradient(60, 50);
    let rgb = Rgb {
        pixels: &image,
        width: 60,
        height: 50,
    };
    // Level 0's pixels 260–319 by 251–300: the tiles in column 1, rows 0
    // and 1, then one above them at each level.
    let changed = canvas
        .draw(rgb, &map(0.0, 1.0, (260.0, 251.0)), Blend::Newest)
        .unwrap();
    let at = |level, col, row| TileKey { level, col, row };
    assert_eq!(
        changed,
        [at(0, 1, 0), at(0, 1, 1), at(1, 0, 0), at(2, 0, 0)]
    );
    let below = canvas.tile(at(0, 1, 0)).unwrap().unwrap();
    let above = canvas.tile(at(1, 0, 0)).unwrap().unwrap();
    // Level 1's pixel (130, 126) is the mean of level 0's 260–261 by
    // 252–253: in tile (1, 0), 4–5 by 252–253.
    let mean = |c: usize| {
        let sum: u32 = [(4, 252), (5, 252), (4, 253), (5, 253)]
            .iter()
            .map(|(x, y)| u32::from(below[(y * 256 + x) * 4 + c]))
            .sum();
        ((sum + 2) / 4) as u8
    };
    let got = &above[(126 * 256 + 130) * 4..][..4];
    assert_eq!(got, [mean(0), mean(1), mean(2), 255]);
    // Red 0 and 4, green 5 and 10 (rows 252–253 of an image from 251).
    assert_eq!(got, [2, 8, 100, 255]);
    // Level 1's row 125 has level 0's 250, bare, and 251, covered.
    let edge = &above[(125 * 256 + 130) * 4..][..4];
    assert_eq!(edge[3], 128, "{edge:?}");
}

#[test]
fn memory_stays_within_its_budget_and_its_tiles_come_back() {
    let (mut small, _a) = canvas(2048, 2048, LEAST_TILES + 1);
    let (mut large, _b) = canvas(2048, 2048, 1000);
    let image = gradient(60, 50);
    let rgb = Rgb {
        pixels: &image,
        width: 60,
        height: 50,
    };
    for k in 0..12 {
        let to_canvas = map(
            10.0 * f64::from(k),
            6.0,
            (150.0 * f64::from(k), 1800.0 - 140.0 * f64::from(k)),
        );
        let a = small
            .draw(rgb, &to_canvas, Blend::Feather { width_px: 30.0 })
            .unwrap();
        let b = large
            .draw(rgb, &to_canvas, Blend::Feather { width_px: 30.0 })
            .unwrap();
        assert_eq!(a, b);
        assert!(small.resident_bytes() <= (LEAST_TILES + 1) * TILE_BYTES);
    }
    assert!(large.tile_count() > 40, "{} tiles", large.tile_count());
    assert_eq!(small.tile_count(), large.tile_count());
    for level in 0..small.geometry().levels() {
        let (cols, rows) = small.geometry().tiles(level);
        for row in 0..rows {
            for col in 0..cols {
                let key = TileKey { level, col, row };
                assert!(
                    small.tile(key).unwrap() == large.tile(key).unwrap(),
                    "{key:?}"
                );
            }
        }
    }
    assert!(small.resident_bytes() <= (LEAST_TILES + 1) * TILE_BYTES);
    let folder = tempfile::tempdir().unwrap();
    let refused = Canvas::new(
        small.geometry(),
        (LEAST_TILES - 1) * TILE_BYTES,
        folder.path(),
    );
    assert!(matches!(refused, Err(CanvasError::Setup(_))));
}

#[test]
fn viewers_learn_what_changed() {
    let (mut canvas, _folder) = canvas(2048, 2048, 64);
    let image = gradient(60, 50);
    let rgb = Rgb {
        pixels: &image,
        width: 60,
        height: 50,
    };
    let before = canvas.generation();
    let first = canvas
        .draw(rgb, &map(0.0, 1.0, (10.0, 10.0)), Blend::Newest)
        .unwrap();
    let second = canvas
        .draw(rgb, &map(0.0, 1.0, (1500.0, 10.0)), Blend::Newest)
        .unwrap();
    let changes = canvas.changes_since(before);
    assert!(!changes.all);
    assert_eq!(changes.generation, before + 2);
    let mut both: Vec<TileKey> = first.iter().chain(&second).copied().collect();
    both.sort();
    both.dedup();
    assert_eq!(changes.tiles, both);
    assert_eq!(canvas.changes_since(before + 1).tiles.len(), second.len());
    assert!(canvas.changes_since(changes.generation).tiles.is_empty());
    // Nothing on the canvas: no generation.
    let off = canvas
        .draw(rgb, &map(0.0, 1.0, (5000.0, 10.0)), Blend::Newest)
        .unwrap();
    assert!(off.is_empty() && canvas.generation() == changes.generation);
    // A generation it hasn't had, or a clearing since: all.
    assert!(canvas.changes_since(changes.generation + 1).all);
    canvas.clear().unwrap();
    let cleared = canvas.changes_since(changes.generation);
    assert!(cleared.all && canvas.tile_count() == 0);
    let since = canvas.generation();
    let again = canvas
        .draw(rgb, &map(0.0, 1.0, (10.0, 10.0)), Blend::Newest)
        .unwrap();
    assert_eq!(canvas.changes_since(since).tiles, again);
}

#[test]
fn a_flat_map_or_a_wrong_image_is_refused() {
    let (mut canvas, _folder) = canvas(512, 512, 64);
    let image = gradient(10, 10);
    let rgb = Rgb {
        pixels: &image,
        width: 10,
        height: 10,
    };
    let flat = Affine([[1.0, 2.0, 0.0], [2.0, 4.0, 0.0]]);
    assert!(matches!(
        canvas.draw(rgb, &flat, Blend::Newest),
        Err(CanvasError::Singular)
    ));
    let short = Rgb {
        pixels: &image[3..],
        ..rgb
    };
    let drawn = canvas.draw(short, &map(0.0, 1.0, (0.0, 0.0)), Blend::Newest);
    assert!(matches!(drawn, Err(CanvasError::Image(_))));
    let thin = Blend::Feather { width_px: 0.0 };
    let drawn = canvas.draw(rgb, &map(0.0, 1.0, (0.0, 0.0)), thin);
    assert!(matches!(drawn, Err(CanvasError::Setup(_))));
    let wide = CanvasGeometry::covering((0.0, 0.0), (1e9, 10.0), 1.0);
    assert!(matches!(wide, Err(CanvasError::Setup(_))));
}
