//! The `preview` Thing's stream, over a real socket: a client's parameters
//! shape its previews, a slow client gets the newest frames, and jogging
//! the simulated stage moves the image at the stage's speed, the way the
//! camera's mounting says.

use std::io::Cursor;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use oscope_acquire::preview::wire::{FRAME, Header, JPEG, MASK, STATISTICS};
use oscope_acquire::stats::{LiveStats, Mask};
use oscope_api::devices::{Devices, DevicesConfig};
use oscope_api::{Api, Roots};
use oscope_core::{LightState, PixelRect, Um, Xyz};
use oscope_sim::Rate;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// The API with `devices`, served on a loopback port of its own until the
/// sender is used.
async fn serve(
    folder: &Path,
    devices: Arc<Devices>,
) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let roots = Roots {
        data: folder.join("data"),
        scratch: folder.join("scratch"),
    };
    std::fs::create_dir_all(&roots.data).unwrap();
    let api = Api::builder(&roots).devices(devices).build().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(api.serve(listener, async {
        let _ = stopped.await;
    }));
    (address, stop)
}

/// The devices of `config`, with imports' paths from `folder`.
async fn devices(folder: &Path, config: DevicesConfig) -> Arc<Devices> {
    Arc::new(Devices::from_config(&config, folder).await.unwrap())
}

/// One HTTP/1.1 request: the status, and the body as JSON.
async fn http(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    let status = answer[9..12].parse().unwrap();
    let json = answer
        .split_once("\r\n\r\n")
        .and_then(|(_, b)| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    (status, json)
}

/// The preview stream, asking for `params`, through a socket with
/// `buffer` bytes to receive into if given, like a slow link's.
async fn stream(address: SocketAddr, params: Value, buffer: Option<u32>) -> Socket {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    if let Some(bytes) = buffer {
        socket.set_recv_buffer_size(bytes).unwrap();
    }
    let tcp = socket.connect(address).await.unwrap();
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tokio_tungstenite::client_async(url, MaybeTlsStream::Plain(tcp))
        .await
        .unwrap();
    socket
        .send(Message::text(params.to_string()))
        .await
        .unwrap();
    socket
}

/// A preview read: its header, its JPEG, and its blocks.
type Read = (Header, Vec<u8>, Vec<(u8, Vec<u8>)>);

/// The next preview: its header and its JPEG. Panics after 60 s.
async fn next(socket: &mut Socket) -> (Header, Vec<u8>) {
    let (header, jpeg, _) = next_with_blocks(socket).await;
    (header, jpeg)
}

/// The next preview, with its blocks. Panics after 60 s.
async fn next_with_blocks(socket: &mut Socket) -> Read {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let message = tokio::time::timeout_at(deadline.into(), socket.next())
            .await
            .expect("a preview within 60 s")
            .expect("the stream stays open")
            .unwrap();
        if let Message::Binary(bytes) = message {
            let read = Header::read(&bytes).unwrap();
            assert_eq!((read.header.kind, read.header.codec), (FRAME, JPEG));
            let blocks = read.blocks.iter().map(|(k, b)| (*k, b.to_vec())).collect();
            return (read.header, read.jpeg.to_vec(), blocks);
        }
    }
}

/// The JPEG as grey, from 0 to 255, with its size.
fn grey(jpeg: &[u8]) -> (Vec<f32>, usize, usize) {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(jpeg), options);
    let rgb = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    let grey = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.iter().map(|&v| f32::from(v)).sum::<f32>() / 3.0)
        .collect();
    (grey, usize::from(info.width), usize::from(info.height))
}

/// The fakes, streaming at 100 frames a second, 640 × 480.
fn fakes() -> DevicesConfig {
    let fake = || "fake".to_owned();
    DevicesConfig {
        camera: fake(),
        stage: fake(),
        illumination: fake(),
        objective: fake(),
        ..DevicesConfig::default()
    }
}

/// Waits until `check` holds, for up to 30 s.
async fn wait_for(mut check: impl AsyncFnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !check().await {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_s_parameters_shape_its_previews() {
    let folder = tempfile::tempdir().unwrap();
    let (address, _stop) = serve(folder.path(), devices(folder.path(), fakes()).await).await;
    let roi = json!({"x": 100, "y": 60, "width": 200, "height": 100});
    let params = json!({"width": 320, "height": 320, "quality": 50, "roi": roi});
    let mut socket = stream(address, params, None).await;
    assert_eq!(
        http(address, "POST", "/api/v1/camera/start_streaming", "{}")
            .await
            .0,
        201
    );

    // The ROI, never enlarged, at the quality asked for; a JPEG.
    let (header, jpeg) = loop {
        let (header, jpeg) = next(&mut socket).await;
        if header.quality == 50 {
            break (header, jpeg);
        }
    };
    assert_eq!(header.roi, PixelRect::new(100, 60, 200, 100));
    assert_eq!((header.width, header.height), (200, 100));
    assert_eq!(grey(&jpeg).1, 200);
    assert!(header.exposure_start <= header.exposure_end && header.exposure_end <= header.ready);

    // A new message replaces the last: the whole frame, at the defaults'
    // quality.
    socket
        .send(Message::text(
            json!({"width": 64, "height": 64}).to_string(),
        ))
        .await
        .unwrap();
    let header = loop {
        let (header, _) = next(&mut socket).await;
        if header.width == 64 {
            break header;
        }
    };
    assert_eq!(header.roi, PixelRect::new(0, 0, 640, 480));
    assert_eq!((header.height, header.quality), (48, 80));

    // An ROI that misses the frame shows the whole frame; and at most
    // `max_fps` frames come a second.
    let roi = json!({"x": 5000, "y": 0, "width": 100, "height": 100});
    let params = json!({"width": 64, "height": 64, "quality": 33, "max_fps": 5.0, "roi": roi});
    socket
        .send(Message::text(params.to_string()))
        .await
        .unwrap();
    let mut slowed = Vec::new();
    while slowed.len() < 6 {
        let (header, _) = next(&mut socket).await;
        if header.quality == 33 {
            assert_eq!(header.roi, PixelRect::new(0, 0, 640, 480));
            slowed.push(header.ready);
        }
    }
    let took = slowed[5].saturating_since(slowed[0]);
    assert!(took >= Duration::from_millis(900), "5 frames in {took:?}");

    // Parameters it can't read are answered, and change nothing.
    socket
        .send(Message::text(r#"{"quality": "high"}"#))
        .await
        .unwrap();
    loop {
        match socket.next().await.unwrap().unwrap() {
            Message::Text(text) => {
                assert!(text.contains("error"), "{text}");
                break;
            }
            Message::Binary(bytes) => assert_eq!(Header::read(&bytes).unwrap().header.quality, 33),
            _ => {}
        }
    }

    // The session says how it's doing, and goes when the client does.
    let (status, sessions) = http(address, "GET", "/api/v1/preview/sessions", "").await;
    assert_eq!(status, 200);
    let session = &sessions[0];
    assert!(session["sent"].as_u64().unwrap() > 0, "{sessions}");
    assert!(session["fps"].as_f64().unwrap() > 0.0, "{sessions}");
    assert!(session["bytes_per_s"].as_f64().unwrap() > 0.0, "{sessions}");
    assert_eq!(session["width"], 64);
    socket.close(None).await.unwrap();
    wait_for(
        async || http(address, "GET", "/api/v1/preview/sessions", "").await.1 == json!([]),
        "the session ends with its socket",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_client_gets_the_newest_frames() {
    // The fake camera's frames, as acquisition publishes them, to a client
    // that shows each preview until the camera has published six more,
    // acknowledging from its first message: slower than the camera and the
    // session alike, on any machine. It gets the newest frame, not the next
    // in a queue: without acknowledging, the sockets' buffers would hold a
    // backlog first.
    let folder = tempfile::tempdir().unwrap();
    let devices = devices(folder.path(), fakes()).await;
    let (address, _stop) = serve(folder.path(), Arc::clone(&devices)).await;
    let mut params = json!({"width": 320, "height": 240, "quality": 95, "ack": 0});
    let mut socket = stream(address, params.clone(), None).await;
    assert_eq!(
        http(address, "POST", "/api/v1/camera/start_streaming", "{}")
            .await
            .0,
        201
    );
    let bus = devices.acquisition.bus();
    let mut read = Vec::new();
    for _ in 0..10 {
        let (header, _) = next(&mut socket).await;
        let published = bus.published();
        read.push((header.frame.0, published));
        wait_for(async || bus.published() >= published + 6, "six more frames").await;
        params["ack"] = json!(header.sequence + 1);
        socket
            .send(Message::text(params.to_string()))
            .await
            .unwrap();
    }
    // Paced from its first message, two ahead: each preview was sent once
    // the one two before it was shown, so its frame is at least as new as
    // the newest published then. Frames between them were skipped.
    for k in 2..read.len() {
        let ((frame, _), (_, published_then)) = (read[k], read[k - 2]);
        assert!(frame + 1 >= published_then, "{read:?}");
    }
    let skipped: u64 = read.windows(2).map(|w| w[1].0 - w[0].0 - 1).sum();
    assert!(skipped > 0, "{read:?}");
    let (_, sessions) = http(address, "GET", "/api/v1/preview/sessions", "").await;
    assert!(sessions[0]["dropped"].as_u64().unwrap() > 0, "{sessions}");
    // Kept waiting while newer frames came, the session took its quality
    // down.
    assert!(sessions[0]["quality"].as_u64().unwrap() < 95, "{sessions}");
}

#[tokio::test(flavor = "multi_thread")]
async fn statistics_come_with_the_previews_ten_times_a_second_at_most() {
    // The fake camera's rows step through every level, (row + frame) % 256,
    // so a row or two of each frame is at full scale.
    let folder = tempfile::tempdir().unwrap();
    let (address, _stop) = serve(folder.path(), devices(folder.path(), fakes()).await).await;
    let mut params = json!({"width": 320, "height": 240, "ack": 0});
    let mut socket = stream(address, params.clone(), None).await;
    assert_eq!(
        http(address, "POST", "/api/v1/camera/start_streaming", "{}")
            .await
            .0,
        201
    );
    let (header, _, blocks) = next_with_blocks(&mut socket).await;
    assert!(blocks.is_empty(), "statistics nobody asked for");
    params["ack"] = json!(header.sequence + 1);
    params["max_fps"] = json!(1000.0);
    params["stats"] = json!({
        "profile": {"from": [0.0, 0.0], "to": [0.0, 100.0]},
        "bins": 64
    });
    socket
        .send(Message::text(params.to_string()))
        .await
        .unwrap();

    // After each message with statistics, the previews switch between a
    // slow one, full size at quality 100 with the statistics of every
    // sample, and a quick one, tiny, without: one takes far longer to make
    // than the next, so the 100 ms must run from one's sending, not from
    // its measuring.
    let whole = json!({"x": 0, "y": 0, "width": 640, "height": 480});
    let (mut measured, mut masks) = (Vec::new(), 0);
    while measured.len() < 8 {
        let (header, _, blocks) = next_with_blocks(&mut socket).await;
        params["ack"] = json!(header.sequence + 1);
        let statistics = blocks.iter().find(|(kind, _)| *kind == STATISTICS);
        if statistics.is_some() {
            let slow = measured.len() % 2 == 0;
            let (width, height, quality) = if slow { (640, 480, 100) } else { (16, 12, 10) };
            (params["width"], params["height"]) = (json!(width), json!(height));
            params["quality"] = json!(quality);
            params["stats"]["roi"] = if slow { whole.clone() } else { Value::Null };
        }
        socket
            .send(Message::text(params.to_string()))
            .await
            .unwrap();
        let Some((_, json)) = statistics else {
            continue;
        };
        let stats: LiveStats = serde_json::from_slice(json).unwrap();
        let id = header.frame.0;
        assert_eq!(stats.frame, id);
        assert_eq!((stats.full_scale, stats.histogram.len()), (255, 3));
        for (bins, counted) in stats.histogram.iter().zip(&stats.counted) {
            assert_eq!((bins.len(), bins.iter().sum::<u64>()), (64, *counted));
        }
        if let Some(roi) = stats.roi {
            let counts = vec![76_800, 153_600, 76_800];
            assert_eq!(
                (roi.rect, roi.count),
                (PixelRect::new(0, 0, 640, 480), counts)
            );
        }
        // A cell every 2 rows down: its red is its first row's level.
        let profile = stats.profile.unwrap();
        assert_eq!(profile.samples.len(), 51);
        for (i, sample) in profile.samples.iter().enumerate() {
            let red = sample.as_ref().unwrap()[0];
            assert_eq!(
                red,
                ((2 * i as u64 + id) % 256) as f64,
                "frame {id}, sample {i}"
            );
        }
        // The rows at full scale, at the preview's size, every pixel across.
        let (width, height) = (header.width, header.height);
        let full: Vec<u32> = (0..480_u32)
            .filter(|&y| (u64::from(y) + id) % 256 == 255)
            .map(|y| y * height / 480)
            .collect();
        let mask = blocks.iter().find(|(kind, _)| *kind == MASK);
        assert_eq!(mask.is_some(), !full.is_empty(), "frame {id}");
        if let Some((_, bytes)) = mask {
            let mask = Mask::from_bytes(bytes).unwrap();
            assert_eq!((mask.width, mask.height), (width, height));
            for y in 0..height {
                let row = (0..width).filter(|&x| mask.get(x, y)).count() as u32;
                let expected = if full.contains(&y) { width } else { 0 };
                assert_eq!(row, expected, "frame {id}, row {y}");
            }
            masks += 1;
        }
        measured.push(header.ready);
    }
    assert!(masks > 0);
    // At most 10 a second: each ready 100 ms or more after the last.
    for pair in measured.windows(2) {
        let apart = pair[1].saturating_since(pair[0]);
        assert!(apart >= Duration::from_millis(100), "{apart:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn jogging_moves_the_image_at_the_stage_s_speed() {
    // The simulator on the scaled-down sensor, 0.96 µm a pixel at 10×,
    // over the H&E slide. Homed and placed at 50 times real time, then
    // jogged towards +x at 200 µm/s at a tenth of real time, so that even a
    // debug build on CI renders each frame in time: about 14 preview pixels
    // a frame, within the search's ±80. Every time is the simulator's.
    let folder = tempfile::tempdir().unwrap();
    let mut config = DevicesConfig::default();
    config.simulator.sensor = "quarter".to_owned();
    config.simulator.focus = 1_000.0;
    config.simulator.rate = 50.0;
    let devices = devices(folder.path(), config).await;
    let simulator = Arc::clone(devices.simulator.as_ref().unwrap());
    devices.stage.home().await.unwrap();
    let start = Xyz::new(Um(40_000.0), Um(12_000.0), Um(1_000.0));
    devices.stage.move_to(start, None).await.unwrap();
    simulator.clock.set_rate(Rate::Scaled(0.1));
    let light = LightState {
        channel: "transmitted".to_owned(),
        on: true,
        intensity: 0.6,
    };
    devices.lights.set(light).await.unwrap();
    let (address, _stop) = serve(folder.path(), Arc::clone(&devices)).await;
    let (_, pixel) = http(address, "GET", "/api/v1/objective/pixel_at_sample", "").await;
    let pixel = pixel.as_f64().unwrap();

    // Half the sensor's size each way: the superpixel, unbinned.
    let params = json!({"width": 684, "height": 456, "quality": 90});
    let mut socket = stream(address, params, None).await;
    assert_eq!(
        http(address, "POST", "/api/v1/camera/start_streaming", "{}")
            .await
            .0,
        201
    );
    next(&mut socket).await;
    let speed = 200.0;
    let jog = json!({"axis": "x", "towards_max": true, "speed": speed}).to_string();
    assert_eq!(
        http(address, "POST", "/api/v1/stage/jog", &jog).await.0,
        201
    );
    let jogging = devices.clock.now() + Duration::from_millis(100);
    let mut previews = Vec::new();
    while previews.len() < 10 {
        let (header, jpeg) = next(&mut socket).await;
        if header.exposure_start >= jogging {
            previews.push((header, grey(&jpeg)));
        }
    }
    assert_eq!(
        http(address, "POST", "/api/v1/stage/halt", "{}").await.0,
        201
    );

    // Each pair's shift, from the frames' own times: the image moves
    // towards -x (the default mounting: neither turned nor mirrored), at the
    // stage's speed over the preview's pixel.
    let (header, _) = &previews[0];
    let preview_pixel = pixel * f64::from(header.roi.width) / f64::from(header.width);
    let expected = -speed / preview_pixel;
    let (mut moved, mut took, mut sideways) = (0.0, 0.0, 0.0_f64);
    for pair in previews.windows(2) {
        let ((a, (first, width, height)), (b, (second, ..))) = (&pair[0], &pair[1]);
        let (dx, dy) = shift(first, second, *width, *height);
        moved += dx;
        sideways = sideways.max(dy.abs());
        took += b
            .exposure_start
            .saturating_since(a.exposure_start)
            .as_secs_f64();
    }
    let rate = moved / took;
    println!("{rate:.1} px/s, expected {expected:.1}; the most sideways {sideways}");
    assert!(
        (rate / expected - 1.0).abs() < 0.05,
        "{rate:.1} px/s, expected {expected:.1}"
    );
    assert!(sideways <= 1.0, "{sideways}");
}

/// How far `second`'s content lies from `first`'s, in pixels: the shift,
/// within ±80 along x and ±6 along y, where they differ least, refined
/// along x to a fraction by a parabola through the three around it.
fn shift(first: &[f32], second: &[f32], width: usize, height: usize) -> (f64, f64) {
    const X: isize = 80;
    const Y: isize = 6;
    let difference = |dx: isize, dy: isize| {
        let mut sum = 0.0;
        let mut count = 0;
        for y in (Y as usize..height - Y as usize).step_by(3) {
            for x in (X as usize..width - X as usize).step_by(3) {
                let (u, v) = ((x as isize + dx) as usize, (y as isize + dy) as usize);
                sum += (first[y * width + x] - second[v * width + u]).abs();
                count += 1;
            }
        }
        f64::from(sum) / f64::from(count)
    };
    let mut best = (f64::INFINITY, 0, 0);
    for dy in -Y..=Y {
        for dx in -X..=X {
            let d = difference(dx, dy);
            if d < best.0 {
                best = (d, dx, dy);
            }
        }
    }
    let (centre, dx, dy) = best;
    assert!(
        dx.abs() < X && dy.abs() < Y,
        "no match within the search: ({dx}, {dy})"
    );
    let (left, right) = (difference(dx - 1, dy), difference(dx + 1, dy));
    let curve = left - 2.0 * centre + right;
    let fraction = if curve > 0.0 {
        (left - right) / (2.0 * curve)
    } else {
        0.0
    };
    // `second` at x + dx matches `first` at x: its content moved by dx.
    (dx as f64 + fraction, dy as f64)
}

/// Invokes `action` with `input`, and waits for it to end: its invocation.
async fn invoke(address: SocketAddr, action: &str, input: Value) -> Value {
    let path = format!("/api/v1/preview/{action}");
    let (status, started) = http(address, "POST", &path, &input.to_string()).await;
    assert_eq!(status, 201, "{action}: {started}");
    let href = started["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (_, invocation) = http(address, "GET", &href, "").await;
        if !["pending", "running"].contains(&invocation["status"].as_str().unwrap_or("")) {
            return invocation;
        }
        assert!(Instant::now() < deadline, "{action} didn't end");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A live recipe of stock nodes, from the image to `last`.
fn recipe(nodes: Value, edges: Value, last: &str) -> Value {
    json!({
        "format": 1,
        "inputs": [{ "name": "image", "type": "image" }, { "name": "region", "type": "json" }],
        "nodes": nodes,
        "edges": edges,
        "outputs": [{ "name": "image", "from": format!("{last}.image") }]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_recipe_runs_on_each_preview_once_checked() {
    let folder = tempfile::tempdir().unwrap();
    let (address, _stop) = serve(folder.path(), devices(folder.path(), fakes()).await).await;
    let mut socket = stream(address, json!({ "width": 320, "height": 240 }), None).await;
    assert_eq!(
        http(address, "POST", "/api/v1/camera/start_streaming", "{}")
            .await
            .0,
        201
    );
    let mean = |jpeg: &[u8]| {
        let (grey, ..) = grey(jpeg);
        grey.iter().sum::<f32>() / grey.len() as f32
    };
    let before = mean(&next(&mut socket).await.1);
    // The budget is for release builds: the tests' are unoptimised, and may
    // have two cores, shared with the other tests (P38's two-core runs found
    // a recipe taking 1.1 s). Recipes are timed at the default viewport's
    // size: the stream's here.
    let budget = http(address, "PUT", "/api/v1/preview/recipe_budget_ms", "10000").await;
    assert!(budget.0 < 300, "{budget:?}");
    let defaults = json!({ "width": 320, "height": 240, "quality": 80, "max_fps": 30.0 });
    let defaults = http(
        address,
        "PUT",
        "/api/v1/preview/defaults",
        &defaults.to_string(),
    )
    .await;
    assert!(defaults.0 < 300, "{defaults:?}");

    // Levels, then a table inverting them: the previews come inverted.
    let inverted = recipe(
        json!([
            { "id": "levels", "node": "display.levels" },
            { "id": "invert", "node": "display.lut", "params": { "table": [1.0, 0.0] } }
        ]),
        json!([
            { "from": "input.image", "to": "levels.image" },
            { "from": "levels.image", "to": "invert.image" }
        ]),
        "invert",
    );
    let set = invoke(address, "set_live_recipe", json!({ "recipe": inverted })).await;
    assert_eq!(set["status"], "completed", "{set}");
    let check = &set["output"];
    assert_eq!(
        (check["steps"].clone(), check["width"].clone()),
        (json!(["levels", "invert"]), json!(320))
    );
    assert!(check["time_ms"].as_f64().unwrap() <= check["budget_ms"].as_f64().unwrap());
    // Previews made before it may still be on their way.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let after = mean(&next(&mut socket).await.1);
        if (before + after - 255.0).abs() < 8.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{before} and {after} aren't inverses"
        );
    }
    let sessions = http(address, "GET", "/api/v1/preview/sessions", "").await.1;
    assert!(sessions[0]["recipe_ms"].is_number(), "{sessions}");
    assert_eq!(
        http(address, "GET", "/api/v1/preview/live_recipe", "")
            .await
            .1["nodes"][1]["id"],
        "invert"
    );

    // Refused, saying why: a recipe that changes the size, one over its
    // budget, and one reading a reference that isn't there.
    let binned = recipe(
        json!([{ "id": "bin", "node": "resize.bin", "params": { "factor": 2 } }]),
        json!([{ "from": "input.image", "to": "bin.image" }]),
        "bin",
    );
    let flat = recipe(
        json!([
            { "id": "flat", "node": "io.reference", "params": { "name": "flat" } },
            { "id": "even", "node": "correction.flat_field" },
            { "id": "show", "node": "display.false_colour", "params": { "map": "spectrum" } }
        ]),
        json!([
            { "from": "input.image", "to": "even.image" },
            { "from": "flat.image", "to": "even.flat" },
            { "from": "input.region", "to": "even.region" },
            { "from": "even.image", "to": "show.image" }
        ]),
        "show",
    );
    let refusal = async |recipe: &Value| {
        let refused = invoke(address, "set_live_recipe", json!({ "recipe": recipe })).await;
        assert_eq!(refused["status"], "error", "{refused}");
        refused["error"].to_string()
    };
    assert!(
        refusal(&binned)
            .await
            .contains("a live recipe keeps the size")
    );
    assert!(
        refusal(&flat)
            .await
            .contains("there is no reference `flat`")
    );
    let budget = http(
        address,
        "PUT",
        "/api/v1/preview/recipe_budget_ms",
        "0.000001",
    )
    .await;
    assert!(budget.0 < 300, "{budget:?}");
    assert!(refusal(&inverted).await.contains("over its budget"));
    http(address, "PUT", "/api/v1/preview/recipe_budget_ms", "10000").await;

    // A flat field captured from the camera, then a recipe that reads it,
    // over the region each preview shows.
    let captured = invoke(
        address,
        "capture_reference",
        json!({ "name": "flat", "frames": 2 }),
    )
    .await;
    assert_eq!(captured["status"], "completed", "{captured}");
    assert_eq!(
        (
            captured["output"]["width"].clone(),
            captured["output"]["frames"].clone()
        ),
        (json!(640), json!(2))
    );
    assert_eq!(
        http(address, "GET", "/api/v1/preview/references", "")
            .await
            .1,
        json!(["flat"])
    );
    let set = invoke(address, "set_live_recipe", json!({ "recipe": flat })).await;
    assert_eq!(set["status"], "completed", "{set}");
    let zoomed = json!({ "width": 160, "height": 120, "roi": { "x": 320, "y": 240, "width": 320, "height": 240 } });
    socket
        .send(Message::text(zoomed.to_string()))
        .await
        .unwrap();
    let header = loop {
        let (header, _) = next(&mut socket).await;
        if header.width == 160 {
            break header;
        }
    };
    assert_eq!(header.roi, PixelRect::new(320, 240, 320, 240));

    // None: the look again.
    let cleared = invoke(address, "set_live_recipe", json!({ "recipe": null })).await;
    assert_eq!(
        (cleared["status"].clone(), cleared["output"].clone()),
        (json!("completed"), Value::Null)
    );
    assert_eq!(
        http(address, "GET", "/api/v1/preview/live_recipe", "")
            .await
            .1,
        Value::Null
    );
    wait_for(
        async || {
            http(address, "GET", "/api/v1/preview/sessions", "").await.1[0]["recipe_ms"].is_null()
        },
        "a preview made without the recipe",
    )
    .await;
}
