//! Deep zoom from the store (P40): reads through the chunk cache that are
//! the image's own, a raw mosaic in colour, the tile cache's budget, and an
//! export that is what is served.

use std::sync::Arc;

use oscope_compute::{Image, Shape};
use oscope_core::{BayerColour, BayerPattern, PixelFormat};
use oscope_imaging::color::{Curve, Demosaic, Display, demosaic};
use oscope_imaging::zoom::{Dzi, Pyramid, tile};
use oscope_store::zoom::{
    ChunkCache, StoredImage, StoredPyramid, TileCache, TileKey, build_overview, export_dzi,
};
use oscope_store::{
    ImageInfo, ImageReader, ImageWriter, NewProject, RecordKind, SampleType, Samples, Store,
};

/// A smooth pattern, from 0 to 1, of `channels` at `(x, y)` of plane `p`.
fn value(x: u64, y: u64, c: u32, p: u32) -> f32 {
    let v = (x as f32 * 0.011 + y as f32 * 0.017 + c as f32 * 0.9 + p as f32).sin();
    0.5 + 0.45 * v
}

/// An image of `info` at `path`, its pattern written, its levels made.
fn write(path: &std::path::Path, info: &ImageInfo) {
    let writer = ImageWriter::create(path, info, "test").unwrap();
    let c = info.channels;
    for p in 0..info.planes {
        let values: Vec<f32> = (0..info.height)
            .flat_map(|y| (0..info.width).flat_map(move |x| (0..c).map(move |k| value(x, y, k, p))))
            .collect();
        let samples = Samples::from_unit(info.sample, &values);
        writer
            .write(p, (0, 0), (info.width, info.height), &samples)
            .unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn reads_through_the_chunk_cache_are_the_image_s_own() {
    let folder = tempfile::tempdir().unwrap();
    for channels in [1, 3] {
        let path = folder.path().join(format!("{channels}.ome.zarr"));
        let mut info = ImageInfo::new(1300, 900, channels, SampleType::U16);
        info.planes = 2;
        write(&path, &info);
        let reader = ImageReader::open(&path).unwrap();
        let cache = Arc::new(ChunkCache::new(64 << 20));
        let image = Arc::new(StoredImage::open(&path, 1).unwrap());
        let pyramid = StoredPyramid::new(image, Arc::clone(&cache), None).unwrap();
        assert_eq!(
            (pyramid.size(), pyramid.channels()),
            ((1300, 900), channels)
        );
        assert_eq!(pyramid.coarsest(), reader.info().levels - 1);
        // Across chunks' edges, at the image's edges, at every level.
        for level in 0..reader.info().levels {
            let (w, h) = reader.info().level_size(level);
            for (at, size) in [
                ((0, 0), (w.min(300), h.min(200))),
                (
                    (w.saturating_sub(70), 500.min(h - 1)),
                    (w.min(70), (h - 500.min(h - 1)).min(30)),
                ),
                ((500.min(w - 1), 0), ((w - 500.min(w - 1)).min(40), h)),
            ] {
                let got = pyramid.read(level, at, size).unwrap();
                let want = reader.read(level, 1, at, size).unwrap().to_unit();
                assert_eq!(got, want, "level {level} at {at:?}, {size:?}");
            }
        }
        // Read again: from the cache.
        let (hits, misses) = cache.counts();
        pyramid.read(0, (480, 480), (64, 64)).unwrap();
        assert_eq!(cache.counts(), (hits + 4, misses));
    }
}

/// The mosaic of `pattern` that samples `rgb` (w × h × 3, from 0 to 1).
fn mosaic(rgb: &[f32], w: u64, h: u64, pattern: BayerPattern) -> Vec<f32> {
    (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                let c = match pattern.colour_at(x as u32, y as u32) {
                    BayerColour::Red => 0,
                    BayerColour::Green => 1,
                    BayerColour::Blue => 2,
                };
                rgb[((y * w + x) * 3) as usize + c]
            })
        })
        .collect()
}

#[test]
fn a_raw_mosaic_shows_in_colour_from_its_overview() {
    let folder = tempfile::tempdir().unwrap();
    let (w, h, pattern) = (1100_u64, 1300_u64, BayerPattern::Rggb);
    let rgb: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).flat_map(move |x| (0..3).map(move |c| value(x, y, c, 0))))
        .collect();
    let raw = mosaic(&rgb, w, h, pattern);
    let path = folder.path().join("raw.ome.zarr");
    let mut info = ImageInfo::new(w, h, 1, SampleType::U16);
    info.channel_names = vec!["raw".to_owned()];
    let writer = ImageWriter::create(&path, &info, "raw").unwrap();
    writer
        .write(
            0,
            (0, 0),
            (w, h),
            &Samples::from_unit(SampleType::U16, &raw),
        )
        .unwrap();
    writer.finish().unwrap();
    let image = Arc::new(StoredImage::open(&path, 0).unwrap());
    let unit = Samples::from_unit(SampleType::U16, &raw).to_unit();
    let whole = Image::from_vec(Shape::new(w as u32, h as u32, 1), unit).unwrap();

    // Before its overview: demosaiced at full size, grey beyond.
    let cache = Arc::new(ChunkCache::new(64 << 20));
    let grey = StoredPyramid::new(
        Arc::clone(&image),
        Arc::clone(&cache),
        Some((pattern, None)),
    )
    .unwrap();
    assert_eq!(grey.channels(), 3);
    let half = grey.read(1, (10, 20), (30, 10)).unwrap();
    assert!(
        half.as_chunks::<3>()
            .0
            .iter()
            .all(|[r, g, b]| r == g && g == b)
    );
    // At full size, as the whole mosaic demosaiced, then cut: at a corner,
    // and across chunks.
    let colour = demosaic(&whole, pattern, Demosaic::Bilinear).unwrap();
    for (at, size) in [
        ((0, 0), (40, 30)),
        ((500, 507), (25, 9)),
        ((1070, 1290), (30, 10)),
    ] {
        let got = grey.read(0, at, size).unwrap();
        for (i, v) in got.iter().enumerate() {
            let (p, c) = (i as u64 / 3, i % 3);
            let (x, y) = (at.0 + p % size.0, at.1 + p / size.0);
            let want = colour.row(y as u32)[x as usize * 3 + c];
            assert!((v - want).abs() < 1e-6, "({x}, {y}) {c}: {v} not {want}");
        }
    }

    // The overview: each cell one pixel, its red, its greens' mean, its blue.
    let out = folder.path().join("overview.ome.zarr");
    build_overview(&image, pattern, &out).unwrap();
    let overview = ImageReader::open(&out).unwrap();
    assert_eq!(
        (
            overview.info().width,
            overview.info().height,
            overview.info().channels
        ),
        (w / 2, h / 2, 3)
    );
    let cells = overview
        .read(0, 0, (0, 0), (w / 2, h / 2))
        .unwrap()
        .to_unit();
    let superpixel = demosaic(&whole, pattern, Demosaic::Superpixel).unwrap();
    let want = Samples::from_unit(SampleType::U16, superpixel.data()).to_unit();
    assert_eq!(cells, want);
    // Halved, a tile comes from it.
    let opened = Arc::new(StoredImage::open(&out, 0).unwrap());
    let shown =
        StoredPyramid::new(Arc::clone(&image), cache, Some((pattern, Some(opened)))).unwrap();
    assert_eq!(shown.coarsest(), overview.info().levels);
    let got = shown.read(1, (10, 20), (30, 10)).unwrap();
    assert_eq!(
        got,
        overview.read(0, 0, (10, 20), (30, 10)).unwrap().to_unit()
    );
    let (w2, h2) = overview.info().level_size(1);
    let got = shown.read(2, (0, 0), (w2, h2)).unwrap();
    assert_eq!(
        got,
        overview.read(1, 0, (0, 0), (w2, h2)).unwrap().to_unit()
    );
}

fn key(n: u64) -> TileKey {
    TileKey {
        image: "an-image".to_owned(),
        look: "a-look".to_owned(),
        level: 9,
        tile: (n, 0),
    }
}

#[test]
fn the_tile_cache_keeps_the_newest_within_its_budget() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("zoom");
    let cache = TileCache::open(&root, 300).unwrap();
    assert_eq!(cache.get(&key(0)), None);
    for n in 0..3 {
        cache.put(&key(n), &[n as u8; 100]).unwrap();
    }
    // Used: 1 comes back, so 0 is now the oldest, and goes first.
    assert_eq!(cache.get(&key(1)), Some(vec![1; 100]));
    cache.put(&key(1), &[1; 100]).unwrap();
    cache.get(&key(0)).unwrap();
    cache.put(&key(3), &[3; 100]).unwrap();
    assert_eq!(cache.get(&key(2)), None, "the least recently used goes");
    assert!(!root.join(key(2).file()).exists());
    for n in [0, 1, 3] {
        assert_eq!(cache.get(&key(n)), Some(vec![n as u8; 100]), "tile {n}");
    }
    let counts = cache.counts();
    assert_eq!((counts.tiles, counts.bytes), (3, 300));
    assert_eq!((counts.hits, counts.misses), (5, 2));
    // Its layout, for the desktop app.
    assert!(root.join("an-image/a-look/9/3_0.jpg").is_file());
    // A tile deleted behind its back is forgotten.
    std::fs::remove_file(root.join(key(0).file())).unwrap();
    assert_eq!(cache.get(&key(0)), None);
    assert_eq!(cache.counts().tiles, 2);
    // Opened again: what is on the disk, within a smaller budget.
    let again = TileCache::open(&root, 100).unwrap();
    assert_eq!((again.counts().tiles, again.counts().bytes), (1, 100));
}

#[test]
fn an_export_is_what_is_served() {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let project = store
        .create(NewProject {
            name: "zoom".to_owned(),
            ..NewProject::default()
        })
        .unwrap()
        .id;
    // A raw acquisition of a camera's Bayer mosaic, 700 × 600.
    let (w, h) = (700_u64, 600_u64);
    let mut info = ImageInfo::new(w, h, 1, SampleType::U16);
    info.channel_names = vec!["raw".to_owned()];
    let mut draft = store.acquisition(project, "a frame", &info).unwrap();
    let values: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| value(x, y, 0, 0)))
        .collect();
    let writer = draft.image().unwrap();
    writer
        .write(
            0,
            (0, 0),
            (w, h),
            &Samples::from_unit(SampleType::U16, &values),
        )
        .unwrap();
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Grbg,
        significant_bits: 12,
    };
    draft.record().camera = serde_json::json!({ "format": format });
    let id = draft.commit().unwrap().id.to_string();

    let display = Display {
        black: 0.1,
        white: 0.9,
        curve: Curve::Gamma { gamma: 2.2 },
    };
    let path = export_dzi(&store, project, RecordKind::Acquisition, &id, Some(display)).unwrap();
    let exported = store.folder(project).join(&path);
    assert_eq!(path, format!("acquisitions/{id}/exports/{id}_dzi"));
    // Nothing left beside it: the overview it needed is gone.
    let exports = exported.parent().unwrap();
    assert_eq!(std::fs::read_dir(exports).unwrap().count(), 1);

    // Served from the same image, with its overview: the same bytes.
    let (image_path, pattern) =
        oscope_store::zoom::record_image(&store, project, RecordKind::Acquisition, &id).unwrap();
    assert_eq!(pattern, Some(BayerPattern::Grbg));
    let image = Arc::new(StoredImage::open(&image_path, 0).unwrap());
    let overview = folder.path().join("overview.ome.zarr");
    build_overview(&image, BayerPattern::Grbg, &overview).unwrap();
    let opened = Arc::new(StoredImage::open(&overview, 0).unwrap());
    let cache = Arc::new(ChunkCache::new(64 << 20));
    let served =
        StoredPyramid::new(image, cache, Some((BayerPattern::Grbg, Some(opened)))).unwrap();
    let dzi = Dzi {
        width: w,
        height: h,
    };
    let xml = std::fs::read_to_string(exported.join("image.dzi")).unwrap();
    assert_eq!(xml, dzi.descriptor());
    for level in 0..dzi.levels() {
        let (cols, rows) = dzi.tiles(level);
        for (col, row) in (0..rows).flat_map(|r| (0..cols).map(move |c| (c, r))) {
            let file = exported.join(format!("image_files/{level}/{col}_{row}.jpg"));
            let want = tile(&served, level, (col, row), &display).unwrap().unwrap();
            assert_eq!(
                std::fs::read(&file).unwrap(),
                want,
                "level {level} ({col}, {row})"
            );
        }
    }
}
