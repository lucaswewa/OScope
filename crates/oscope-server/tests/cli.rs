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
    assert_eq!(td["properties"].as_object().unwrap().len(), 11);
    assert!(td["properties"]["scratch"].is_object());
    assert!(td["properties"]["recordings"].is_object());
    assert!(td["properties"]["notices"].is_object());
    assert_eq!(
        td["properties"]["os"]["forms"][0]["href"],
        "/api/v1/system/os"
    );

    assert!(!folder.path().join("never-made").exists());
}

#[test]
fn printing_takes_an_extension_s_driver_for_a_fake_and_loads_no_extension() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("oscope.toml");
    let devices = "[devices]
camera = \"example.fakecam\"
stage = \"example.stage\"
";
    std::fs::write(&path, devices).unwrap();
    let root = folder.path().join("never-made");
    let output = run(&[
        "--print-td",
        "--config",
        path.to_str().unwrap(),
        "--data-root",
        root.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let tds: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(tds["camera"]["properties"].is_object());
    assert!(tds["stage"]["properties"].is_object());
    assert!(!root.exists());
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
    let text = "[storage]\nscratch_root = \"scratch\"\n\n[power]\ndry_run = true\n";
    std::fs::write(&config, text).unwrap();
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
        // Power in a dry run, as the configuration says (P51).
        assert_eq!(get(&address, "/api/v1/power/dry_run").1, "true");
        // The settings Thing has the file: its port, not the command line's.
        let (_, system) = get(&address, "/api/v1/settings/system");
        assert!(system.contains("\"port\":8210"), "{system}");
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

/// Writes a test's own configuration in `folder`: the server's data and its
/// scratch space there, and power in a dry run. Without it, the scratch space
/// is the working folder's, which every test's server would share, and each
/// clears it as it starts.
fn own_config(folder: &std::path::Path) -> String {
    let config = folder.join("oscope.toml");
    let text = "[storage]\ndata_root = \"data\"\nscratch_root = \"scratch\"\n\n\
                [power]\ndry_run = true\n";
    std::fs::write(&config, text).unwrap();
    config.to_str().unwrap().to_owned()
}

/// Starts the server on a port the system picks, with `args`, and gives it
/// with the address its first line says it serves on.
fn serve(args: &[&str]) -> (std::process::Child, Option<String>) {
    let mut server = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["--port", "0"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let served = line.trim().rsplit_once("serving on http://");
    let address = served.map(|(_, a)| a.trim_end_matches('/').to_owned());
    (server, address)
}

#[test]
fn it_serves_the_web_app_s_folder_beside_the_api() {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("dist");
    std::fs::create_dir_all(app.join("assets")).unwrap();
    std::fs::write(
        app.join("index.html"),
        "<!doctype html><title>OScope</title>",
    )
    .unwrap();
    std::fs::write(app.join("assets/app.js"), "export const app = 1;").unwrap();
    // A file beside the app's folder, which nothing may serve.
    std::fs::write(folder.path().join("secret.txt"), "not the app's").unwrap();
    let config = own_config(folder.path());
    let (mut server, address) = serve(&["--config", &config, "--web", app.to_str().unwrap()]);
    let result = std::panic::catch_unwind(|| {
        let address = address.expect("the serving line");
        // Its files in the test's folder: none in the working folder's.
        for own in ["data", "scratch"] {
            assert!(folder.path().join(own).is_dir(), "{own}");
        }
        // The app at the root, and its files by their paths.
        let (status, body) = get(&address, "/");
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(body.contains("<title>OScope</title>"), "{body}");
        let (status, body) = get(&address, "/assets/app.js");
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, "export const app = 1;");
        // The API, at the same origin.
        assert_eq!(get(&address, "/health").0, "HTTP/1.1 200 OK");
        assert_eq!(get(&address, "/api/v1/system/version").0, "HTTP/1.1 200 OK");
        // Nothing outside the folder, however the path is written.
        for path in [
            "/../secret.txt",
            "/assets/../../secret.txt",
            "/%2e%2e/secret.txt",
        ] {
            let (status, body) = get(&address, path);
            assert!(!status.contains("200"), "{path}: {status}");
            assert!(!body.contains("not the app's"), "{path}");
        }
        assert_eq!(get(&address, "/missing.js").0, "HTTP/1.1 404 Not Found");
    });
    server.kill().unwrap();
    server.wait().unwrap();
    result.unwrap();
}

#[test]
fn a_web_folder_that_would_hide_the_api_or_is_missing_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let config = own_config(folder.path());
    // A file named as one of the API's own paths.
    let hiding = folder.path().join("hiding");
    std::fs::create_dir_all(&hiding).unwrap();
    std::fs::write(hiding.join("health"), "ok").unwrap();
    let missing = folder.path().join("never-built");
    for (web, says) in [(&hiding, "health"), (&missing, "never-built")] {
        let web = web.to_str().unwrap();
        let output = run(&["--config", &config, "--port", "0", "--web", web]);
        assert_eq!(output.status.code(), Some(1), "{web}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("the pages in") && stderr.contains(says),
            "{stderr}"
        );
    }
}

/// A Python extension's manifest.
const MANIFEST: &str = r#"id = "org.example.hello"
name = "Hello"
version = "0.1.0"
license = "MIT"
publisher = "Example Lab"
oscope-api = "^1.0"
kind = "python"

[provides]
nodes = ["hello.greet"]

[python]
requires-python = ">=3.12"
entry = "hello"
"#;

#[test]
fn config_migrate_rewrites_an_old_file_once() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("oscope.toml");
    std::fs::write(&path, "# Mine.\n[server]\nport = 8300\n").unwrap();
    let file = path.to_str().unwrap();
    let output = run(&["--config", file, "config", "migrate"]);
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(said.contains("[server] is now [network]"), "{said}");
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.starts_with("version = 2\n"), "{new}");
    assert!(new.contains("# Mine.\n[network]\nport = 8300\n"), "{new}");
    let again = run(&["--config", file, "config", "migrate"]);
    let said = String::from_utf8_lossy(&again.stdout);
    assert!(said.contains("already version 2"), "{said}");
    assert!(!run(&["config", "migrate"]).status.success());
}

#[test]
fn ext_pack_names_the_package_and_packs_the_same_bytes_each_time() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("hello");
    std::fs::create_dir_all(source.join("hello")).unwrap();
    std::fs::write(source.join("manifest.toml"), MANIFEST).unwrap();
    std::fs::write(source.join("hello/__init__.py"), "GREETING = 'hello'\n").unwrap();
    let source = source.to_str().unwrap();

    // Named by its ID and version, where it runs.
    let output = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["ext", "pack", source])
        .current_dir(folder.path())
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{said}");
    let named = folder.path().join("org.example.hello-0.1.0.omext");
    let sha256 = oscope_ext::sha256(&named).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("packed org.example.hello 0.1.0 into "),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("bytes, SHA-256 {sha256}")),
        "{stdout}"
    );

    // Where --out says: the same bytes.
    let out = folder.path().join("again.omext");
    let output = run(&["ext", "pack", source, "--out", out.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&named).unwrap());

    // A wrong manifest fails, saying where and why, and writes nothing.
    let wrong = MANIFEST.replace("\"^1.0\"", "\"one\"");
    std::fs::write(folder.path().join("hello/manifest.toml"), wrong).unwrap();
    let out = folder.path().join("wrong.omext");
    let output = run(&["ext", "pack", source, "--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("oscope-server: extensions: the manifest: line 6"),
        "{stderr}"
    );
    assert!(!out.exists());
}

/// Runs `oscope-server` with `args`, `input` on its stdin.
fn run_with(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("oscope-server runs");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

#[test]
fn accounts_are_added_listed_changed_and_removed_from_the_command_line() {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    let data = data.to_str().unwrap();
    let account = |args: &[&str], input: &str| {
        let mut all = vec!["--data-root", data, "account"];
        all.extend_from_slice(args);
        run_with(&all, input)
    };
    let text = |output: &Output| String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let added = account(&["add", "alice"], "alice's password\n");
    assert!(added.status.success(), "{added:?}");
    assert_eq!(text(&added), "added alice");
    // Its password, stdin's line without its end.
    let file = std::path::Path::new(data).join("accounts.json");
    let kept = oscope_api::auth::Accounts::open(&file).unwrap();
    assert!(kept.verify("alice", "alice's password"));
    assert_eq!(text(&account(&["list"], "")), "alice");
    let changed = account(&["passwd", "alice"], "a new password\n");
    assert_eq!(text(&changed), "changed alice's password");
    // Refused, saying why: a name, a short password, one that isn't.
    for (args, input) in [
        (&["add", "Alice"][..], "a long password\n"),
        (&["add", "bob"][..], "short\n"),
        (&["remove", "bob"][..], ""),
    ] {
        let refused = account(args, input);
        let error = String::from_utf8_lossy(&refused.stderr).to_string();
        assert!(
            !refused.status.success() && error.contains("accounts:"),
            "{args:?}: {error}"
        );
    }
    assert_eq!(text(&account(&["remove", "alice"], "")), "removed alice");
    assert_eq!(text(&account(&["list"], "")), "");
}

#[test]
fn the_network_needs_an_account_and_a_given_certificate_serves_tls() {
    let folder = tempfile::tempdir().unwrap();
    let data = folder.path().join("data");
    // On the network without an account: refused before it listens.
    let config = folder.path().join("network.toml");
    std::fs::write(&config, "[server]\naddress = \"0.0.0.0\"\nport = 0\n").unwrap();
    let args = [
        "--config",
        config.to_str().unwrap(),
        "--data-root",
        data.to_str().unwrap(),
    ];
    let refused = run(&args);
    let error = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(
        !refused.status.success() && error.contains("needs an account"),
        "{error}"
    );
    // A certificate made once and kept, its key private; served over TLS.
    let tls = folder.path().join("tls");
    let localhost = "127.0.0.1".parse().unwrap();
    let made = oscope_server::certificate(&tls, localhost).unwrap();
    assert_eq!(oscope_server::certificate(&tls, localhost).unwrap(), made);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(tls.join("key.pem"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let config = folder.path().join("tls.toml");
    let text = "[security]\ncertificate = \"tls/certificate.pem\"\nkey = \"tls/key.pem\"\n";
    std::fs::write(&config, text).unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_oscope-server"))
        .args(["--config", config.to_str().unwrap(), "--port", "0"])
        .args(["--data-root", data.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let result = std::panic::catch_unwind(|| {
        let served = line.trim().rsplit_once("serving on https://");
        let address = served.map(|(_, a)| a.trim_end_matches('/').to_owned());
        let address = address.unwrap_or_else(|| panic!("{line}"));
        // Plain HTTP gets no answer. Asked to close, a plain server would
        // answer and close, rather than keep the test waiting.
        let mut stream = TcpStream::connect(&address).unwrap();
        let request =
            format!("GET /health HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
        assert!(!answer.starts_with(b"HTTP/1.1 200"), "{answer:?}");
    });
    server.kill().unwrap();
    server.wait().unwrap();
    result.unwrap();
}
