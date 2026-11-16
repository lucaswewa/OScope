//! Volumes over HTTP (P41): a z-stack's metadata and levels; slices across
//! z, y and x and bricks, their header and voxels the store's own, as 8 or
//! 16 bits; the level chosen to fit; what is refused; and a raw volume from
//! its first level.

use oscope_api::{Api, Roots};
use oscope_core::ProjectId;
use oscope_store::{ImageInfo, ImageReader, NewProject, PixelSize, SampleType, Samples, Store};
use serde_json::json;
use teta_wot::testing::{TestClient, TestResponse};

/// Sample `(x, y, z)`, from 0 to 1.
fn value(x: u64, y: u64, z: u64) -> f32 {
    ((x * 7 + y * 13 + z * 29) % 1000) as f32 / 1000.0
}

struct Served {
    client: TestClient,
    project: ProjectId,
    id: String,
    image: std::path::PathBuf,
    _folder: tempfile::TempDir,
}

/// A volume of 1100 × 900 × 13 voxels of 0.3 × 0.3 × 1 µm in 16 bits, or
/// a raw mosaic's.
fn stack(raw: bool) -> ImageInfo {
    let mut info = ImageInfo::new(1100, 900, 1, SampleType::U16);
    (info.planes, info.volume) = (13, true);
    info.pixel_size = Some(PixelSize {
        x: 0.3,
        y: 0.3,
        z: 1.0,
    });
    if raw {
        info.channel_names = vec!["raw".to_owned()];
    }
    info
}

/// A server with one acquisition, of `info`'s voxels, `value` at each.
async fn serve(info: ImageInfo) -> Served {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "volumes".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let draft = store.acquisition(project, "a stack", &info).unwrap();
    let writer = draft.image().unwrap();
    let (w, h) = (info.width, info.height);
    for z in 0..u64::from(info.planes) {
        let values: Vec<f32> = (0..h)
            .flat_map(|y| (0..w).map(move |x| value(x, y, z)))
            .collect();
        let samples = Samples::from_unit(info.sample, &values);
        writer.write(z as u32, (0, 0), (w, h), &samples).unwrap();
    }
    let id = draft.commit().unwrap().id.to_string();
    let image = store
        .folder(project)
        .join("acquisitions")
        .join(&id)
        .join("raw.ome.zarr");
    let roots = Roots {
        data,
        scratch: folder.path().join("scratch"),
    };
    let api = Api::builder(&roots).projects(store).build().unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    Served {
        client,
        project,
        id,
        image,
        _folder: folder,
    }
}

impl Served {
    async fn get(&self, rest: &str) -> TestResponse {
        let path = format!(
            "/api/v1/volume/{}/acquisitions/{}/{rest}",
            self.project, self.id
        );
        self.client.get(&path).await
    }
}

/// A header, read back.
#[derive(Debug, PartialEq)]
struct Header {
    sample: u8,
    channels: u8,
    level: u32,
    size: [u32; 3],
    origin: [u32; 3],
    voxel: [f32; 3],
}

fn header(body: &[u8]) -> Header {
    assert_eq!(&body[..4], b"OSV1");
    assert_eq!(u16::from_le_bytes([body[4], body[5]]), 64);
    let u = |at: usize| u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
    let f = |at: usize| f32::from_le_bytes(body[at..at + 4].try_into().unwrap());
    assert!(body[48..64].iter().all(|&b| b == 0));
    Header {
        sample: body[6],
        channels: body[7],
        level: u(8),
        size: [u(12), u(16), u(20)],
        origin: [u(24), u(28), u(32)],
        voxel: [f(36), f(40), f(44)],
    }
}

/// The store's samples of the box at `origin`, `size`, of `level`, x
/// fastest, then y, then z, as 16 bits.
fn source(image: &std::path::Path, level: u32, origin: [u64; 3], size: [u64; 3]) -> Vec<u16> {
    let reader = ImageReader::open(image).unwrap();
    let mut out = Vec::new();
    for z in origin[2]..origin[2] + size[2] {
        let Samples::U16(plane) = reader
            .read(level, z as u32, (origin[0], origin[1]), (size[0], size[1]))
            .unwrap()
        else {
            panic!("16-bit samples");
        };
        out.extend(plane);
    }
    out
}

fn eight(v: u16) -> u8 {
    ((u32::from(v) * 255 + 32_767) / 65_535) as u8
}

#[tokio::test(flavor = "multi_thread")]
async fn a_volume_s_slices_and_bricks_are_its_own_voxels() {
    let served = serve(stack(false)).await;
    let meta = served.get("meta").await.json();
    assert_eq!(
        (
            &meta["size"],
            &meta["voxel_um"],
            &meta["volume"],
            &meta["raw"],
            &meta["header"]
        ),
        (
            &json!([1100, 900, 13]),
            &json!([0.3, 0.3, 1.0]),
            &json!(true),
            &json!(false),
            &json!(64)
        ),
        "{meta}"
    );
    let levels: Vec<_> = meta["levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["size"].clone(), l["voxel_um"].clone()))
        .collect();
    assert_eq!(levels.len(), 3);
    assert_eq!(levels[1], (json!([550, 450, 13]), json!([0.6, 0.6, 1.0])));
    assert_eq!(levels[2].0, json!([275, 225, 7]));
    let voxel = levels[2].1.as_array().unwrap();
    assert!((voxel[0].as_f64().unwrap() - 1.2).abs() < 1e-9 && voxel[2] == json!(2.0));

    // A slice across z, at level 1, in 8 bits.
    let slice = served.get("slice?axis=z&index=5&level=1").await;
    assert_eq!(slice.status, 200);
    assert_eq!(
        slice.header("content-type"),
        Some("application/octet-stream")
    );
    let head = header(&slice.body);
    assert_eq!(
        head,
        Header {
            sample: 1,
            channels: 1,
            level: 1,
            size: [550, 450, 1],
            origin: [0, 0, 5],
            voxel: [0.6, 0.6, 1.0],
        }
    );
    let want: Vec<u8> = source(&served.image, 1, [0, 0, 5], [550, 450, 1])
        .into_iter()
        .map(eight)
        .collect();
    assert_eq!(&slice.body[64..], &want[..]);
    // Across y and x, at level 2, in 16 bits: a row, and a column, of each
    // of its 7 planes.
    let across = served
        .get("slice?axis=y&index=100&level=2&format=u16")
        .await;
    let head = header(&across.body);
    assert_eq!(
        (head.sample, head.size, head.origin),
        (2, [275, 1, 7], [0, 100, 0])
    );
    let want = source(&served.image, 2, [0, 100, 0], [275, 1, 7]);
    let got: Vec<u16> = across.body[64..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .collect();
    assert_eq!(got, want);
    let down = served.get("slice?axis=x&index=274&level=2").await;
    let head = header(&down.body);
    assert_eq!((head.size, head.origin), ([1, 225, 7], [274, 0, 0]));
    let want: Vec<u8> = source(&served.image, 2, [274, 0, 0], [1, 225, 7])
        .into_iter()
        .map(eight)
        .collect();
    assert_eq!(&down.body[64..], &want[..]);

    // A brick: the whole volume fits 512³ at full size.
    let whole = served.get("brick").await;
    let head = header(&whole.body);
    assert_eq!((head.level, head.size), (0, [1100, 900, 13]));
    assert_eq!(whole.body.len(), 64 + 1100 * 900 * 13);
    // Held to a million voxels: the finest level that fits, the third; to
    // the second's own count, the second; to one, the coarsest.
    for (most, level) in [(1_000_000, 2), (550 * 450 * 13, 1), (1, 2)] {
        let fits = served.get(&format!("brick?max={most}")).await;
        assert_eq!(header(&fits.body).level, level, "{most}");
    }
    // A box, given in full-size voxels, at level 1: from its first voxel's
    // to past its last's.
    let boxed = served
        .get("brick?x=101&y=33&z=3&w=300&h=200&d=5&level=1&format=u16")
        .await;
    let head = header(&boxed.body);
    assert_eq!((head.origin, head.size), ([50, 16, 3], [151, 101, 5]));
    let want = source(&served.image, 1, [50, 16, 3], [151, 101, 5]);
    let got: Vec<u16> = boxed.body[64..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .collect();
    assert_eq!(got, want);

    // Deep zoom through a level that halves z: plane 12 is its plane 6.
    let zoom = format!(
        "/api/v1/zoom/{}/acquisitions/{}/image_files/9/0_0.jpg?plane=12",
        served.project, served.id
    );
    let tile = served.client.get(&zoom).await;
    assert_eq!(tile.status, 200, "{}", String::from_utf8_lossy(&tile.body));

    // What is refused, and why: past the last plane, by the route itself,
    // even where level 2's 7 planes would hold the box rounded.
    for (bad, why) in [
        ("slice?axis=q&index=0", "no axis q"),
        ("slice?axis=z&index=13", "no index 13 across z"),
        ("slice?axis=z&index=0&level=3", "no level 3"),
        ("brick?x=1000&w=200", "inside 1100 × 900 × 13"),
        ("brick?z=12&d=2&level=2", "inside 1100 × 900 × 13"),
        ("brick?level=3", "no level 3"),
        ("brick?d=0", "not empty"),
    ] {
        let refused = served.get(bad).await;
        let detail = refused.json()["detail"].to_string();
        assert!(
            refused.status == 400 && detail.contains(why),
            "{bad}: {detail}"
        );
    }
    let nobody = format!(
        "/api/v1/volume/{}/acquisitions/{}/meta",
        ProjectId::generate(),
        served.id
    );
    assert_eq!(served.client.get(&nobody).await.status, 404);
    let reports = format!(
        "/api/v1/volume/{}/reports/{}/meta",
        served.project, served.id
    );
    assert_eq!(served.client.get(&reports).await.status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_raw_volume_starts_at_its_first_level() {
    let served = serve(stack(true)).await;
    let meta = served.get("meta").await.json();
    assert_eq!(meta["raw"], json!(true));
    assert_eq!(meta["levels"][0]["level"], json!(1), "{meta}");
    assert_eq!(served.get("slice?axis=z&index=0&level=0").await.status, 400);
    assert_eq!(served.get("brick?level=0").await.status, 400);
    let brick = served.get("brick").await;
    assert_eq!(header(&brick.body).level, 1);
    let slice = served.get("slice?axis=z&index=2").await;
    assert_eq!(header(&slice.body).level, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn eight_bit_and_float_volumes_come_in_either_format() {
    for sample in [SampleType::U8, SampleType::F32] {
        // Of unknown voxels, so z halves at each level: its thumbnail, of
        // level 2, is of plane 1 of 3 there.
        let mut info = ImageInfo::new(2100, 300, 1, sample);
        (info.planes, info.volume) = (9, true);
        let served = serve(info).await;
        let reader = ImageReader::open(&served.image).unwrap();
        // 8 bits to 16 by 257; floats clamped from 0 to 1, then rounded.
        let (eight, sixteen): (Vec<u8>, Vec<u8>) =
            match reader.read(0, 1, (0, 0), (2100, 300)).unwrap() {
                Samples::U8(v) => (
                    v.clone(),
                    v.iter()
                        .flat_map(|&s| (u16::from(s) * 257).to_le_bytes())
                        .collect(),
                ),
                Samples::F32(v) => (
                    v.iter()
                        .map(|s| (s.clamp(0.0, 1.0) * 255.0).round() as u8)
                        .collect(),
                    v.iter()
                        .flat_map(|s| ((s.clamp(0.0, 1.0) * 65_535.0).round() as u16).to_le_bytes())
                        .collect(),
                ),
                Samples::U16(_) => panic!("written as {sample:?}"),
            };
        for (format, bytes, want) in [("u8", 1, eight), ("u16", 2, sixteen)] {
            let slice = served
                .get(&format!("slice?axis=z&index=1&level=0&format={format}"))
                .await;
            assert_eq!(header(&slice.body).sample, bytes);
            assert!(slice.body[64..] == want[..], "{sample:?} as {format}");
        }
    }
}
