//! The sidecar contract (ADR-0015), with `oscope-server` run as a process,
//! managed: its ready line and token, stopping when stdin closes, exit code
//! 75 for a restart, and the preview ring.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use oscope_acquire::preview::wire::{FRAME, Header, JPEG};
use oscope_shm::{Read as RingRead, RingReader};
use serde_json::Value;

/// A configuration in `folder`: the fakes for devices, the data root beside
/// it.
fn config(folder: &Path) -> PathBuf {
    let path = folder.join("oscope.toml");
    let text = "[storage]\ndata_root = \"data\"\nscratch_root = \"scratch\"\n\n\
                [devices]\ncamera = \"fake\"\nstage = \"fake\"\n\
                illumination = \"fake\"\nobjective = \"fake\"\n";
    std::fs::write(&path, text).unwrap();
    path
}

/// A server a test started: killed if the test ends while it still runs,
/// so that a failing test leaves nothing behind.
struct Server(Child);

impl std::ops::Deref for Server {
    type Target = Child;

    fn deref(&self) -> &Child {
        &self.0
    }
}

impl std::ops::DerefMut for Server {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// The server, managed, and its ready line, read within 60 s.
fn start(config: &Path) -> (Server, Value) {
    let spawned = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["--managed", "--config"])
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    // Guarded from the start: killed even if no ready line comes.
    let mut child = Server(spawned.unwrap());
    let stdout = child.stdout.take().unwrap();
    let (line, read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut first = String::new();
        let _ = BufReader::new(stdout).read_line(&mut first);
        let _ = line.send(first);
    });
    let line = read
        .recv_timeout(Duration::from_secs(60))
        .expect("a ready line");
    (child, serde_json::from_str(&line).unwrap())
}

/// The exit code, once the process has exited, within 30 s.
fn exited(child: &mut Child) -> Option<i32> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code();
        }
        assert!(Instant::now() < deadline, "still running after 30 s");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One HTTP/1.1 request to the server on `port`: its status.
fn http(port: u64, method: &str, path: &str) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
         Content-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    answer[9..12].parse().unwrap()
}

/// The ring's name, from the ready line.
fn ring(ready: &Value) -> &str {
    ready["shm"].as_str().unwrap()
}

#[test]
fn managed_it_says_where_it_is_and_stops_when_stdin_closes() {
    let folder = tempfile::tempdir().unwrap();
    let (mut child, ready) = start(&config(folder.path()));
    assert_eq!(ready["ready"], true);
    assert_eq!(ready["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(ready["pid"], child.id());
    let port = ready["port"].as_u64().unwrap();
    assert!(port > 0);
    assert_eq!(http(port, "GET", "/health"), 200);

    // The token: 32 random bytes in hex, in the data root, its owner's alone.
    let token = PathBuf::from(ready["token"].as_str().unwrap());
    assert!(
        token.is_absolute() && token.starts_with(folder.path()),
        "{token:?}"
    );
    let text = std::fs::read_to_string(&token).unwrap();
    assert!(
        text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit()),
        "{text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&token).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
    #[cfg(windows)]
    {
        // One entry, this account's: nothing inherited from the data root.
        let listing = Command::new("icacls").arg(&token).output().unwrap().stdout;
        let listing = String::from_utf8_lossy(&listing);
        let entries = listing
            .lines()
            .take_while(|line| !line.trim().is_empty())
            .count();
        let me = std::env::var("USERNAME").unwrap().to_lowercase();
        assert!(
            entries == 1 && listing.to_lowercase().contains(&me),
            "{listing}"
        );
    }

    // The ring, named for the process.
    assert_eq!(ring(&ready), format!("preview-{}", child.id()));
    let reader = RingReader::open(ring(&ready)).unwrap();
    assert_eq!(reader.writer(), child.id());
    drop(reader);

    drop(child.stdin.take());
    assert_eq!(exited(&mut child), Some(0));
    let gone = RingReader::open(ring(&ready)).err().unwrap();
    assert!(gone.is_not_found(), "{gone}");
}

#[test]
fn a_restart_exits_with_75_and_the_next_server_has_a_new_token() {
    let folder = tempfile::tempdir().unwrap();
    let config = config(folder.path());
    let (mut child, ready) = start(&config);
    let token = std::fs::read_to_string(ready["token"].as_str().unwrap()).unwrap();
    let port = ready["port"].as_u64().unwrap();
    assert_eq!(http(port, "POST", "/api/v1/system/restart"), 201);
    assert_eq!(exited(&mut child), Some(75));

    // What a supervisor does then: start it again.
    let (mut again, ready) = start(&config);
    assert_ne!(ready["pid"], child.id());
    let new = std::fs::read_to_string(ready["token"].as_str().unwrap()).unwrap();
    assert_ne!(new, token, "a new token each start");
    drop(again.stdin.take());
    assert_eq!(exited(&mut again), Some(0));
}

#[test]
fn the_ring_carries_previews_of_the_camera_s_frames() {
    let folder = tempfile::tempdir().unwrap();
    let (mut child, ready) = start(&config(folder.path()));
    let port = ready["port"].as_u64().unwrap();
    assert_eq!(http(port, "POST", "/api/v1/camera/start_streaming"), 201);
    let mut reader = RingReader::open(ring(&ready)).unwrap();
    let (mut message, deadline) = (Vec::new(), Instant::now() + Duration::from_secs(60));
    let mut numbers = Vec::new();
    while numbers.len() < 3 {
        assert!(Instant::now() < deadline, "previews: {numbers:?}");
        match reader.read(&mut message).unwrap() {
            RingRead::New(number) => {
                let read = Header::read(&message).unwrap();
                assert_eq!((read.header.kind, read.header.codec), (FRAME, JPEG));
                assert_eq!(read.header.sequence as u64, number);
                // The fake camera's whole 640 × 480, within the defaults'
                // 1920 × 1080.
                assert_eq!((read.header.width, read.header.height), (640, 480));
                assert!(read.jpeg.starts_with(&[0xff, 0xd8]) && read.blocks.is_empty());
                numbers.push(number);
            }
            RingRead::Same => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    assert!(numbers.windows(2).all(|w| w[1] > w[0]), "{numbers:?}");
    drop(child.stdin.take());
    assert_eq!(exited(&mut child), Some(0));
}
