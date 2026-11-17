//! The soak (P53): the server's release build, the simulator with its
//! scaled sensor, a preview client, and scans and stitches, one after
//! another, for `OSCOPE_SOAK_MINUTES` (2 unless set). The server's memory
//! and handles are sampled all along and checked for leaks, and the
//! preview for dropped frames. `cargo xtask soak` runs it, and so does the
//! nightly workflow. Its leak detector is tested here too.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use oscope_acquire::preview::wire::Header;
use oscope_sim::{HeTissue, Slide};
use serde_json::{Value, json};
use tungstenite::Message;

/// A leak, found by Theil–Sen's estimate of a series' slope (H. Theil,
/// 1950, "A rank-invariant method of linear and polynomial regression
/// analysis"; P. K. Sen, 1968, "Estimates of the regression coefficient
/// based on Kendall's tau", JASA 63:1379–1389): the median of the slopes
/// between every two points. A spike or a cache's sawtooth barely moves
/// it, as steady growth does.
mod detector {
    fn median(mut values: Vec<f64>) -> f64 {
        values.sort_by(f64::total_cmp);
        match values.len() {
            0 => 0.0,
            n if n % 2 == 1 => values[n / 2],
            n => (values[n / 2 - 1] + values[n / 2]) / 2.0,
        }
    }

    /// Theil–Sen's slope of `points`, (x, y).
    pub fn slope(points: &[(f64, f64)]) -> f64 {
        let mut slopes = Vec::new();
        for (k, a) in points.iter().enumerate() {
            for b in &points[k + 1..] {
                if b.0 > a.0 {
                    slopes.push((b.1 - a.1) / (b.0 - a.0));
                }
            }
        }
        median(slopes)
    }

    /// The shortest warm-up, in seconds: caches fill while it lasts.
    pub const WARM_UP: f64 = 300.0;

    /// The steady part of `series`, (seconds, value): after its warm-up,
    /// its first quarter and at least [`WARM_UP`]. None, under 10 samples:
    /// too short to say.
    pub fn steady(series: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let span = series.last().map_or(0.0, |p| p.0);
        let from = (span / 4.0).max(WARM_UP);
        let steady: Vec<(f64, f64)> = series.iter().copied().filter(|p| p.0 >= from).collect();
        if steady.len() < 10 {
            Vec::new()
        } else {
            steady
        }
    }

    /// How fast `series` grows once steady, a second.
    pub fn rate(series: &[(f64, f64)]) -> f64 {
        slope(&steady(series))
    }

    /// Whether `series` leaks: once steady, it grows by more than `floor`,
    /// and by more than `fraction` of its median.
    pub fn leaks(series: &[(f64, f64)], floor: f64, fraction: f64) -> bool {
        let steady = steady(series);
        let (Some(first), Some(last)) = (steady.first(), steady.last()) else {
            return false;
        };
        let growth = slope(&steady) * (last.0 - first.0);
        let level = median(steady.iter().map(|p| p.1).collect());
        growth > floor && growth > fraction * level
    }
}

const MB: f64 = 1_048_576.0;

/// Memory leaks if it grows by more than 32 MB and 5 %.
const MEMORY: (f64, f64) = (32.0 * MB, 0.05);

/// Handles leak if they grow by more than 32 and 10 %.
const HANDLES: (f64, f64) = (32.0, 0.10);

/// The most of the preview's frames that may be dropped.
const DROPPED: f64 = 0.01;

/// A series every 10 s for `minutes`: `level(t)`, with a wobble of up to
/// `noise` either way, the same each time.
fn series(minutes: f64, noise: f64, level: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let mut state = 1_u64;
    let samples = (minutes * 6.0) as usize;
    (0..samples)
        .map(|k| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let unit = (state >> 11) as f64 / (1_u64 << 53) as f64;
            let t = k as f64 * 10.0;
            (t, level(t) + (2.0 * unit - 1.0) * noise)
        })
        .collect()
}

#[test]
fn a_steady_series_is_no_leak_noisy_warming_up_or_a_sawtooth() {
    let (floor, fraction) = MEMORY;
    let flat = series(240.0, 20.0 * MB, |_| 400.0 * MB);
    assert!(!detector::leaks(&flat, floor, fraction));
    // Warming up for its first 20 minutes, then flat.
    let warming = series(240.0, 5.0 * MB, |t| {
        300.0 * MB + 100.0 * MB * (t / 1200.0).min(1.0)
    });
    assert!(!detector::leaks(&warming, floor, fraction));
    // A cache filled and emptied every 15 minutes.
    let sawtooth = series(240.0, 2.0 * MB, |t| {
        400.0 * MB + 50.0 * MB * ((t % 900.0) / 900.0)
    });
    assert!(!detector::leaks(&sawtooth, floor, fraction));
    // Spikes at the very end, a gigabyte each, as a large job's buffers:
    // they would pull a mean of the slopes up, but not their median.
    let mut spiky = flat.clone();
    for sample in spiky.iter_mut().rev().take(5) {
        sample.1 += 1024.0 * MB;
    }
    assert!(!detector::leaks(&spiky, floor, fraction));
    // Past the floor but not 5 %: 45 MB more once warm, on 2 GB.
    let large = series(240.0, 2.0 * MB, |t| 2048.0 * MB + 60.0 * MB * t / 14_400.0);
    assert!(!detector::leaks(&large, floor, fraction));
    // Past 10 % but not the floor: 20 handles more, on 100.
    let (floor, fraction) = HANDLES;
    let few = series(240.0, 0.0, |t| 100.0 + 20.0 * t / 14_400.0);
    assert!(!detector::leaks(&few, floor, fraction));
    // Too short to say: nothing, or 3 minutes, all warm-up.
    assert!(!detector::leaks(&[], floor, fraction));
    let short = series(3.0, 0.0, |t| 100.0 + t);
    assert!(detector::steady(&short).is_empty() && !detector::leaks(&short, floor, fraction));
    // Six minutes: six samples once warm, too few to judge.
    let six = series(6.0, 0.0, |t| 100.0 + t);
    assert!(detector::steady(&six).is_empty() && !detector::leaks(&six, floor, fraction));
}

#[test]
fn a_slow_steady_growth_is_a_leak() {
    let (floor, fraction) = MEMORY;
    // 15 MB an hour, on 400 MB, wobbling by 20 MB.
    let memory = series(240.0, 20.0 * MB, |t| 400.0 * MB + 15.0 * MB * t / 3600.0);
    assert!(detector::leaks(&memory, floor, fraction));
    // A handle a minute, on 300.
    let (floor, fraction) = HANDLES;
    let handles = series(240.0, 3.0, |t| 300.0 + t / 60.0);
    assert!(detector::leaks(&handles, floor, fraction));
    // The slope itself, on a line.
    let line: Vec<(f64, f64)> = (0..10)
        .map(|k| (f64::from(k), 3.0 * f64::from(k) + 1.0))
        .collect();
    assert_eq!(detector::slope(&line), 3.0);
}

/// The soak's configuration: the simulator with its scaled sensor, quiet
/// logs, a small log ring, and power only said.
const CONFIG: &str = r#"version = 2

[storage]
data_root = "data"
scratch_root = "scratch"

[logging]
level = "warn"
ring = 2000

[simulator]
sensor = "quarter"

[power]
dry_run = true
"#;

/// The server, killed when the soak ends however it ends.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One HTTP/1.1 request: its status, and its body as JSON.
fn call(address: &str, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let path = path.split_once("//").map_or(path, |(_, rest)| {
        rest.find('/').map_or("/", |at| &rest[at..])
    });
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut stream = TcpStream::connect(address).unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// Invokes `action` with `input`, and waits for its output.
fn invoke(address: &str, action: &str, input: Value) -> Value {
    let (status, started) = call(address, "POST", &format!("/api/v1/{action}"), Some(&input));
    assert_eq!(status, 201, "{action}: {started}");
    let href = started["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        let (_, invocation) = call(address, "GET", &href, None);
        match invocation["status"].as_str() {
            Some("completed") => return invocation["output"].clone(),
            Some("pending" | "running") => {}
            _ => panic!("{action}: {invocation}"),
        }
        assert!(Instant::now() < deadline, "{action} didn't end");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Runs the job `action` starts, and waits for it to complete.
fn job(address: &str, action: &str, input: Value) -> Value {
    let submitted = invoke(address, action, input);
    let href = submitted["href"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(3600);
    loop {
        let (_, job) = call(address, "GET", &href, None);
        match job["state"].as_str() {
            Some("completed") => return job,
            Some("failed" | "cancelled") => panic!("{action}: {job}"),
            _ => {}
        }
        assert!(Instant::now() < deadline, "{action} didn't end");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The server's memory, in bytes, and its handles: on Linux, its resident
/// memory and open files; on Windows, its private bytes and handles.
#[cfg(target_os = "linux")]
fn usage(pid: u32) -> (f64, f64) {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let rss = status.lines().find_map(|l| l.strip_prefix("VmRSS:"));
    let kb: f64 = rss
        .unwrap()
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .unwrap();
    let handles = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .count();
    (kb * 1024.0, handles as f64)
}

#[cfg(windows)]
fn usage(pid: u32) -> (f64, f64) {
    let script =
        format!("$p = Get-Process -Id {pid}; \"$($p.PrivateMemorySize64) $($p.HandleCount)\"");
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    let mut words = text.split_whitespace().map(|w| w.parse::<f64>().unwrap());
    (words.next().unwrap(), words.next().unwrap())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn usage(_: u32) -> (f64, f64) {
    panic!("the soak samples a server on Windows and Linux only (ADR-0003)")
}

/// A preview client, acknowledging each frame, until `stop`: the frames it
/// had.
fn previews(address: &str, stop: &AtomicBool) -> u64 {
    let url = format!("ws://{address}/api/v1/preview/stream");
    let (mut socket, _) = tungstenite::connect(url).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_ref() {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
    }
    let mut params = json!({"width": 342, "height": 342, "quality": 70, "ack": 0});
    socket.send(Message::text(params.to_string())).unwrap();
    let mut frames = 0;
    while !stop.load(Ordering::Relaxed) {
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                let read = Header::read(&bytes).unwrap();
                frames += 1;
                params["ack"] = json!(read.header.sequence + 1);
                socket.send(Message::text(params.to_string())).unwrap();
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => panic!("the preview: {e}"),
        }
    }
    let _ = socket.close(None);
    frames
}

/// The instrument, ready: homed, over tissue in focus, streaming, exposed.
/// Where its tissue is.
fn prepare(address: &str) -> (f64, f64) {
    invoke(address, "stage/home", json!({}));
    let island = HeTissue::new(1, Slide::COVERSLIP).islands()[0];
    let z = invoke(
        address,
        "simulation/in_focus",
        json!({"x": island.x, "y": island.y}),
    );
    let to = json!({"x": island.x, "y": island.y, "z": z});
    invoke(address, "stage/move_to", to);
    invoke(address, "camera/start_streaming", json!({}));
    job(address, "exposure/auto", json!({}));
    (island.x, island.y)
}

/// One cycle: a project, a scan of 800 × 600 µm over the tissue, its
/// stitch, and the project deleted, so that the disk stays as it was.
fn cycle(address: &str, tissue: (f64, f64), k: usize) {
    let made = invoke(
        address,
        "projects/create",
        json!({"name": format!("soak {k}")}),
    );
    let project = made["id"].as_str().unwrap().to_owned();
    let (x, y) = (tissue.0 - 400.0, tissue.1 - 300.0);
    let rect = json!({"origin": {"x": x, "y": y}, "size": {"width": 800.0, "height": 600.0}});
    let request = json!({"pattern": "snake", "region": {"rect": rect}});
    let scan = json!({"request": request, "project": project, "name": "a soak's scan"});
    let scanned = job(address, "scan/run", scan);
    let result = scanned["results"][0].as_str().unwrap();
    let acquisition = result.rsplit('/').next().unwrap();
    let params = json!({"project": project, "acquisition": acquisition});
    job(
        address,
        "jobs/submit",
        json!({"kind": "stitch.run", "params": params}),
    );
    invoke(address, "projects/delete", json!({"id": project}));
}

/// How long the soak runs, in minutes.
fn minutes() -> f64 {
    let set = std::env::var("OSCOPE_SOAK_MINUTES").ok();
    set.and_then(|m| m.parse().ok()).unwrap_or(2.0)
}

#[test]
#[ignore = "the soak: cargo xtask soak, or OSCOPE_SOAK_MINUTES and --ignored"]
fn soak() {
    let minutes = minutes();
    let folder = tempfile::tempdir().unwrap();
    let config = folder.path().join("oscope.toml");
    std::fs::write(&config, CONFIG).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["--config", config.to_str().unwrap(), "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    BufReader::new(stdout).read_line(&mut line).unwrap();
    let served = line.trim().rsplit_once("serving on http://");
    let address = served.map(|(_, a)| a.trim_end_matches('/').to_owned());
    let address = Arc::new(address.unwrap_or_else(|| panic!("the serving line: {line:?}")));
    let pid = server.0.id();
    let tissue = prepare(&address);
    // The preview client, and the sampler: every 2 s, or for longer soaks,
    // 360 samples in all.
    let stop = Arc::new(AtomicBool::new(false));
    let client = {
        let (address, stop) = (Arc::clone(&address), Arc::clone(&stop));
        std::thread::spawn(move || previews(&address, &stop))
    };
    let every = Duration::from_secs_f64((minutes * 60.0 / 360.0).max(2.0));
    let started = Instant::now();
    let sampler = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut samples = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                let (memory, handles) = usage(pid);
                samples.push((started.elapsed().as_secs_f64(), memory, handles));
                let next = Instant::now() + every;
                while Instant::now() < next && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            samples
        })
    };
    // Scans and stitches, until the time is up.
    let deadline = started + Duration::from_secs_f64(minutes * 60.0);
    let mut cycles = 0;
    while Instant::now() < deadline {
        cycle(&address, tissue, cycles);
        cycles += 1;
    }
    // The preview's session, while it is open; then the rest stops.
    let (_, sessions) = call(&address, "GET", "/api/v1/preview/sessions", None);
    stop.store(true, Ordering::Relaxed);
    let frames = client.join().unwrap();
    let samples = sampler.join().unwrap();
    drop(server);
    // The verdicts, and the report.
    let memory: Vec<(f64, f64)> = samples.iter().map(|s| (s.0, s.1)).collect();
    let handles: Vec<(f64, f64)> = samples.iter().map(|s| (s.0, s.2)).collect();
    let judged = !detector::steady(&memory).is_empty();
    let leaks = (
        detector::leaks(&memory, MEMORY.0, MEMORY.1),
        detector::leaks(&handles, HANDLES.0, HANDLES.1),
    );
    let session = &sessions[0];
    let (sent, dropped) = (
        session["sent"].as_f64().unwrap_or(0.0),
        session["dropped"].as_f64().unwrap_or(0.0),
    );
    let rates = (
        detector::rate(&memory) * 3600.0 / MB,
        detector::rate(&handles) * 3600.0,
    );
    let report = json!({
        "minutes": minutes, "cycles": cycles, "frames": frames, "sent": sent, "dropped": dropped,
        "leaks_judged": judged, "memory_leaks": leaks.0, "handles_leak": leaks.1,
        "memory_mb_per_hour": rates.0, "handles_per_hour": rates.1,
        "samples": samples.iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>(),
    });
    let out = std::env::var_os("OSCOPE_SOAK_REPORT").map(std::path::PathBuf::from);
    if let Some(out) = out.as_deref().map(Path::new) {
        std::fs::create_dir_all(out).unwrap();
        std::fs::write(out.join("soak.json"), format!("{report:#}\n")).unwrap();
    }
    let (memory_rate, handle_rate) = rates;
    println!(
        "soak: {minutes} min, {cycles} scans and stitches, {frames} previews ({dropped} of {sent} \
         dropped); once warm, memory {memory_rate:+.1} MB an hour, handles {handle_rate:+.1} an hour"
    );
    if !judged {
        println!("soak: too short to judge leaks, which it does after 5 minutes' warm-up");
    }
    assert!(cycles >= 1 && frames > 0 && sent > 0.0, "{report}");
    assert!(!leaks.0, "memory leaks: {memory_rate:.1} MB an hour");
    assert!(!leaks.1, "handles leak: {handle_rate:.1} an hour");
    let most = DROPPED * sent;
    assert!(dropped <= most, "{dropped} of {sent} previews dropped");
}
