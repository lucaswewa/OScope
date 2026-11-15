//! The `live_mosaic` Thing over HTTP (P38; ADR-0026): a mosaic begun and
//! drawn as the stage roams, its tiles served as JPEGs and announced with
//! the previews of a client that asks, then halted and cleared; and each
//! tile a scan takes, drawn as it is taken.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::{Header, TILES};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_jobs::{JobService, JobsConfig};
use oscope_sim::{HeTissue, Slide};
use oscope_store::{Scratch, ScratchPolicy, Store};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The simulator ten times as fast as real time on the quarter sensor,
/// with the projects, scratch space and jobs, served on a loopback port of
/// its own until the sender is used.
async fn serve(folder: &Path) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let mut config = DevicesConfig::default();
    let sim = &mut config.simulator;
    (sim.sensor, sim.focus, sim.rate) = ("quarter".to_owned(), 1_000.0, 10.0);
    let devices = Arc::new(Devices::from_config(&config, folder).await.unwrap());
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let store = Store::open(&roots.data, &roots.data.join("index.sqlite")).unwrap();
    let policy = ScratchPolicy {
        reserve_bytes: 0,
        low_bytes: 0,
    };
    let jobs = JobService::open(&folder.join("jobs.sqlite"), JobsConfig::default()).unwrap();
    let api = Api::builder(&roots)
        .devices(devices)
        .projects(store)
        .scratch(Scratch::open(&roots.scratch, policy).unwrap())
        .jobs(jobs)
        .build()
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));
    (address, stop)
}

/// One HTTP/1.1 request: the status, the content type, and the body.
async fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.unwrap();
    let end = answer.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&answer[..end]).to_lowercase();
    let status = head[9..12].parse().unwrap();
    let kind = head
        .lines()
        .find_map(|line| line.strip_prefix("content-type: "))
        .unwrap_or_default()
        .to_owned();
    (status, kind, answer[end + 4..].to_vec())
}

async fn json(address: SocketAddr, method: &str, path: &str, body: &Value) -> (u16, Value) {
    let (status, _, body) = request(address, method, path, &body.to_string()).await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

/// The output of `action` with `input`, which must complete within 300 s.
async fn output(address: SocketAddr, action: &str, input: Value) -> Value {
    let path = format!("/api/v1/{action}");
    let (status, started) = json(address, "POST", &path, &input).await;
    assert_eq!(status, 201, "{action}: {started}");
    let href = started["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let (_, invocation) = json(address, "GET", &href, &Value::Null).await;
        match invocation["status"].as_str() {
            Some("pending" | "running") => {}
            _ => {
                assert_eq!(invocation["status"], "completed", "{action}: {invocation}");
                return invocation["output"].clone();
            }
        }
        assert!(Instant::now() < deadline, "{action}: {invocation}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The live mosaic's state, once `check` passes, within 120 s.
async fn until(address: SocketAddr, what: &str, check: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let (_, state) = json(address, "GET", "/api/v1/live_mosaic/state", &Value::Null).await;
        if check(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "{what}: {state}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Homes the stage, moves over the first island at its focus, and
/// streams: where `x` and `y` are.
async fn over_the_island(address: SocketAddr) -> (f64, f64) {
    output(address, "stage/home", json!({})).await;
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let at = json!({"x": island.x, "y": island.y});
    let z = output(address, "simulation/in_focus", at).await;
    let over = json!({"x": island.x, "y": island.y, "z": z});
    output(address, "stage/move_to", over).await;
    output(address, "camera/start_streaming", json!({})).await;
    (island.x, island.y)
}

/// A JPEG's samples, and its width and height.
fn decode(jpeg: &[u8]) -> (Vec<u8>, (usize, usize)) {
    let options = DecoderOptions::default();
    let mut decoder = JpegDecoder::new_with_options(std::io::Cursor::new(jpeg), options);
    let pixels = decoder.decode().unwrap();
    (pixels, decoder.dimensions().unwrap())
}

/// The preview stream, asking for `params`.
async fn stream(address: SocketAddr, params: Value) -> Socket {
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    socket
        .send(Message::text(params.to_string()))
        .await
        .unwrap();
    socket
}

/// The next preview's blocks. Panics after 60 s.
async fn blocks(socket: &mut Socket) -> Vec<(u8, Vec<u8>)> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let message = tokio::time::timeout_at(deadline.into(), socket.next())
            .await
            .expect("a preview within 60 s")
            .expect("the stream stays open")
            .unwrap();
        if let Message::Binary(bytes) = message {
            let read = Header::read(&bytes).unwrap();
            return read.blocks.iter().map(|(k, b)| (*k, b.to_vec())).collect();
        }
    }
}

/// The next block of the mosaic's tiles changed, as JSON.
async fn changed(socket: &mut Socket) -> Value {
    loop {
        if let Some((_, block)) = blocks(socket).await.into_iter().find(|(k, _)| *k == TILES) {
            return serde_json::from_slice(&block).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_roaming_mosaic_is_drawn_served_announced_halted_and_cleared() {
    let folder = tempfile::tempdir().unwrap();
    let (address, _stop) = serve(folder.path()).await;
    let (x, y) = over_the_island(address).await;
    // 6 × 4 mm about the island, in pixels of 4 µm: 1500 × 1000, in four
    // levels.
    let region = json!({"origin": {"x": x - 3000.0, "y": y - 2000.0}, "size": {"width": 6000.0, "height": 4000.0}});
    let begun = output(
        address,
        "live_mosaic/begin",
        json!({"request": {"region": region, "pixel_um": 4.0}}),
    )
    .await;
    assert_eq!(
        (&begun["drawing"], &begun["roaming"]),
        (&json!(true), &json!(true))
    );
    assert_eq!(
        (&begun["levels"], &begun["canvas"]["width"]),
        (&json!(4), &json!(1500))
    );
    // Drawn where the stage stops: here, and 1.5 mm along.
    let drawn = |n: u64| move |state: &Value| state["counts"]["drawn"].as_u64() >= Some(n);
    until(address, "a frame drawn here", drawn(1)).await;
    output(
        address,
        "stage/move_by",
        json!({"dx": 1500.0, "dy": 0.0, "dz": 0.0}),
    )
    .await;
    until(address, "a frame drawn 1.5 mm along", drawn(2)).await;
    // The tile under the island, a JPEG, its corner nearest the island
    // drawn; one where nothing is drawn, or no level, 404.
    let tile = |level: u32, col: u32, row: u32| format!("/api/v1/mosaic/tiles/{level}/{col}/{row}");
    let (status, kind, jpeg) = request(address, "GET", &tile(0, 2, 1), "").await;
    assert_eq!((status, kind.as_str()), (200, "image/jpeg"));
    let (pixels, size) = decode(&jpeg);
    assert_eq!(size, (256, 256));
    let lit = pixels.iter().filter(|&&p| p > 30).count();
    assert!(
        lit > pixels.len() / 5,
        "{lit} of {} samples lit",
        pixels.len()
    );
    assert_eq!(request(address, "GET", &tile(0, 0, 0), "").await.0, 404);
    assert_eq!(request(address, "GET", &tile(4, 0, 0), "").await.0, 404);
    // A tile at its edge is cut there: 1500 = 5 × 256 + 220.
    let (status, _, edge) = request(address, "GET", &tile(0, 5, 1), "").await;
    assert_eq!((status, decode(&edge).1), (200, (220, 256)));
    // Announced: everything first, then the tiles a frame drawn changes,
    // at every level.
    let mut socket = stream(
        address,
        json!({"mosaic": true, "width": 320, "height": 240}),
    )
    .await;
    let first = changed(&mut socket).await;
    assert_eq!(
        (&first["all"], &first["mosaic"]),
        (&json!(true), &json!(1)),
        "{first}"
    );
    output(
        address,
        "stage/move_by",
        json!({"dx": 0.0, "dy": 1200.0, "dz": 0.0}),
    )
    .await;
    let next = changed(&mut socket).await;
    assert_eq!(next["all"], json!(false), "{next}");
    let tiles = next["tiles"].as_array().unwrap();
    assert!(
        tiles.iter().any(|t| t[0] == 0) && tiles.contains(&json!([3, 0, 0])),
        "{next}"
    );
    // Halted, it draws no more, as frames go by; cleared, it holds nothing.
    let halted = output(address, "live_mosaic/halt", json!({})).await;
    assert_eq!(
        (&halted["drawing"], &halted["roaming"]),
        (&json!(false), &json!(false))
    );
    output(
        address,
        "stage/move_by",
        json!({"dx": -1500.0, "dy": 0.0, "dz": 0.0}),
    )
    .await;
    for _ in 0..5 {
        blocks(&mut socket).await;
    }
    let still = until(address, "the state", |_| true).await;
    assert_eq!(still["counts"]["drawn"], halted["counts"]["drawn"]);
    let cleared = output(address, "live_mosaic/clear", json!({})).await;
    assert_eq!(cleared["tiles"], json!(0));
    assert_eq!(request(address, "GET", &tile(0, 2, 1), "").await.0, 404);
    assert_eq!(changed(&mut socket).await["all"], json!(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn each_tile_a_scan_takes_is_drawn() {
    let folder = tempfile::tempdir().unwrap();
    let (address, _stop) = serve(folder.path()).await;
    let roi = json!({"x": 556, "y": 360, "width": 256, "height": 192});
    let (status, _) = json(address, "PUT", "/api/v1/camera/roi", &roi).await;
    assert!((200..300).contains(&status));
    let (x, y) = over_the_island(address).await;
    let made = output(address, "projects/create", json!({"name": "Scans"})).await;
    // Only the scan's frames: not roaming.
    let region = json!({"origin": {"x": x - 1000.0, "y": y - 1000.0}, "size": {"width": 2000.0, "height": 2000.0}});
    let request = json!({"region": region, "pixel_um": 2.0, "roam": false});
    let begun = output(address, "live_mosaic/begin", json!({"request": request})).await;
    assert_eq!(begun["roaming"], json!(false));
    let square = json!({"rect": {"origin": {"x": x - 300.0, "y": y - 300.0}, "size": {"width": 600.0, "height": 600.0}}});
    let scan = json!({"pattern": "raster", "region": square});
    let planned = output(address, "scan/plan", json!({"request": scan})).await;
    let tiles = planned["plan"]["tiles"].as_array().unwrap().len() as u64;
    assert!(tiles >= 4, "{tiles} tiles");
    let job = output(
        address,
        "scan/run",
        json!({"request": scan, "project": made["id"]}),
    )
    .await;
    let href = job["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let (_, job) = json(address, "GET", &href, &Value::Null).await;
        match job["state"].as_str() {
            Some("completed") => break,
            Some("failed" | "cancelled") => panic!("{job}"),
            _ => assert!(Instant::now() < deadline, "{job}"),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let state = until(address, "each tile drawn", |s| {
        s["counts"]["drawn"].as_u64() >= Some(tiles)
    })
    .await;
    let counts = &state["counts"];
    assert_eq!(counts["drawn"], json!(tiles), "{state}");
    assert_eq!(
        (&counts["dropped"], &counts["failed"]),
        (&json!(0), &json!(0)),
        "{state}"
    );
}
