//! The command line of `oscope-server`, run as a process.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(args)
        .output()
        .expect("oscope-server runs")
}

#[test]
fn version_prints_the_name_and_the_crate_version() {
    for flag in ["--version", "-V"] {
        let output = run(&[flag]);
        assert!(output.status.success(), "{flag}");
        let expected = format!("oscope-server {}\n", env!("CARGO_PKG_VERSION"));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flag}");
    }
}

#[test]
fn wrong_arguments_fail_and_point_to_the_help() {
    for args in [
        &["--serve"][..],
        &["--port", "x"],
        &["--print-openapi", "--print-td"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("error:") && stderr.contains("--help"),
            "{args:?}: {stderr}"
        );
    }
}

#[test]
fn print_openapi_and_print_td_describe_the_api_and_create_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("never-made");
    let root = root.to_str().unwrap();

    let output = run(&["--print-openapi", "--data-root", root]);
    assert!(output.status.success());
    let openapi: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(openapi["info"]["title"], "OScope");
    assert!(openapi["paths"]["/api/v1/system/uptime"]["get"].is_object());

    let output = run(&["--print-td", "--port", "8299", "--data-root", root]);
    assert!(output.status.success());
    let tds: Value = serde_json::from_slice(&output.stdout).unwrap();
    let td = &tds["system"];
    assert_eq!(td["base"], "http://127.0.0.1:8299/");
    assert_eq!(td["properties"].as_object().unwrap().len(), 8);
    assert_eq!(
        td["properties"]["os"]["forms"][0]["href"],
        "/api/v1/system/os"
    );

    assert!(!folder.path().join("never-made").exists());
}

#[test]
fn a_wrong_log_level_fails_with_exit_code_2() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("oscope.toml");
    std::fs::write(
        &path,
        "[logging]
level = \"oscope=loud\"
",
    )
    .unwrap();
    let data = folder.path().join("data");
    let output = run(&[
        "--config",
        path.to_str().unwrap(),
        "--data-root",
        data.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("oscope=loud"), "{stderr}");
}

#[test]
fn a_wrong_configuration_file_fails_with_where_and_why() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("oscope.toml");
    std::fs::write(&path, "[server]\nport = 8300\nhost = \"lab\"\n").unwrap();
    let output = run(&["--config", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("oscope.toml") && stderr.contains("line 3"),
        "{stderr}"
    );
    assert!(stderr.contains("host"), "{stderr}");
}

/// `GET path` over HTTP/1.1: the status line and the body.
fn get(address: &str, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_owned(), body.to_owned())
}

#[test]
fn it_serves_from_its_configuration_and_creates_its_folders() {
    let folder = tempfile::tempdir().unwrap();
    let config = folder.path().join("oscope.toml");
    std::fs::write(&config, "[storage]\nscratch_root = \"scratch\"\n").unwrap();
    let data = folder.path().join("data");
    let mut server = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["--config", config.to_str().unwrap(), "--port", "0"])
        .args(["--data-root", data.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // The first line says where it serves; the port is the system's choice.
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let served = line.trim().rsplit_once("serving on http://");
    let address = served.map(|(_, a)| a.trim_end_matches('/').to_owned());
    let result = std::panic::catch_unwind(|| {
        let address = address.expect("the serving line");
        assert_eq!(get(&address, "/health").0, "HTTP/1.1 200 OK");
        let (status, body) = get(&address, "/api/v1/system/data_root");
        assert_eq!(status, "HTTP/1.1 200 OK");
        let root: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            root["path"],
            std::path::absolute(&data).unwrap().display().to_string()
        );
        assert!(data.is_dir() && folder.path().join("scratch").is_dir());
        // Its logs: teta-wot's serving record, in today's daily file.
        let (status, body) = get(&address, "/api/v1/logs/records?text=serving");
        assert_eq!(status, "HTTP/1.1 200 OK");
        let page: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(page["records"][0]["level"], "info", "{page}");
        let today = data
            .join("logs")
            .join(format!("oscope-{}.jsonl", chrono::Utc::now().date_naive()));
        let file = std::fs::read_to_string(&today).unwrap();
        assert!(file.contains("serving"), "{file}");
    });
    server.kill().unwrap();
    server.wait().unwrap();
    result.unwrap();
}
