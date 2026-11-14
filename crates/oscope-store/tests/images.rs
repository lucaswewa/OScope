//! OME-Zarr images: written in regions, read back at every level, the
//! pyramid's means checked against their definition, and the metadata
//! checked against NGFF 0.5's official schemas.

use std::path::{Path, PathBuf};

use oscope_store::{
    ImageInfo, ImageReader, ImageWriter, PixelSize, SampleType, Samples, StoreError,
};
use serde_json::Value;

/// A test pattern's sample at (`x`, `y`), channel `c`, plane `z`.
fn level(x: u64, y: u64, c: u64, z: u64) -> f64 {
    ((x * 37 + y * 101 + c * 7_919 + z * 131) % 4_096) as f64 + ((x ^ y) % 7) as f64
}

/// The pattern of a region, as `sample`s.
fn pattern(info: &ImageInfo, plane: u32, (x0, y0): (u64, u64), (w, h): (u64, u64)) -> Samples {
    let values = (y0..y0 + h).flat_map(move |y| {
        (x0..x0 + w).flat_map(move |x| {
            (0..u64::from(info.channels)).map(move |c| level(x, y, c, u64::from(plane)))
        })
    });
    match info.sample {
        SampleType::U8 => Samples::U8(values.map(|v| (v as u64 % 256) as u8).collect()),
        SampleType::U16 => Samples::U16(values.map(|v| (v * 16.0) as u16).collect()),
        SampleType::F32 => Samples::F32(values.map(|v| (v / 4_096.0) as f32 - 0.25).collect()),
    }
}

fn as_f64(samples: &Samples) -> Vec<f64> {
    match samples {
        Samples::U8(v) => v.iter().map(|&s| f64::from(s)).collect(),
        Samples::U16(v) => v.iter().map(|&s| f64::from(s)).collect(),
        Samples::F32(v) => v.iter().map(|&s| f64::from(s)).collect(),
    }
}

/// The next level, by the definition: each pixel the mean of the 2 × 2 (or,
/// at an odd edge, fewer) pixels below it, rounded for integer samples.
fn next_level(
    samples: &[f64],
    (w, h): (u64, u64),
    channels: usize,
    sample: SampleType,
) -> Vec<f64> {
    let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = Vec::new();
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..channels {
                let mut sum_count = (0.0, 0.0);
                for (yy, xx) in [
                    (2 * y, 2 * x),
                    (2 * y, 2 * x + 1),
                    (2 * y + 1, 2 * x),
                    (2 * y + 1, 2 * x + 1),
                ] {
                    if yy < h && xx < w {
                        sum_count.0 += samples[((yy * w + xx) as usize) * channels + c];
                        sum_count.1 += 1.0;
                    }
                }
                let mean = sum_count.0 / sum_count.1;
                out.push(match sample {
                    SampleType::F32 => f64::from(mean as f32),
                    _ => mean.round(),
                });
            }
        }
    }
    out
}

/// Writes `info`'s pattern in strips of uneven heights, as a camera or a
/// stitcher hands rows over, then finishes the pyramid.
fn write(path: &Path, info: &ImageInfo) -> ImageInfo {
    let writer = ImageWriter::create(path, info, "test").unwrap();
    for plane in 0..info.planes {
        let mut y = 0;
        for height in [1, 300, 511, 512, 513].iter().cycle() {
            let h = (*height).min(info.height - y);
            let samples = pattern(info, plane, (0, y), (info.width, h));
            writer
                .write(plane, (0, y), (info.width, h), &samples)
                .unwrap();
            y += h;
            if y == info.height {
                break;
            }
        }
    }
    writer.finish().unwrap()
}

/// Checks every level of every plane of the image at `path` against the
/// pattern and the definition of its pyramid.
fn check(path: &Path, info: &ImageInfo) {
    let reader = ImageReader::open(path).unwrap();
    assert_eq!(reader.info(), info);
    let channels = info.channels as usize;
    for plane in 0..info.planes {
        let full = (info.width, info.height);
        let mut expected = as_f64(&pattern(info, plane, (0, 0), full));
        for level in 0..info.levels {
            let size = info.level_size(level);
            if level > 0 {
                expected = next_level(&expected, info.level_size(level - 1), channels, info.sample);
            }
            let read = as_f64(&reader.read(level, plane, (0, 0), size).unwrap());
            assert!(read == expected, "level {level} of plane {plane}");
        }
        // A region inside, across chunks' edges.
        let (x, y, w, h) = (500, 9, 30, 520.min(info.height - 9));
        let region = reader.read(0, plane, (x, y), (w, h)).unwrap();
        assert_eq!(region, pattern(info, plane, (x, y), (w, h)));
    }
}

#[test]
fn images_round_trip_at_every_level_for_each_sample_type() {
    let folder = tempfile::tempdir().unwrap();
    for (n, (sample, channels, planes)) in [
        (SampleType::U8, 1, 1),
        (SampleType::U16, 3, 2),
        (SampleType::F32, 2, 1),
        (SampleType::U16, 4, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let mut info = ImageInfo::new(1_301, 1_029, channels, sample);
        info.planes = planes;
        info.pixel_size = Some(PixelSize {
            x: 0.345,
            y: 0.345,
            z: 2.0,
        });
        info.channel_names = (0..channels).map(|c| format!("c{c}")).collect();
        let path = folder.path().join(format!("{n}.ome.zarr"));
        let written = write(&path, &info);
        assert_eq!(written.levels, 3, "1301 → 651 → 326");
        check(&path, &written);
    }
}

#[test]
fn a_full_michrome_20_frame_round_trips() {
    // The camera's whole sensor in 16 bits: 5472 × 3648, five levels.
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("frame.ome.zarr");
    let info = ImageInfo::new(5_472, 3_648, 1, SampleType::U16);
    let written = write(&path, &info);
    assert_eq!(written.levels, 5);
    assert_eq!(written.level_size(4), (342, 228));
    check(&path, &written);
    // One chunk holds a level that fits it: 342 × 228, in one file.
    let chunks = folder.path().join("frame.ome.zarr/4/c/0/0");
    assert_eq!(std::fs::read_dir(chunks).unwrap().count(), 1);
}

/// The vendored schemas, by their `$id`.
struct Schemas;

fn schema(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/ngff-0.5")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

impl jsonschema::Retrieve for Schemas {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let name = uri
            .as_str()
            .strip_prefix("https://ngff.openmicroscopy.org/0.5/schemas/");
        name.map(schema)
            .ok_or_else(|| format!("not one of NGFF 0.5's schemas: {uri}").into())
    }
}

/// The errors of `attributes` against NGFF 0.5's schema `name`.
fn ngff_errors(name: &str, attributes: &Value) -> Vec<String> {
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .with_retriever(Schemas)
        .build(&schema(name))
        .unwrap();
    validator
        .iter_errors(attributes)
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn the_metadata_is_ngff_0_5_s() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("image.ome.zarr");
    let mut info = ImageInfo::new(1_100, 600, 3, SampleType::U16);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.25,
        z: 1.0,
    });
    write(&path, &info);
    let group: Value =
        serde_json::from_slice(&std::fs::read(path.join("zarr.json")).unwrap()).unwrap();
    let attributes = &group["attributes"];
    for name in ["image.schema", "strict_image.schema"] {
        assert_eq!(
            ngff_errors(name, attributes),
            Vec::<String>::new(),
            "{name}"
        );
    }
    // The schemas do check: a version they don't know, or no axes, fails.
    let mut wrong = attributes.clone();
    wrong["ome"]["version"] = "0.4".into();
    assert!(!ngff_errors("image.schema", &wrong).is_empty());
    let mut wrong = attributes.clone();
    wrong["ome"]["multiscales"][0]
        .as_object_mut()
        .unwrap()
        .remove("axes");
    assert!(!ngff_errors("image.schema", &wrong).is_empty());

    // Level 2's pixels are 4 × 4 of level 0's, and centred on them.
    let multiscale = &attributes["ome"]["multiscales"][0];
    let level2 = &multiscale["datasets"][2]["coordinateTransformations"];
    assert_eq!(level2[0]["scale"], serde_json::json!([1.0, 1.0, 1.0, 2.0]));
    assert_eq!(
        level2[1]["translation"],
        serde_json::json!([0.0, 0.0, 0.375, 0.75])
    );
    assert_eq!(
        multiscale["axes"][3],
        serde_json::json!({"name": "x", "type": "space", "unit": "micrometer"})
    );
    // Each array is a Zarr v3 array, compressed with zstd, in tiles of 512.
    let array: Value =
        serde_json::from_slice(&std::fs::read(path.join("0/zarr.json")).unwrap()).unwrap();
    assert_eq!(array["zarr_format"], 3);
    assert_eq!(array["shape"], serde_json::json!([3, 1, 600, 1100]));
    assert_eq!(
        array["chunk_grid"]["configuration"]["chunk_shape"],
        serde_json::json!([3, 1, 512, 512])
    );
    assert_eq!(
        array["dimension_names"],
        serde_json::json!(["c", "z", "y", "x"])
    );
    let codecs: Vec<&str> = array["codecs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(codecs, ["bytes", "zstd"]);
}

#[test]
fn images_that_do_not_hold_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let refused = |info: &ImageInfo| match ImageWriter::create(
        &folder.path().join("x.ome.zarr"),
        info,
        "x",
    ) {
        Err(StoreError::Refused(why)) => why,
        other => panic!("{:?}", other.map(|w| w.info().clone())),
    };
    let mut info = ImageInfo::new(0, 10, 1, SampleType::U8);
    assert_eq!(
        refused(&info),
        "an image needs at least one pixel and one plane"
    );
    info = ImageInfo::new(10, 10, 5, SampleType::U8);
    assert_eq!(refused(&info), "an image has 1 to 4 channels");
    info = ImageInfo::new(10, 10, 2, SampleType::U8);
    info.channel_names = vec!["one".to_owned()];
    assert_eq!(
        refused(&info),
        "an image names each of its channels, or none"
    );
    info = ImageInfo::new(10, 10, 1, SampleType::U8);
    info.pixel_size = Some(PixelSize {
        x: 0.0,
        y: 1.0,
        z: 1.0,
    });
    assert_eq!(
        refused(&info),
        "an image has pixels of a positive, finite size"
    );

    let info = ImageInfo::new(100, 50, 3, SampleType::U16);
    let writer = ImageWriter::create(&folder.path().join("y.ome.zarr"), &info, "y").unwrap();
    let samples = Samples::U16(vec![0; 10 * 10 * 3]);
    let outside = writer
        .write(0, (95, 0), (10, 10), &samples)
        .unwrap_err()
        .to_string();
    assert_eq!(
        outside,
        "a region of 10 × 10 at (95, 0) in plane 0: the image is 100 × 50 in 1 planes"
    );
    let wrong = writer
        .write(0, (0, 0), (10, 10), &Samples::U8(vec![0; 300]))
        .unwrap_err()
        .to_string();
    assert_eq!(wrong, "300 samples of U8 for 300 of U16");
    let short = writer
        .write(0, (0, 0), (10, 10), &Samples::U16(vec![0; 299]))
        .unwrap_err()
        .to_string();
    assert_eq!(short, "299 samples of U16 for 300 of U16");
    assert!(
        writer.write(1, (0, 0), (10, 10), &samples).is_err(),
        "no plane 1"
    );
}
