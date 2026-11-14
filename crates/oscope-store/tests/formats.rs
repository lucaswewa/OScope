//! Importers and exporters: every format exported and imported back, across
//! samples, channels and stacks; OME-TIFF's metadata; PNG, JPEG and
//! multi-page TIFF files imported; what they refuse; where exports go; and
//! a Bayer capture's rendition.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use oscope_core::{BayerPattern, PixelFormat, ProjectId};
use oscope_store::{
    Acquisition, ImageInfo, ImageReader, NewProject, PixelSize, RecordKind, Rendition, SampleType,
    Samples, Store, StoreError, export, exporters, import, importers, render,
};
use serde_json::{Map, Value, json};
use tiff::encoder::{TiffEncoder, colortype};

fn open(root: &Path) -> (Store, ProjectId) {
    let store = Store::open(root, &root.join("index.sqlite")).unwrap();
    let project = NewProject {
        name: "formats".to_owned(),
        sample: String::new(),
        tags: Vec::new(),
        owner: "lab".to_owned(),
        metadata: Map::new(),
    };
    let id = store.create(project).unwrap().id;
    (store, id)
}

/// `len` samples of `sample`, of a pattern of their own for `seed`.
fn samples(sample: SampleType, len: usize, seed: usize) -> Samples {
    let at = |i: usize| (i * 7 + seed * 131) % 251;
    match sample {
        SampleType::U8 => Samples::U8((0..len).map(|i| at(i) as u8).collect()),
        SampleType::U16 => Samples::U16((0..len).map(|i| (at(i) * 257 + i % 3) as u16).collect()),
        SampleType::F32 => Samples::F32((0..len).map(|i| at(i) as f32 / 250.0).collect()),
    }
}

/// An acquisition of `info`, its planes from [`samples`], and `camera`.
fn acquisition(store: &Store, project: ProjectId, info: &ImageInfo, camera: Value) -> Acquisition {
    let mut draft = store.acquisition(project, "a field: 1/2", info).unwrap();
    draft.record().camera = camera;
    let image = draft.image().unwrap();
    let len = (info.width * info.height) as usize * info.channels as usize;
    for plane in 0..info.planes {
        let samples = samples(info.sample, len, plane as usize);
        image
            .write(plane, (0, 0), (info.width, info.height), &samples)
            .unwrap();
    }
    draft.commit().unwrap()
}

/// The planes of `acquisition`, at full resolution.
fn planes(store: &Store, project: ProjectId, acquisition: &Acquisition) -> Vec<Samples> {
    let path = store
        .folder(project)
        .join("acquisitions")
        .join(acquisition.id.to_string())
        .join("raw.ome.zarr");
    let reader = ImageReader::open(&path).unwrap();
    let info = reader.info().clone();
    (0..info.planes)
        .map(|p| {
            reader
                .read(0, p, (0, 0), (info.width, info.height))
                .unwrap()
        })
        .collect()
}

/// Exports `acquisition` as `format`, then imports the file back: the new
/// acquisition, and the file.
fn round_trip(
    store: &Store,
    project: ProjectId,
    acquisition: &Acquisition,
    format: &str,
) -> Result<(Acquisition, std::path::PathBuf), StoreError> {
    let exporter = exporters()
        .into_iter()
        .find(|e| e.format().id == format)
        .unwrap();
    let id = acquisition.id.to_string();
    let path = export(store, project, RecordKind::Acquisition, &id, &*exporter)?;
    let file = store.folder(project).join(&path);
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let importer = importers()
        .into_iter()
        .find(|i| i.format().matches(&name))
        .unwrap();
    Ok((import(store, project, &*importer, &file, None)?, file))
}

fn as_u16(samples: &Samples) -> Vec<u16> {
    match samples {
        Samples::U8(v) => v.iter().map(|s| u16::from(*s) * 257).collect(),
        Samples::U16(v) => v.clone(),
        Samples::F32(v) => v
            .iter()
            .map(|s| (s.clamp(0.0, 1.0) * 65_535.0).round() as u16)
            .collect(),
    }
}

#[test]
fn every_format_round_trips_across_samples_channels_and_stacks() {
    let root = tempfile::tempdir().unwrap();
    let (store, project) = open(root.path());
    let shapes = [
        (SampleType::U8, 1, 3),
        (SampleType::U16, 1, 2),
        (SampleType::U16, 3, 1),
        (SampleType::F32, 1, 2),
        (SampleType::U8, 4, 1),
        (SampleType::U16, 2, 1),
    ];
    for (sample, channels, plane_count) in shapes {
        let mut info = ImageInfo::new(301, 203, channels, sample);
        info.planes = plane_count;
        info.pixel_size = Some(PixelSize {
            x: 0.24,
            y: 0.25,
            z: 1.5,
        });
        info.channel_names = (0..channels).map(|c| format!("band {c}")).collect();
        let original = acquisition(&store, project, &info, Value::Null);
        let written = planes(&store, project, &original);
        let what = format!("{sample:?}, {channels} channels, {plane_count} planes");

        // OME-TIFF: every plane, as it was, with its scale and names.
        match round_trip(&store, project, &original, "ome-tiff") {
            Ok((back, file)) => {
                assert!(file.to_string_lossy().ends_with(".ome.tif"), "{what}");
                assert_eq!(planes(&store, project, &back), written, "{what}");
                assert_eq!(back.image.pixel_size, info.pixel_size, "{what}");
                assert_eq!(back.image.channel_names, info.channel_names, "{what}");
                assert_eq!(back.instrument["imported"]["format"], "tiff");
            }
            Err(StoreError::Refused(why)) => {
                assert_eq!(channels, 2, "{what}: {why}");
                assert!(why.contains("1, 3 or 4 channels"), "{why}");
            }
            Err(other) => panic!("{what}: {other}"),
        }

        // 16-bit TIFF: every plane, in 16 bits.
        if channels != 2 {
            let (back, _) = round_trip(&store, project, &original, "tiff16").unwrap();
            let expected: Vec<Samples> = written.iter().map(|p| Samples::U16(as_u16(p))).collect();
            assert_eq!(planes(&store, project, &back), expected, "{what}");
        }

        // PNG: the first plane, in 8 bits as it was, or 16.
        let (back, _) = round_trip(&store, project, &original, "png").unwrap();
        let first = match &written[0] {
            Samples::U8(v) => Samples::U8(v.clone()),
            other => Samples::U16(as_u16(other)),
        };
        assert_eq!(planes(&store, project, &back), [first], "{what}");
    }
}

#[test]
fn ome_tiff_s_metadata_says_what_the_image_is() {
    let root = tempfile::tempdir().unwrap();
    let (store, project) = open(root.path());
    let mut info = ImageInfo::new(64, 48, 1, SampleType::U16);
    info.planes = 3;
    info.pixel_size = Some(PixelSize {
        x: 0.24,
        y: 0.24,
        z: 2.0,
    });
    info.channel_names = vec!["raw & <grey> µ".to_owned()];
    let acquisition = acquisition(&store, project, &info, Value::Null);
    let exporter = exporters()
        .into_iter()
        .find(|e| e.format().id == "ome-tiff")
        .unwrap();
    let id = acquisition.id.to_string();
    let path = export(&store, project, RecordKind::Acquisition, &id, &*exporter).unwrap();
    // Named as the record, safely: "a field: 1/2".
    assert_eq!(
        path,
        format!("acquisitions/{id}/exports/a field- 1-2.ome.tif")
    );
    let file = File::open(store.folder(project).join(&path)).unwrap();
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(file)).unwrap();
    let xml = decoder
        .get_tag_ascii_string(tiff::tags::Tag::ImageDescription)
        .unwrap();
    for expected in [
        "xmlns=\"http://www.openmicroscopy.org/Schemas/OME/2016-06\"",
        "<Image ID=\"Image:0\" Name=\"a field: 1/2\">",
        "DimensionOrder=\"XYCZT\" Type=\"uint16\" SizeX=\"64\" SizeY=\"48\" SizeC=\"1\" SizeZ=\"3\" SizeT=\"1\"",
        "PhysicalSizeX=\"0.24\" PhysicalSizeY=\"0.24\"",
        "PhysicalSizeZ=\"2\"",
        "Name=\"raw &amp; &lt;grey&gt; &#xB5;\" SamplesPerPixel=\"1\"",
        "<TiffData IFD=\"0\" PlaneCount=\"3\"/>",
    ] {
        assert!(xml.contains(expected), "{expected} in {xml}");
    }
    let mut pages = 1;
    while decoder.more_images() {
        decoder.next_image().unwrap();
        // The metadata is in the first page alone.
        assert!(
            decoder
                .get_tag_ascii_string(tiff::tags::Tag::ImageDescription)
                .is_err()
        );
        pages += 1;
    }
    assert_eq!(pages, 3);
}

#[test]
fn png_jpeg_and_tiff_stacks_import() {
    let root = tempfile::tempdir().unwrap();
    let (store, project) = open(root.path());
    let (width, height) = (40_u32, 30_u32);
    let rgb: Vec<u8> = (0..width * height * 3).map(|i| (i % 256) as u8).collect();

    // An 8-bit RGB PNG, as it is.
    let png = root.path().join("field.png");
    oscope_imaging::color::encode_png(File::create(&png).unwrap(), &rgb, width, height).unwrap();
    let importer = importers()
        .into_iter()
        .find(|i| i.format().id == "png")
        .unwrap();
    let field = import(&store, project, &*importer, &png, None).unwrap();
    assert_eq!(field.name, "field");
    assert_eq!(
        (field.image.channels, field.image.sample),
        (3, SampleType::U8)
    );
    assert_eq!(planes(&store, project, &field), [Samples::U8(rgb.clone())]);

    // A JPEG, near what was encoded.
    let jpeg = root.path().join("field.jpg");
    let flat: Vec<u8> = (0..width * height)
        .flat_map(|_| [200_u8, 120, 40])
        .collect();
    std::fs::write(
        &jpeg,
        oscope_imaging::color::encode_jpeg(&flat, width, height, 95).unwrap(),
    )
    .unwrap();
    let importer = importers()
        .into_iter()
        .find(|i| i.format().matches("field.jpg"))
        .unwrap();
    let field = import(&store, project, &*importer, &jpeg, Some("photo")).unwrap();
    assert_eq!((field.name.as_str(), field.image.channels), ("photo", 3));
    let Samples::U8(read) = &planes(&store, project, &field)[0] else {
        panic!("8 bits");
    };
    assert!(read.iter().zip(&flat).all(|(a, b)| a.abs_diff(*b) <= 4));

    // A stack of 16-bit pages, a plane each; and one whose pages differ.
    let stack = |path: &Path, sizes: &[(u32, u32)]| {
        let file = BufWriter::new(File::create(path).unwrap());
        let mut encoder = TiffEncoder::new(file).unwrap();
        for (k, &(w, h)) in sizes.iter().enumerate() {
            let page: Vec<u16> = (0..w * h)
                .map(|i| (i as u16).wrapping_mul(k as u16 + 1))
                .collect();
            encoder
                .write_image::<colortype::Gray16>(w, h, &page)
                .unwrap();
        }
    };
    let tif = root.path().join("stack.tif");
    stack(&tif, &[(32, 24); 4]);
    let importer = importers()
        .into_iter()
        .find(|i| i.format().matches("stack.tif"))
        .unwrap();
    let field = import(&store, project, &*importer, &tif, None).unwrap();
    assert_eq!(
        (field.image.planes, field.image.sample),
        (4, SampleType::U16)
    );
    for (k, plane) in planes(&store, project, &field).iter().enumerate() {
        let page: Vec<u16> = (0..32 * 24)
            .map(|i| (i as u16).wrapping_mul(k as u16 + 1))
            .collect();
        assert_eq!(plane, &Samples::U16(page), "page {k}");
    }
    let uneven = root.path().join("uneven.tif");
    stack(&uneven, &[(32, 24), (16, 24)]);
    let error = import(&store, project, &*importer, &uneven, None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("page 2") && error.contains("share their size"),
        "{error}"
    );
    let broken = root.path().join("broken.tif");
    std::fs::write(&broken, b"II*\0not a tiff").unwrap();
    assert!(matches!(
        import(&store, project, &*importer, &broken, None),
        Err(StoreError::Document { .. })
    ));
}

#[test]
fn exports_refuse_what_they_cannot_find() {
    let root = tempfile::tempdir().unwrap();
    let (store, project) = open(root.path());
    let exporter = exporters()
        .into_iter()
        .find(|e| e.format().id == "png")
        .unwrap();
    let refused = |kind, id: &str| {
        export(&store, project, kind, id, &*exporter)
            .unwrap_err()
            .to_string()
    };
    assert!(refused(RecordKind::Acquisition, "../../x").contains("isn't a record's ID"));
    let missing = oscope_core::AcquisitionId::generate().to_string();
    assert!(refused(RecordKind::Acquisition, &missing).starts_with("no acquisition"));
    assert!(refused(RecordKind::Report, &missing).contains("a report has no image"));
    let provenance = oscope_store::Provenance {
        recipe: Value::Null,
        nodes: Vec::new(),
        inputs: Vec::new(),
        software: "a test".to_owned(),
    };
    let draft = store.result(project, "numbers", provenance, None).unwrap();
    let result = draft.commit().unwrap();
    assert!(refused(RecordKind::Result, &result.id.to_string()).contains("has no image"));
}

#[test]
fn a_bayer_capture_s_rendition_is_demosaiced() {
    let root = tempfile::tempdir().unwrap();
    let (store, project) = open(root.path());
    let format = PixelFormat::Bayer16 {
        pattern: BayerPattern::Rggb,
        significant_bits: 12,
    };
    // RGGB: the red sites bright, the green and blue dark.
    let info = ImageInfo::new(96, 64, 1, SampleType::U16);
    let mut draft = store.acquisition(project, "red", &info).unwrap();
    draft.record().camera = json!({ "format": format });
    let mosaic: Vec<u16> = (0..96 * 64)
        .map(|i| {
            if (i / 96) % 2 == 0 && i % 2 == 0 {
                60_000
            } else {
                1_000
            }
        })
        .collect();
    let image = draft.image().unwrap();
    image
        .write(0, (0, 0), (96, 64), &Samples::U16(mosaic))
        .unwrap();
    let raw = draft.commit().unwrap();
    let path = render(&store, project, raw.id, Rendition::Png).unwrap();
    assert_eq!(path, format!("acquisitions/{}/rendition.png", raw.id));
    let file = File::open(store.folder(project).join(&path)).unwrap();
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let png = reader.info();
    assert_eq!(
        (png.width, png.height, png.color_type),
        (96, 64, png::ColorType::Rgb)
    );
    // Demosaiced: red, not a grey mosaic.
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    let middle = (32 * 96 + 48) * 3;
    let (r, g, b) = (pixels[middle], pixels[middle + 1], pixels[middle + 2]);
    assert!(r > 200 && g < 60 && b < 60, "{r} {g} {b}");
    let path = render(&store, project, raw.id, Rendition::Jpeg).unwrap();
    assert!(store.folder(project).join(path).metadata().unwrap().len() > 100);
}
