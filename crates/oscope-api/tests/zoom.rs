//! Deep zoom over HTTP (P40): the descriptor and tiles of a project's
//! image, cached and timed, at every level and edge; looks of their own;
//! what is refused; a raw image in colour once its overview is built; and
//! a DZI export that is what is served.

use std::time::{Duration, Instant};

use oscope_api::{Api, Roots};
use oscope_core::{BayerColour, BayerPattern, PixelFormat, ProjectId};
use oscope_imaging::zoom::Dzi;
use oscope_jobs::{JobService, JobsConfig};
use oscope_store::{ImageInfo, NewProject, SampleType, Samples, Store};
use serde_json::{Value, json};
use teta_wot::testing::{TestClient, TestResponse};
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// An image's colour at `(x, y)`: reddish, its blue low, a little texture.
fn colour(x: u64, y: u64) -> [f32; 3] {
    let t = 0.05 * ((x as f32 * 0.05).sin() + (y as f32 * 0.07).cos());
    [0.8 + t, 0.5 + t, 0.2 + t]
}

struct Served {
    client: TestClient,
    project: ProjectId,
    id: String,
    scratch: std::path::PathBuf,
    _folder: tempfile::TempDir,
}

/// A server of one project, with an acquisition `w` × `h`: a raw Bayer
/// mosaic of its camera, or 8-bit RGB.
async fn serve(raw: bool, (w, h): (u64, u64)) -> Served {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let roots = Roots {
        data: data.clone(),
        scratch: folder.path().join("scratch"),
    };
    let store = Store::open(&data, &data.join("index.sqlite")).unwrap();
    let new = NewProject {
        name: "zoom".to_owned(),
        ..NewProject::default()
    };
    let project = store.create(new).unwrap().id;
    let pattern = BayerPattern::Rggb;
    let (info, values) = if raw {
        let mut info = ImageInfo::new(w, h, 1, SampleType::U16);
        info.channel_names = vec!["raw".to_owned()];
        let values = (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    let c = match pattern.colour_at(x as u32, y as u32) {
                        BayerColour::Red => 0,
                        BayerColour::Green => 1,
                        BayerColour::Blue => 2,
                    };
                    colour(x, y)[c]
                })
            })
            .collect::<Vec<f32>>();
        (info, values)
    } else {
        let info = ImageInfo::new(w, h, 3, SampleType::U8);
        let values = (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| colour(x, y)))
            .collect();
        (info, values)
    };
    let mut draft = store.acquisition(project, "a slide", &info).unwrap();
    let writer = draft.image().unwrap();
    let samples = Samples::from_unit(info.sample, &values);
    writer.write(0, (0, 0), (w, h), &samples).unwrap();
    if raw {
        let format = PixelFormat::Bayer16 {
            pattern,
            significant_bits: 12,
        };
        draft.record().camera = json!({ "format": format });
    }
    let id = draft.commit().unwrap().id.to_string();
    let jobs = JobService::open(&folder.path().join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .projects(store)
        .jobs(jobs)
        .build()
        .unwrap();
    let client = TestClient::start(api.into_thing_server()).await.unwrap();
    Served {
        client,
        project,
        id,
        scratch: roots.scratch,
        _folder: folder,
    }
}

impl Served {
    fn base(&self) -> String {
        format!("/api/v1/zoom/{}/acquisitions/{}", self.project, self.id)
    }

    async fn tile(&self, level: u32, (col, row): (u64, u64), query: &str) -> TestResponse {
        let path = format!("{}/image_files/{level}/{col}_{row}.jpg{query}", self.base());
        self.client.get(&path).await
    }

    async fn status(&self) -> Value {
        self.client
            .get(&format!("{}/status", self.base()))
            .await
            .json()
    }
}

/// `jpeg`'s RGB, and its size.
fn decode(jpeg: &[u8]) -> (Vec<u8>, (usize, usize)) {
    let mut decoder =
        JpegDecoder::new_with_options(std::io::Cursor::new(jpeg), DecoderOptions::default());
    let pixels = decoder.decode().unwrap();
    (pixels, decoder.dimensions().unwrap())
}

/// The mean of each channel of `rgb`.
fn means(rgb: &[u8]) -> [f64; 3] {
    let n = (rgb.len() / 3) as f64;
    [0, 1, 2].map(|c| {
        rgb.iter()
            .skip(c)
            .step_by(3)
            .map(|&v| f64::from(v))
            .sum::<f64>()
            / n
    })
}

/// Whether `response` says where its tile came from: `hit` or `miss`.
fn timed(response: &TestResponse, from: &str) -> bool {
    let timing = response.header("server-timing").unwrap_or("");
    timing.starts_with(&format!("tile;desc=\"{from}\";dur="))
}

#[tokio::test(flavor = "multi_thread")]
async fn tiles_are_served_kept_and_timed_at_every_level_and_edge() {
    let served = serve(false, (1500, 1100)).await;
    let dzi = Dzi {
        width: 1500,
        height: 1100,
    };
    let xml = served
        .client
        .get(&format!("{}/image.dzi", served.base()))
        .await;
    assert_eq!(xml.status, 200);
    assert_eq!(xml.header("content-type"), Some("application/xml"));
    assert_eq!(xml.text(), dzi.descriptor());
    let status = served.status().await;
    assert_eq!(
        (
            &status["size"],
            &status["levels"],
            &status["raw"],
            &status["overview"]
        ),
        (
            &json!([1500, 1100]),
            &json!(12),
            &json!(false),
            &json!("none")
        ),
        "{status}"
    );

    // Made, then kept: the same bytes, the second from the cache.
    let last = dzi.levels() - 1;
    let first = served.tile(last, (0, 0), "").await;
    assert_eq!(first.status, 200);
    assert_eq!(first.header("content-type"), Some("image/jpeg"));
    assert!(timed(&first, "miss"), "{:?}", first.header("server-timing"));
    let again = served.tile(last, (0, 0), "").await;
    assert!(timed(&again, "hit"), "{:?}", again.header("server-timing"));
    assert_eq!(again.body, first.body);
    // In the cache's layout, for the desktop app.
    let image = served
        .scratch
        .join("zoom")
        .join(format!("{}-acquisition-{}-0", served.project, served.id));
    let looks: Vec<_> = std::fs::read_dir(&image).unwrap().flatten().collect();
    assert_eq!(looks.len(), 1);
    assert!(looks[0].path().join(format!("{last}/0_0.jpg")).is_file());
    // Another look has tiles of its own.
    let look = "?black=0.2&white=0.9&gamma=1.8";
    let other = served.tile(last, (0, 0), look).await;
    assert!(timed(&other, "miss"));
    assert_ne!(other.body, first.body);
    assert!(timed(&served.tile(last, (0, 0), look).await, "hit"));

    // Each level's last tile ends at its edge; past it, nothing.
    for level in [0, 1, 4, 9, 10, last] {
        let (cols, rows) = dzi.tiles(level);
        let corner = served.tile(level, (cols - 1, rows - 1), "").await;
        assert_eq!(corner.status, 200, "level {level}");
        let area = dzi.tile(level, (cols - 1, rows - 1)).unwrap();
        let (_, size) = decode(&corner.body);
        assert_eq!(
            size,
            (area.width as usize, area.height as usize),
            "level {level}"
        );
        assert_eq!(served.tile(level, (cols, 0), "").await.status, 404);
        assert_eq!(served.tile(level, (0, rows), "").await.status, 404);
    }
    // In colour: the image's red above its blue.
    let (rgb, _) = decode(
        &served
            .tile(last, (2, 2), "?black=0&white=1&gamma=1")
            .await
            .body,
    );
    let [r, g, b] = means(&rgb);
    assert!(r > g + 40.0 && g > b + 40.0, "{r} {g} {b}");

    // What it refuses.
    assert_eq!(served.tile(last + 1, (0, 0), "").await.status, 404);
    let base = served.base();
    for bad in [
        "image_files/3/a_b.jpg",
        "image_files/3/0_0.png",
        "image_files/x/0_0.jpg",
    ] {
        let status = served.client.get(&format!("{base}/{bad}")).await.status;
        assert!(status == 404 || status == 400, "{bad}: {status}");
    }
    for look in ["?black=0.8&white=0.2", "?gamma=0", "?plane=3"] {
        assert_eq!(served.tile(last, (0, 0), look).await.status, 400, "{look}");
    }
    let nobody = format!(
        "/api/v1/zoom/{}/acquisitions/{}/image.dzi",
        ProjectId::generate(),
        served.id
    );
    assert_eq!(served.client.get(&nobody).await.status, 404);
    let reports = base.replace("/acquisitions/", "/reports/");
    assert_eq!(
        served
            .client
            .get(&format!("{reports}/image.dzi"))
            .await
            .status,
        404
    );
}

async fn invoke(client: &TestClient, action: &str, input: Value) -> Value {
    let started = client
        .post_json(&format!("/api/v1/{action}"), Some(&input))
        .await;
    assert_eq!(started.status, 201, "{action}: {}", started.text());
    let href = started.json()["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let invocation = client.get(&href).await.json();
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Submits job `kind` with `params`, and waits for it to end.
async fn job(client: &TestClient, kind: &str, params: Value) -> Value {
    let submitted = invoke(
        client,
        "jobs/submit",
        json!({"kind": kind, "params": params}),
    )
    .await;
    let href = submitted["output"]["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let job = client.get(&href).await.json();
        if ["completed", "failed", "cancelled"].contains(&job["state"].as_str().unwrap_or("")) {
            return job;
        }
        assert!(Instant::now() < deadline, "{job}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dzi_export_is_what_is_served() {
    let served = serve(false, (900, 700)).await;
    let params = json!({"project": served.project, "record": served.id, "format": "dzi"});
    let exported = job(&served.client, "projects.export", params).await;
    assert_eq!(exported["state"], "completed", "{exported}");
    let folder = format!("/api/v1/{}", exported["results"][0].as_str().unwrap());
    assert!(folder.ends_with(&format!(
        "/acquisitions/{}/exports/{}_dzi",
        served.id, served.id
    )));
    let dzi = Dzi {
        width: 900,
        height: 700,
    };
    let xml = served.client.get(&format!("{folder}/image.dzi")).await;
    assert_eq!(xml.text(), dzi.descriptor());
    for level in 0..dzi.levels() {
        let (cols, rows) = dzi.tiles(level);
        for (col, row) in (0..rows).flat_map(|r| (0..cols).map(move |c| (c, r))) {
            let file = format!("{folder}/image_files/{level}/{col}_{row}.jpg");
            let kept = served.client.get(&file).await;
            assert_eq!(kept.status, 200, "{file}");
            let tile = served.tile(level, (col, row), "").await;
            assert_eq!(kept.body, tile.body, "level {level} ({col}, {row})");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_raw_image_is_served_in_colour_once_its_overview_is_built() {
    let served = serve(true, (1200, 1000)).await;
    let status = served.status().await;
    assert_eq!(
        (&status["raw"], &status["overview"]),
        (&json!(true), &json!("not built"))
    );
    let (dzi, linear) = (
        Dzi {
            width: 1200,
            height: 1000,
        },
        "?black=0&white=1&gamma=1",
    );
    let last = dzi.levels() - 1;
    // At full size, demosaiced: in colour at once.
    let (rgb, _) = decode(&served.tile(last, (1, 1), linear).await.body);
    let [r, g, b] = means(&rgb);
    assert!(r > g + 40.0 && g > b + 40.0, "{r} {g} {b}");
    // Halved, grey until its overview is built, which asking starts; and
    // not kept.
    let early = served.tile(last - 1, (0, 0), linear).await;
    let [r, _, b] = means(&decode(&early.body).0);
    assert!((r - b).abs() < 8.0, "grey: {r} {b}");
    let deadline = Instant::now() + Duration::from_secs(60);
    while served.status().await["overview"] != "ready" {
        assert!(Instant::now() < deadline, "{}", served.status().await);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let coloured = served.tile(last - 1, (0, 0), linear).await;
    assert!(timed(&coloured, "miss"), "the grey tile wasn't kept");
    let [r, g, b] = means(&decode(&coloured.body).0);
    assert!(r > g + 40.0 && g > b + 40.0, "{r} {g} {b}");
    assert!(timed(&served.tile(last - 1, (0, 0), linear).await, "hit"));
    // The coarsest levels too.
    let (rgb, size) = decode(&served.tile(3, (0, 0), linear).await.body);
    assert_eq!(size, (dzi.size(3).0 as usize, dzi.size(3).1 as usize));
    let [r, _, b] = means(&rgb);
    assert!(r > b + 80.0, "{r} {b}");
}
