//! Annotations and measurements (P44): annotations kept, replaced and
//! removed in layers; synthetic shapes measured in µm and in pixels; a
//! polygon's statistics against every pixel, the same tiled or untiled;
//! a raw image's by its filters' colours; and the CSV.

use oscope_core::{BayerPattern, PixelFormat, ProjectId};
use oscope_store::{
    Feature, Geometry, ImageInfo, NewProject, PixelSize, RecordKind, SampleType, Samples, Store,
    StoreError, measure, measurements_csv,
};
use serde_json::{Map, Value, json};

const ACQ: RecordKind = RecordKind::Acquisition;

struct Made {
    store: Store,
    project: ProjectId,
    id: String,
    _folder: tempfile::TempDir,
}

/// A store with one acquisition, of `info`, its samples from `value`.
fn made(info: &ImageInfo, value: impl Fn(u64, u64, u32, u32) -> f32, camera: Value) -> Made {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "annotations".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let mut draft = store.acquisition(project, "a slide", info).unwrap();
    let (w, h, c) = (info.width, info.height, info.channels);
    for p in 0..info.planes {
        let values: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| (0..c).map(move |k| (x, y, k))))
            .map(|(x, y, k)| value(x, y, k, p))
            .collect();
        let writer = draft.image().unwrap();
        let samples = match info.sample {
            SampleType::F32 => Samples::F32(values),
            _ => Samples::U16(values.iter().map(|&v| v as u16).collect()),
        };
        writer.write(p, (0, 0), (w, h), &samples).unwrap();
    }
    draft.record().camera = camera;
    let id = draft.commit().unwrap().id.to_string();
    Made {
        store,
        project,
        id,
        _folder: folder,
    }
}

fn polygon(rings: &[&[[f64; 2]]]) -> Feature {
    let rings = rings
        .iter()
        .map(|r| r.iter().map(|p| p.to_vec()).collect())
        .collect();
    feature(Geometry::Polygon(rings), "")
}

fn feature(geometry: Geometry, name: &str) -> Feature {
    let mut properties = Map::new();
    if !name.is_empty() {
        properties.insert("name".to_owned(), json!(name));
    }
    Feature {
        id: None,
        geometry,
        properties,
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h], [x, y]]
}

/// An integer pattern that varies along both axes and between channels.
fn counts(x: u64, y: u64, c: u32, p: u32) -> f32 {
    ((x * 7 + y * 13 + u64::from(c) * 1000 + u64::from(p) * 500) % 4096) as f32
}

fn u16_image(width: u64, height: u64, channels: u32) -> ImageInfo {
    let mut info = ImageInfo::new(width, height, channels, SampleType::U16);
    info.pixel_size = Some(PixelSize {
        x: 0.5,
        y: 0.25,
        z: 1.0,
    });
    info
}

#[test]
fn annotations_are_kept_replaced_and_removed_in_layers() {
    let m = made(&u16_image(64, 48, 1), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    assert!(
        store
            .annotations(project, ACQ, id)
            .unwrap()
            .layers
            .is_empty()
    );
    let square = polygon(&[&rect(1.0, 1.0, 4.0, 4.0)]);
    let one = store.annotate(project, ACQ, id, "cells", square).unwrap();
    let line = feature(
        Geometry::LineString(vec![vec![0.0, 0.0], vec![3.0, 4.0]]),
        "",
    );
    let two = store.annotate(project, ACQ, id, "lines", line).unwrap();
    let point = feature(Geometry::Point(vec![2.0, 2.0, 0.5]), "a nucleus");
    let three = store.annotate(project, ACQ, id, "cells", point).unwrap();
    let ids = [&one, &two, &three].map(|f| f.id.clone().unwrap());
    assert_eq!(ids, ["1", "2", "3"].map(str::to_owned));
    // Replaced in place, by its ID; an unknown ID isn't made.
    let mut moved = polygon(&[&rect(2.0, 2.0, 4.0, 4.0)]);
    moved.id = Some("1".to_owned());
    store
        .annotate(project, ACQ, id, "cells", moved.clone())
        .unwrap();
    moved.id = Some("9".to_owned());
    let unknown = store.annotate(project, ACQ, id, "cells", moved);
    assert!(
        matches!(unknown, Err(StoreError::NotFound { .. })),
        "{unknown:?}"
    );
    // Kept on disk, as GeoJSON in a versioned document, and read again.
    let reopened =
        Store::open(&m._folder.path().join("data"), &m._folder.path().join("i")).unwrap();
    let annotations = reopened.annotations(project, ACQ, id).unwrap();
    let cells = &annotations.layers["cells"].features;
    assert_eq!(cells.len(), 2);
    assert_eq!(
        cells[0].geometry,
        Geometry::Polygon(vec![
            rect(2.0, 2.0, 4.0, 4.0)
                .iter()
                .map(|p| p.to_vec())
                .collect()
        ])
    );
    let file = store
        .folder(project)
        .join(format!("acquisitions/{id}/annotations.json"));
    let document: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(document["schema"], "oscope.annotations");
    assert_eq!(document["version"], 1);
    let collection = &document["layers"]["cells"];
    assert_eq!(collection["type"], "FeatureCollection");
    assert_eq!(collection["features"][1]["type"], "Feature");
    assert_eq!(
        collection["features"][1]["geometry"],
        json!({"type": "Point", "coordinates": [2.0, 2.0, 0.5]})
    );
    assert_eq!(collection["features"][1]["properties"]["name"], "a nucleus");
    // Removed: a feature, then a layer; IDs aren't given twice.
    store
        .remove_annotation(project, ACQ, id, "cells", Some("3"))
        .unwrap();
    store
        .remove_annotation(project, ACQ, id, "lines", None)
        .unwrap();
    for (layer, feature) in [("cells", Some("3")), ("lines", None), ("lines", Some("2"))] {
        let again = store.remove_annotation(project, ACQ, id, layer, feature);
        assert!(
            matches!(again, Err(StoreError::NotFound { .. })),
            "{again:?}"
        );
    }
    let four = store.annotate(
        project,
        ACQ,
        id,
        "cells",
        feature(Geometry::Point(vec![1.0, 1.0]), ""),
    );
    assert_eq!(four.unwrap().id.as_deref(), Some("4"));
    let annotations = store.annotations(project, ACQ, id).unwrap();
    assert_eq!(annotations.layers.keys().collect::<Vec<_>>(), ["cells"]);
}

#[test]
fn annotations_kept_at_once_are_all_kept() {
    let m = made(&u16_image(64, 48, 1), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    std::thread::scope(|scope| {
        for t in 0..8 {
            scope.spawn(move || {
                for k in 0..25 {
                    let at = vec![f64::from(t), f64::from(k)];
                    let point = feature(Geometry::Point(at), "");
                    store.annotate(project, ACQ, id, "points", point).unwrap();
                }
            });
        }
    });
    let annotations = store.annotations(project, ACQ, id).unwrap();
    let mut ids: Vec<u64> = annotations.layers["points"]
        .features
        .iter()
        .map(|f| f.id.as_deref().unwrap().parse().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=200).collect::<Vec<u64>>());
}

#[test]
fn shapes_and_names_that_aren_t_annotations_are_refused() {
    let m = made(&u16_image(64, 48, 1), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    let open = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
    let flat = vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0], [0.0, 0.0]];
    let wrong = [
        polygon(&[&open]),
        polygon(&[&open[..3]]),
        polygon(&[&flat]),
        polygon(&[]),
        polygon(&[&rect(f64::NAN, 0.0, 1.0, 1.0)]),
        feature(Geometry::Point(vec![1.0]), ""),
        feature(Geometry::Point(vec![f64::NAN, 1.0]), ""),
        feature(
            Geometry::LineString(vec![vec![0.0, 0.0], vec![f64::INFINITY, 1.0]]),
            "",
        ),
        feature(Geometry::Point(vec![1.0, 2.0, 3.0, 4.0]), ""),
        feature(Geometry::LineString(vec![vec![1.0, 2.0]]), ""),
    ];
    for shape in wrong {
        let kept = store.annotate(project, ACQ, id, "cells", shape.clone());
        assert!(
            matches!(kept, Err(StoreError::Refused(_))),
            "{shape:?}: {kept:?}"
        );
    }
    let square = polygon(&[&rect(1.0, 1.0, 4.0, 4.0)]);
    for layer in ["", " cells", "cells ", "a\nb", &"x".repeat(65)] {
        let kept = store.annotate(project, ACQ, id, layer, square.clone());
        assert!(
            matches!(kept, Err(StoreError::Refused(_))),
            "{layer:?}: {kept:?}"
        );
    }
    let report = store.annotate(project, RecordKind::Report, id, "cells", square.clone());
    assert!(matches!(report, Err(StoreError::Refused(_))), "{report:?}");
    let named = store.annotate(project, ACQ, "../../x", "cells", square.clone());
    assert!(matches!(named, Err(StoreError::Refused(_))), "{named:?}");
    let missing = uuid::Uuid::now_v7().to_string();
    let unknown = store.annotate(project, ACQ, &missing, "cells", square);
    assert!(
        matches!(unknown, Err(StoreError::NotFound { .. })),
        "{unknown:?}"
    );
    assert!(
        store
            .annotations(project, ACQ, id)
            .unwrap()
            .layers
            .is_empty()
    );
}

/// The count, mean, standard deviation, least and greatest of `values`.
fn summary(values: &[f64]) -> [f64; 5] {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    [n, mean, sd, min, max]
}

#[test]
fn shapes_measure_in_micrometres_and_their_pixels_as_every_pixel_says() {
    // 3 × 3 chunks of 512, pixels of 0.5 × 0.25 µm.
    let (w, h) = (1500, 1100);
    let m = made(&u16_image(w, h, 3), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    // 50 × 25 µm: 100 × 100 pixels, across a chunk's corner.
    let square = rect(231.0, 115.0, 50.0, 25.0);
    store
        .annotate(
            project,
            ACQ,
            id,
            "areas",
            feature(
                Geometry::Polygon(vec![square.iter().map(|p| p.to_vec()).collect()]),
                "a, \"square\"",
            ),
        )
        .unwrap();
    let line = vec![vec![10.0, 10.0], vec![40.0, 50.0], vec![40.0, 60.0]];
    store
        .annotate(
            project,
            ACQ,
            id,
            "lines",
            feature(Geometry::LineString(line), ""),
        )
        .unwrap();
    store
        .annotate(
            project,
            ACQ,
            id,
            "points",
            feature(Geometry::Point(vec![7.0, 8.0]), ""),
        )
        .unwrap();
    let measured = measure(store, project, ACQ, id, None, 0, 512).unwrap();
    assert_eq!(measured.len(), 3);
    let (area, line, point) = (&measured[0], &measured[1], &measured[2]);
    assert_eq!(
        (
            area.layer.as_str(),
            line.layer.as_str(),
            point.layer.as_str()
        ),
        ("areas", "lines", "points")
    );
    assert_eq!(
        (area.unit.as_str(), area.geometry.as_str()),
        ("µm", "Polygon")
    );
    assert_eq!(
        (area.area, area.length, area.centroid),
        (1250.0, 150.0, [256.0, 127.5])
    );
    assert_eq!(area.name.as_deref(), Some("a, \"square\""));
    assert_eq!(
        (line.length, line.area, line.channels.len()),
        (60.0, 0.0, 0)
    );
    assert_eq!((point.length, point.centroid), (0.0, [7.0, 8.0]));
    // Every pixel of it, in pixels: columns 462–561, rows 460–559.
    for (c, stats) in area.channels.iter().enumerate() {
        let values: Vec<f64> = (460..560)
            .flat_map(|y| (462..562).map(move |x| f64::from(counts(x, y, c as u32, 0))))
            .collect();
        let [n, mean, sd, min, max] = summary(&values);
        assert_eq!(stats.name, format!("channel {c}"));
        assert_eq!((stats.pixels as f64, stats.min, stats.max), (n, min, max));
        assert!(
            (stats.mean - mean).abs() < 1e-9 * mean && (stats.sd - sd).abs() < 1e-9 * sd,
            "{stats:?} {mean} {sd}"
        );
    }
    // One layer's only.
    let lines = measure(store, project, ACQ, id, Some("lines"), 0, 512).unwrap();
    assert_eq!(lines, vec![line.clone()]);
}

#[test]
fn a_polygon_s_statistics_are_the_same_tiled_or_untiled() {
    let star: Vec<[f64; 2]> = (0..18)
        .map(|k| {
            let a = k as f64 * std::f64::consts::PI / 9.0 + 0.1;
            let r = if k % 2 == 0 { 360.3 } else { 150.7 };
            [380.25 + r * a.cos(), 140.6 + 0.5 * r * a.sin()]
        })
        .chain([[
            380.25 + 360.3 * 0.1_f64.cos(),
            140.6 + 180.15 * 0.1_f64.sin(),
        ]])
        .collect();
    let hole = rect(330.0, 120.0, 90.0, 40.0);
    let shape = polygon(&[&star, &hole]);
    for sample in [SampleType::U16, SampleType::F32] {
        let mut info = u16_image(1500, 1100, 2);
        info.sample = sample;
        let smooth = |x: u64, y: u64, c: u32, _: u32| {
            let v = (x as f32 * 0.013).sin() * (y as f32 * 0.021 + c as f32).cos();
            if sample == SampleType::F32 {
                0.5 + 0.4 * v
            } else {
                2000.0 + 1500.0 * v
            }
        };
        let m = made(&info, smooth, Value::Null);
        let (store, project, id) = (&m.store, m.project, m.id.as_str());
        store
            .annotate(project, ACQ, id, "star", shape.clone())
            .unwrap();
        let whole = measure(store, project, ACQ, id, None, 0, 4096).unwrap();
        for tile in [512, 100] {
            let tiled = measure(store, project, ACQ, id, None, 0, tile).unwrap();
            assert_eq!(tiled, whole, "{sample:?} in tiles of {tile}");
        }
        // And against every pixel whose centre, in µm, a crossing test puts
        // inside.
        let rings = [star.clone(), hole.clone()];
        let inside = |p: [f64; 2]| {
            let mut odd = false;
            for ring in &rings {
                for i in 1..ring.len() {
                    let (a, b) = (ring[i - 1], ring[i]);
                    if (a[1] > p[1]) != (b[1] > p[1])
                        && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
                    {
                        odd = !odd;
                    }
                }
            }
            odd
        };
        let held: Vec<(u64, u64)> = (0..1100_u64)
            .flat_map(|y| (0..1500_u64).map(move |x| (x, y)))
            .filter(|&(x, y)| inside([(x as f64 + 0.5) * 0.5, (y as f64 + 0.5) * 0.25]))
            .collect();
        for (c, stats) in whole[0].channels.iter().enumerate() {
            let values: Vec<f64> = held
                .iter()
                .map(|&(x, y)| {
                    let v = smooth(x, y, c as u32, 0);
                    if sample == SampleType::F32 {
                        f64::from(v)
                    } else {
                        f64::from(v as u16)
                    }
                })
                .collect();
            let [n, mean, sd, min, max] = summary(&values);
            assert_eq!(
                (stats.pixels as f64, stats.min, stats.max),
                (n, min, max),
                "{sample:?}"
            );
            assert!(
                (stats.mean - mean).abs() < 1e-9 * mean.abs() && (stats.sd - sd).abs() < 1e-9 * sd,
                "{stats:?} {mean} {sd}"
            );
        }
    }
}

#[test]
fn units_follow_the_pixel_size_and_planes_are_chosen() {
    // Unknown pixels: a µm is a pixel, and the unit says so.
    let mut info = ImageInfo::new(300, 200, 1, SampleType::U16);
    info.planes = 2;
    let m = made(&info, counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    store
        .annotate(
            project,
            ACQ,
            id,
            "areas",
            polygon(&[&rect(10.0, 20.0, 30.0, 40.0)]),
        )
        .unwrap();
    let first = measure(store, project, ACQ, id, None, 0, 512).unwrap();
    assert_eq!(
        (
            first[0].unit.as_str(),
            first[0].area,
            first[0].channels[0].pixels
        ),
        ("px", 1200.0, 1200)
    );
    let second = measure(store, project, ACQ, id, None, 1, 512).unwrap();
    let mean = |p: u32| {
        let values: Vec<f64> = (20..60)
            .flat_map(|y| (10..40).map(move |x| f64::from(counts(x, y, 0, p))))
            .collect();
        summary(&values)[1]
    };
    assert!((first[0].channels[0].mean - mean(0)).abs() < 1e-9);
    assert!((second[0].channels[0].mean - mean(1)).abs() < 1e-9);
    assert!(matches!(
        measure(store, project, ACQ, id, None, 2, 512),
        Err(StoreError::Refused(_))
    ));
    let header = "layer,id,name,geometry,length_px,area_px2,centroid_x_px,centroid_y_px,";
    assert!(measurements_csv(&first).starts_with(header));
    // Partly off the image, its pixels on it; wholly off, none, and nothing
    // to say of them.
    let (half, off) = (rect(-10.0, 20.0, 30.0, 40.0), rect(400.0, 0.0, 9.0, 9.0));
    for (shape, pixels) in [(half, 20 * 40), (off, 0)] {
        let mut kept = store.annotate(project, ACQ, id, "edges", polygon(&[&shape]));
        let edge = kept.as_mut().unwrap().id.take().unwrap();
        let measured = measure(store, project, ACQ, id, Some("edges"), 0, 512).unwrap();
        let stats = &measured[0].channels[0];
        assert_eq!(stats.pixels, pixels, "{shape:?}");
        if pixels == 0 {
            assert_eq!([stats.mean, stats.sd, stats.min, stats.max], [0.0; 4]);
        }
        store
            .remove_annotation(project, ACQ, id, "edges", Some(&edge))
            .unwrap();
    }
    // Pixels of 0.5 × 0.25 µm: 30 × 40 µm is 60 × 160 of them.
    let m = made(&u16_image(300, 200, 1), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    store
        .annotate(
            project,
            ACQ,
            id,
            "areas",
            polygon(&[&rect(10.0, 5.0, 30.0, 40.0)]),
        )
        .unwrap();
    let measured = &measure(store, project, ACQ, id, None, 0, 512).unwrap()[0];
    assert_eq!(
        (
            measured.unit.as_str(),
            measured.area,
            measured.channels[0].pixels
        ),
        ("µm", 1200.0, 60 * 160)
    );
}

#[test]
fn a_raw_image_s_pixels_are_measured_by_their_filters_colours() {
    let mut info = u16_image(256, 128, 1);
    info.channel_names = vec!["raw".to_owned()];
    let pattern = BayerPattern::Grbg;
    let colour = |x: u64, y: u64, _: u32, _: u32| match pattern.colour_at(x as u32, y as u32) {
        oscope_core::BayerColour::Red => 1000.0 + (x % 3) as f32,
        oscope_core::BayerColour::Green => 2000.0,
        oscope_core::BayerColour::Blue => 3000.0,
    };
    let format = PixelFormat::Bayer16 {
        pattern,
        significant_bits: 12,
    };
    let m = made(&info, colour, json!({ "format": format }));
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    // 10 × 10 pixels from (21, 41): odd, so a cell shifted from the image's.
    store
        .annotate(
            project,
            ACQ,
            id,
            "areas",
            polygon(&[&rect(10.5, 10.25, 5.0, 2.5)]),
        )
        .unwrap();
    let channels = &measure(store, project, ACQ, id, None, 0, 512).unwrap()[0].channels;
    let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["red", "green", "blue"]);
    assert_eq!(
        channels.iter().map(|c| c.pixels).collect::<Vec<_>>(),
        [25, 50, 25]
    );
    assert_eq!(
        (channels[1].mean, channels[1].sd, channels[2].mean),
        (2000.0, 0.0, 3000.0)
    );
    // GRBG's red sites: rows 42, 44…, columns 21, 23…, 1000 + x mod 3.
    let reds: Vec<f64> = (0..5)
        .flat_map(|_| (0..5).map(|i| 1000.0 + ((21 + 2 * i) % 3) as f64))
        .collect();
    assert!(
        (channels[0].mean - summary(&reds)[1]).abs() < 1e-9,
        "{:?}",
        channels[0]
    );
}

#[test]
fn measurements_are_written_as_csv() {
    let m = made(&u16_image(64, 48, 2), counts, Value::Null);
    let (store, project, id) = (&m.store, m.project, m.id.as_str());
    store
        .annotate(
            project,
            ACQ,
            id,
            "a",
            feature(
                Geometry::Polygon(vec![
                    rect(1.0, 1.0, 4.0, 2.0)
                        .iter()
                        .map(|p| p.to_vec())
                        .collect(),
                ]),
                "one, \"two\"",
            ),
        )
        .unwrap();
    store
        .annotate(
            project,
            ACQ,
            id,
            "b",
            feature(Geometry::Point(vec![1.5, 2.0]), ""),
        )
        .unwrap();
    let csv = measurements_csv(&measure(store, project, ACQ, id, None, 0, 512).unwrap());
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(lines.len(), 4, "{csv}");
    assert_eq!(
        lines[0],
        "layer,id,name,geometry,length_um,area_um2,centroid_x_um,centroid_y_um,channel 0 pixels,channel 0 mean,channel 0 sd,channel 0 min,channel 0 max,channel 1 pixels,channel 1 mean,channel 1 sd,channel 1 min,channel 1 max"
    );
    assert!(
        lines[1].starts_with("a,1,\"one, \"\"two\"\"\",Polygon,12,8,3,2,64,"),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2], "b,2,,Point,0,0,1.5,2,,,,,,,,,,");
    assert_eq!(lines[3], "");
    assert_eq!(
        measurements_csv(&[]),
        "layer,id,name,geometry,length_um,area_um2,centroid_x_um,centroid_y_um\r\n"
    );
}
