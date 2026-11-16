//! Volumes in the store (P41): a z-stack's levels halving z as its voxels
//! near a cube, each voxel the mean of those below, the OME metadata's
//! scale per level; and a flat image's planes left alone.

use oscope_store::{ImageInfo, ImageReader, ImageWriter, PixelSize, SampleType, Samples};

/// Sample `(x, y, z)` of the test volumes, from 0 to 1.
fn value(x: u64, y: u64, z: u64) -> f32 {
    ((x * 7 + y * 13 + z * 29) % 1000) as f32 / 1000.0
}

/// A volume `w` × `h` × `planes` of `value`, written and its levels made.
fn write(path: &std::path::Path, info: &ImageInfo) -> ImageInfo {
    let writer = ImageWriter::create(path, info, "a volume").unwrap();
    for z in 0..u64::from(info.planes) {
        let values: Vec<f32> = (0..info.height)
            .flat_map(|y| (0..info.width).map(move |x| value(x, y, z)))
            .collect();
        let samples = Samples::from_unit(info.sample, &values);
        writer
            .write(z as u32, (0, 0), (info.width, info.height), &samples)
            .unwrap();
    }
    writer.finish().unwrap()
}

/// Level `level` of `reader`, whole, its planes in turn.
fn level(reader: &ImageReader, level: u32) -> Vec<Vec<f32>> {
    let info = reader.info();
    let (w, h) = info.level_size(level);
    (0..info.level_planes(level))
        .map(|p| reader.read(level, p, (0, 0), (w, h)).unwrap().to_unit())
        .collect()
}

#[test]
fn a_volume_s_levels_halve_z_as_its_voxels_near_a_cube() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("stack.ome.zarr");
    let mut info = ImageInfo::new(1100, 900, 1, SampleType::F32);
    (info.planes, info.volume) = (13, true);
    info.pixel_size = Some(PixelSize {
        x: 0.3,
        y: 0.3,
        z: 1.0,
    });
    let info = write(&path, &info);
    // Pixels of 0.6 µm at level 1 are under the 1 µm step; of 1.2 at level
    // 2, over it: z halves there, and again at level 3, 2.4 over 2.
    assert_eq!(info.levels, 3);
    assert_eq!(
        (0..3).map(|l| info.z_halvings(l)).collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(
        (0..3).map(|l| info.level_planes(l)).collect::<Vec<_>>(),
        [13, 13, 7]
    );
    let reader = ImageReader::open(&path).unwrap();
    assert!(reader.info().volume);
    assert!(reader.read(2, 6, (0, 0), (1, 1)).is_ok());
    assert!(reader.read(2, 7, (0, 0), (1, 1)).is_err());
    let (w2, h2) = info.level_size(2);
    assert!(reader.read(2, 0, (w2 - 1, 0), (2, 1)).is_err());
    assert!(reader.read(2, 0, (0, h2 - 1), (1, 2)).is_err());
    // Level 2's voxels: the means of level 1's 2 × 2 × 2, or of the one
    // plane left at the end.
    let (one, two) = (level(&reader, 1), level(&reader, 2));
    let (w1, h1) = info.level_size(1);
    let (w2, _) = info.level_size(2);
    for (z, y, x) in [(0, 0, 0), (3, 100, 200), (6, 112, 137), (6, 0, 274)] {
        let below: Vec<usize> = (2 * z..(2 * z + 2).min(13)).collect();
        let mut sum = 0.0;
        let mut n = 0.0;
        for &p in &below {
            for yy in 2 * y..(2 * y + 2).min(h1 as usize) {
                for xx in 2 * x..(2 * x + 2).min(w1 as usize) {
                    sum += one[p][yy * w1 as usize + xx];
                    n += 1.0;
                }
            }
        }
        let got = two[z][y * w2 as usize + x];
        assert!(
            (got - sum / n).abs() < 1e-6,
            "({x}, {y}, {z}): {got} not {}",
            sum / n
        );
    }
    // The metadata: z's scale and centre at each level.
    let attributes: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("zarr.json")).unwrap()).unwrap();
    let multiscale = &attributes["attributes"]["ome"]["multiscales"][0];
    let scale =
        |l: usize| multiscale["datasets"][l]["coordinateTransformations"][0]["scale"].clone();
    let shift =
        |l: usize| multiscale["datasets"][l]["coordinateTransformations"][1]["translation"].clone();
    assert_eq!(scale(1), serde_json::json!([1.0, 1.0, 0.6, 0.6]));
    assert_eq!(scale(2)[1], serde_json::json!(2.0));
    assert_eq!(shift(2)[1], serde_json::json!(0.5));
    assert_eq!(multiscale["metadata"]["volume"], true);
    // Level 2's array holds its 7 planes, and its chunks no more.
    let array: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("2").join("zarr.json")).unwrap()).unwrap();
    assert_eq!(array["shape"], serde_json::json!([1, 7, 225, 275]));
    let planes = std::fs::read_dir(path.join("2/c/0")).unwrap().count();
    assert_eq!(planes, 7);
}

#[test]
fn a_volume_of_unknown_voxels_halves_z_at_every_level_and_a_flat_image_never() {
    let folder = tempfile::tempdir().unwrap();
    let mut info = ImageInfo::new(1500, 600, 1, SampleType::U16);
    (info.planes, info.volume) = (9, true);
    let cube = write(&folder.path().join("cube.ome.zarr"), &info);
    assert_eq!(
        (0..cube.levels)
            .map(|l| cube.level_planes(l))
            .collect::<Vec<_>>(),
        [9, 5, 3]
    );
    // Once one plane is left, z halves no more.
    let mut two = info.clone();
    two.planes = 2;
    assert_eq!(
        (0..3).map(|l| two.z_halvings(l)).collect::<Vec<_>>(),
        [0, 1, 1]
    );
    info.volume = false;
    let flat = write(&folder.path().join("flat.ome.zarr"), &info);
    assert!((0..flat.levels).all(|l| flat.level_planes(l) == 9));
    let reader = ImageReader::open(&folder.path().join("flat.ome.zarr")).unwrap();
    assert!(!reader.info().volume);
    assert!(reader.read(2, 8, (0, 0), (1, 1)).is_ok());
    // An image recorded before volumes were, without the flag, isn't one.
    let mut recorded = serde_json::to_value(&cube).unwrap();
    recorded.as_object_mut().unwrap().remove("volume");
    let old: ImageInfo = serde_json::from_value(recorded).unwrap();
    assert!(!old.volume && old.planes == 9);
}
