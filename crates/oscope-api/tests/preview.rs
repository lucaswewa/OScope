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
use oscope_acquire::preview::wire::{FRAME, Header, JPEG};
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

/// The next preview: its header and its JPEG. Panics after 60 s.
async fn next(socket: &mut Socket) -> (Header, Vec<u8>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let message = tokio::time::timeout_at(deadline.into(), socket.next())
            .await
            .expect("a preview within 60 s")
            .expect("the stream stays open")
            .unwrap();
        if let Message::Binary(bytes) = message {
            let (header, jpeg) = Header::read(&bytes).unwrap();
            assert_eq!((header.kind, header.codec), (FRAME, JPEG));
            return (header, jpeg.to_vec());
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
            Message::Binary(bytes) => assert_eq!(Header::read(&bytes).unwrap().0.quality, 33),
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
    // that shows one every 250 ms, and acknowledges it. It gets the newest,
    // not the next in a queue: without acknowledging, the sockets' buffers
    // would hold a backlog first.
    let folder = tempfile::tempdir().unwrap();
    let devices = devices(folder.path(), fakes()).await;
    let (address, _stop) = serve(folder.path(), Arc::clone(&devices)).await;
    let mut params = json!({"width": 640, "height": 480, "quality": 95});
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
        read.push((header.frame.0, bus.published()));
        tokio::time::sleep(Duration::from_millis(250)).await;
        params["ack"] = json!(header.sequence);
        socket
            .send(Message::text(params.to_string()))
            .await
            .unwrap();
    }
    // What the sockets held before its first acknowledgement is soon read.
    // Then each preview is of one of the last few frames published, and
    // those between are skipped.
    for pair in read[6..].windows(2) {
        let ((first, _), (second, published)) = (pair[0], pair[1]);
        assert!(second > first + 1, "{read:?}");
        assert!(published - second <= 10, "{read:?}");
    }
    let (_, sessions) = http(address, "GET", "/api/v1/preview/sessions", "").await;
    assert!(sessions[0]["dropped"].as_u64().unwrap() > 0, "{sessions}");
    // Kept waiting, the session took its quality down.
    assert!(sessions[0]["quality"].as_u64().unwrap() < 95, "{sessions}");
}

#[tokio::test(flavor = "multi_thread")]
async fn jogging_moves_the_image_at_the_stage_s_speed() {
    // The simulator on the scaled-down sensor, 0.96 µm a pixel at 10×,
    // over the H&E slide. Homed and placed at 50 times real time, then
    // jogged towards +x at 540 µm/s, in real time.
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
    simulator.clock.set_rate(Rate::RealTime);
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
    let speed = 540.0;
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
