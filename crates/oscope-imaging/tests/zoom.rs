//! Deep zoom (P40): DZI levels and tiles at every level and edge; tiles
//! that are the image's levels, cut and averaged; and a DZI folder.

use oscope_compute::{Image, Shape, cpu};
use oscope_imaging::color::{Curve, Display};
use oscope_imaging::zoom::{Dzi, InMemory, Pyramid, TILE, tile_rgb, write_dzi};

const LINEAR: Display = Display {
    black: 0.0,
    white: 1.0,
    curve: Curve::Linear,
};

#[test]
fn levels_and_tiles_follow_deep_zoom() {
    for (width, height) in [(1, 1), (256, 256), (257, 100), (1000, 700), (5472, 3648)] {
        let dzi = Dzi { width, height };
        // Levels from a pixel to the full size, each half the next.
        let mut levels = 1;
        while (1_u64 << (levels - 1)) < width.max(height) {
            levels += 1;
        }
        assert_eq!(dzi.levels(), levels, "{width} × {height}");
        let last = levels - 1;
        assert_eq!(dzi.size(last), (width, height));
        assert_eq!(dzi.size(0).0.max(dzi.size(0).1), 1);
        for level in 0..levels {
            let factor = 1 << (last - level);
            let (w, h) = (width.div_ceil(factor), height.div_ceil(factor));
            assert_eq!(dzi.size(level), (w, h));
            let (cols, rows) = dzi.tiles(level);
            assert_eq!((cols, rows), (w.div_ceil(TILE), h.div_ceil(TILE)));
            // The last tile of a row and of a column ends at the edge.
            let corner = dzi.tile(level, (cols - 1, rows - 1)).unwrap();
            assert_eq!((corner.x + corner.width, corner.y + corner.height), (w, h));
            assert!(corner.width >= 1 && corner.width <= TILE && corner.height <= TILE);
            assert_eq!(dzi.tile(level, (cols, 0)), None);
            assert_eq!(dzi.tile(level, (0, rows)), None);
        }
        assert_eq!(dzi.tile(levels, (0, 0)), None);
        assert_eq!(dzi.tile(last, (u64::MAX, 0)), None);
        let xml = dzi.descriptor();
        assert!(
            xml.contains(&format!("Width=\"{width}\" Height=\"{height}\"")),
            "{xml}"
        );
        assert!(xml.contains("TileSize=\"256\" Overlap=\"0\" Format=\"jpg\""));
    }
}

/// A grey image `w` × `h` of a smooth pattern, from 0 to 1.
fn pattern(w: u32, h: u32, channels: usize) -> Image {
    let data = (0..h)
        .flat_map(|y| {
            (0..w).flat_map(move |x| {
                (0..channels).map(move |c| {
                    let v = (x as f32 * 0.013 + y as f32 * 0.007 + c as f32 * 0.3).sin();
                    0.5 + 0.45 * v
                })
            })
        })
        .collect();
    Image::from_vec(Shape::new(w, h, channels), data).unwrap()
}

#[test]
fn tiles_are_the_levels_cut_and_averaged_past_the_coarsest() {
    let image = pattern(1000, 700, 3);
    let pyramid = InMemory::new(image.clone()).unwrap();
    // 1000 → 500 → 250: the coarsest, halved twice.
    assert_eq!((pyramid.coarsest(), pyramid.channels()), (2, 3));
    let mut levels = vec![image];
    for _ in 0..2 {
        let half = cpu::downsample(levels.last().unwrap(), 2).unwrap();
        levels.push(half);
    }
    let dzi = Dzi {
        width: 1000,
        height: 700,
    };
    let last = dzi.levels() - 1;
    for level in 0..=last {
        let halvings = last - level;
        // The expected level: the pyramid's own, or its coarsest averaged.
        let direct = halvings.min(2) as usize;
        let expected = cpu::downsample(&levels[direct], 1 << (halvings - direct as u32)).unwrap();
        let (cols, rows) = dzi.tiles(level);
        for (col, row) in [(0, 0), (cols - 1, 0), (0, rows - 1), (cols - 1, rows - 1)] {
            let area = dzi.tile(level, (col, row)).unwrap();
            let (rgb, w, h) = tile_rgb(&pyramid, level, (col, row), &LINEAR)
                .unwrap()
                .unwrap();
            assert_eq!((u64::from(w), u64::from(h)), (area.width, area.height));
            for (i, &got) in rgb.iter().enumerate() {
                let (p, c) = (i / 3, i % 3);
                let (x, y) = (area.x as u32 + (p as u32 % w), area.y as u32 + p as u32 / w);
                let want = expected.row(y)[x as usize * 3 + c];
                let want = (want * 255.0).round();
                assert!(
                    (f32::from(got) - want).abs() <= 1.0,
                    "level {level} tile ({col}, {row}) at ({x}, {y}): {got} not {want}"
                );
            }
        }
        assert!(
            tile_rgb(&pyramid, level, (cols, 0), &LINEAR)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn a_dzi_folder_holds_every_tile_of_every_level() {
    let folder = tempfile::tempdir().unwrap();
    let pyramid = InMemory::new(pattern(600, 300, 1)).unwrap();
    let count = write_dzi(&pyramid, &LINEAR, folder.path(), "slide").unwrap();
    let dzi = Dzi {
        width: 600,
        height: 300,
    };
    let want: u64 = (0..dzi.levels())
        .map(|l| dzi.tiles(l).0 * dzi.tiles(l).1)
        .sum();
    assert_eq!(count, want);
    let xml = std::fs::read_to_string(folder.path().join("slide.dzi")).unwrap();
    assert_eq!(xml, dzi.descriptor());
    // The last level's corner tile, decoded: 88 × 44 pixels of grey.
    let last = dzi.levels() - 1;
    let path = folder.path().join(format!("slide_files/{last}/2_1.jpg"));
    let bytes = std::fs::read(path).unwrap();
    let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(&bytes));
    decoder.decode_headers().unwrap();
    let (w, h) = decoder.dimensions().unwrap();
    assert_eq!((w, h), (88, 44));
}
