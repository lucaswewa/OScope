//! Actual Linux process signals and restart, with an open media WebSocket.

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn start(folder: &std::path::Path) -> (Server, String) {
    let config = folder.join("oscope.toml");
    std::fs::write(&config, "[network]\nport=0\n[storage]\ndata_root='data'\nscratch_root='scratch'\n[devices]\ncamera='fake'\nstage='fake'\nillumination='fake'\nobjective='fake'\n").unwrap();
    let mut child = Server(
        Command::new(env!("CARGO_BIN_EXE_oscope-server"))
            .arg("--config")
            .arg(config)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = send.send(line);
    });
    let line = receive
        .recv_timeout(Duration::from_secs(60))
        .expect("serving line");
    let address = line
        .trim()
        .split("serving on http://")
        .nth(1)
        .expect("HTTP address")
        .trim_end_matches('/')
        .to_owned();
    (child, address)
}

fn request(address: &str, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn stopped(server: &mut Server, expected: i32) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(exit) = server.0.try_wait().unwrap() {
            assert_eq!(exit.code(), Some(expected));
            break;
        }
        assert!(Instant::now() < until, "server did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn check(restart: bool) {
    let folder = tempfile::tempdir().unwrap();
    let (mut server, address) = start(folder.path());
    assert!(request(&address, "GET", "/ready", "").starts_with("HTTP/1.1 200"));
    let stream = TcpStream::connect(&address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (mut socket, _) =
        tungstenite::client(format!("ws://{address}/api/v1/media/ping"), stream).unwrap();
    socket
        .send(tungstenite::Message::Text("lifecycle".into()))
        .unwrap();
    assert_eq!(socket.read().unwrap().into_text().unwrap(), "lifecycle");
    if restart {
        let answer = request(
            &address,
            "POST",
            "/api/v1/power/restart",
            r#"{"confirm":true}"#,
        );
        assert!(answer.starts_with("HTTP/1.1 201"), "{answer}");
    } else {
        assert!(
            Command::new("bash")
                .args([
                    "-c",
                    "kill -TERM \"$1\"",
                    "oscope-lifecycle",
                    &server.0.id().to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
    }
    let frame = socket.read().expect("graceful WebSocket close");
    assert!(
        matches!(frame, tungstenite::Message::Close(Some(frame)) if frame.code==tungstenite::protocol::frame::coding::CloseCode::Away)
    );
    stopped(&mut server, if restart { 75 } else { 0 });
}

#[test]
fn sigterm_stops_the_process_and_closes_media() {
    check(false);
}

#[test]
fn restart_returns_the_supervisor_exit_code_and_closes_media() {
    check(true);
}
