//! The configuration file, and the command line's overrides.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use oscope_server::{Cli, Config, ConfigError, DEFAULT_PORT, Logging};

/// Writes `text` to `oscope.toml` in `folder`, and returns its path.
fn write(folder: &Path, text: &str) -> PathBuf {
    let path = folder.join("oscope.toml");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_defaults_listen_on_this_computer_only_at_8210() {
    let config = Config::default();
    assert_eq!(config.server.address, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!((config.server.port, DEFAULT_PORT), (8210, 8210));
    assert_eq!(config.storage.data_root, Path::new(".oscope/data"));
    assert_eq!(config.storage.scratch_root, Path::new(".oscope/scratch"));
    assert!(config.devices.is_empty());
    let logging = (
        config.logging.level.as_str(),
        config.logging.ring,
        config.logging.keep_days,
    );
    assert_eq!(logging, ("info", 50_000, 30));

    // An empty file means the defaults, with folders beside it.
    let folder = tempfile::tempdir().unwrap();
    let config = Config::load(&write(folder.path(), "")).unwrap();
    assert_eq!(config.server, Config::default().server);
    assert_eq!(config.storage.data_root, folder.path().join(".oscope/data"));
}

#[test]
fn a_file_sets_every_key_and_its_folders_are_relative_to_it() {
    let folder = tempfile::tempdir().unwrap();
    let scratch = folder.path().join("fast").join("scratch");
    let text = format!(
        r#"
[server]
address = "0.0.0.0"
port = 8300

[storage]
data_root = "projects"
scratch_root = '{}'

[logging]
level = "info,oscope_api=debug"
ring = 1000
keep_days = 7

[devices]
camera = {{ kind = "simulated" }}

[python]
uv = "tools/uv"
sdk = "sdk"
wheelhouse = "wheels"
call_timeout_s = 30
idle_s = 0.5
"#,
        scratch.display()
    );
    let config = Config::load(&write(folder.path(), &text)).unwrap();
    assert_eq!(config.server.address, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(config.server.port, 8300);
    assert_eq!(config.storage.data_root, folder.path().join("projects"));
    assert_eq!(config.storage.scratch_root, scratch);
    assert_eq!(config.devices["camera"]["kind"].as_str(), Some("simulated"));
    let logging = Logging {
        level: "info,oscope_api=debug".into(),
        ring: 1000,
        keep_days: 7,
    };
    assert_eq!(config.logging, logging);
    let python = config.python.runtime();
    let at = |name: &str| Some(folder.path().join(name));
    assert_eq!(
        (python.uv, Some(python.sdk), python.wheelhouse),
        (at("tools/uv"), at("sdk"), at("wheels"))
    );
    let times = (python.call_timeout, python.idle);
    assert_eq!(times, (Duration::from_secs(30), Duration::from_millis(500)));
}

#[test]
fn a_wrong_file_is_refused_with_where_and_why() {
    let folder = tempfile::tempdir().unwrap();
    for (text, line, why) in [
        ("[server]\nprot = 8300\n", "line 2", "prot"),
        ("[server]\nport = \"high\"\n", "line 2", "u16"),
        ("[server]\nport = 70000\n", "line 2", "70000"),
        (
            "[storage]\ndata_root = \"a\"\n[colour]\n",
            "line 3",
            "colour",
        ),
    ] {
        let path = write(folder.path(), text);
        let error = Config::load(&path).unwrap_err();
        assert!(matches!(error, ConfigError::Invalid { .. }), "{error:?}");
        let message = error.to_string();
        assert!(message.contains("oscope.toml"), "{message}");
        assert!(
            message.contains(line) && message.contains(why),
            "{text}: {message}"
        );
    }

    let missing = folder.path().join("missing.toml");
    let error = Config::load(&missing).unwrap_err();
    assert!(matches!(error, ConfigError::Read { .. }), "{error:?}");
    assert!(error.to_string().contains("missing.toml"), "{error}");
}

#[test]
fn the_command_line_overrides_the_file() {
    let folder = tempfile::tempdir().unwrap();
    let path = write(
        folder.path(),
        "[server]\nport = 8300\n[storage]\ndata_root = \"a\"\nscratch_root = \"b\"\n",
    );
    let file = path.to_str().unwrap();
    let args = [
        "oscope-server",
        "--config",
        file,
        "--port",
        "8301",
        "--data-root",
        "c",
    ];
    let config = Cli::try_parse_from(args).unwrap().config().unwrap();
    assert_eq!(config.server.port, 8301);
    // A folder on the command line is relative to where it is run.
    assert_eq!(config.storage.data_root, Path::new("c"));
    assert_eq!(config.storage.scratch_root, folder.path().join("b"));

    let defaults = Cli::try_parse_from(["oscope-server"])
        .unwrap()
        .config()
        .unwrap();
    assert_eq!(defaults, Config::default());
}

#[test]
fn the_command_line_refuses_what_it_cannot_do() {
    for args in [
        &["--port", "70000"][..],
        &["--port"],
        &["--print-openapi", "--print-td"],
        &["--serve"],
    ] {
        let all = std::iter::once("oscope-server").chain(args.iter().copied());
        assert!(Cli::try_parse_from(all).is_err(), "{args:?}");
    }
}

#[test]
fn the_sample_configuration_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/oscope.toml");
    let config = Config::load(&path).unwrap();
    assert_eq!(config.server, Config::default().server);
    assert_eq!(config.logging, Logging::default());
    assert!(config.storage.data_root.ends_with("../.oscope/data"));
    assert!(config.storage.scratch_root.ends_with("../.oscope/scratch"));
}
