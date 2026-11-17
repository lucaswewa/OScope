//! The configuration file, and the command line's overrides.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use oscope_api::settings::ConfigFile;
use oscope_server::{
    CONFIG_VERSION, Cli, Config, ConfigError, DEFAULT_PORT, Logging, TomlFile, migrate_file,
};

/// Writes `text` to `oscope.toml` in `folder`, and returns its path.
fn write(folder: &Path, text: &str) -> PathBuf {
    let path = folder.join("oscope.toml");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_defaults_listen_on_this_computer_only_at_8210() {
    let config = Config::default();
    assert_eq!(config.network.address, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!((config.network.port, DEFAULT_PORT), (8210, 8210));
    assert_eq!(config.storage.data_root, Path::new(".oscope/data"));
    assert_eq!(config.storage.scratch_root, Path::new(".oscope/scratch"));
    assert!(config.devices.is_empty());
    let logging = (
        config.logging.level.as_str(),
        config.logging.ring,
        config.logging.keep_days,
    );
    assert_eq!(logging, ("info", 50_000, 30));
    assert!(!config.power.dry_run, "power acts by default");

    // An empty file means the defaults, with folders beside it.
    let folder = tempfile::tempdir().unwrap();
    let config = Config::load(&write(folder.path(), "")).unwrap();
    assert_eq!(config.network, Config::default().network);
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

[power]
dry_run = true
"#,
        scratch.display()
    );
    let config = Config::load(&write(folder.path(), &text)).unwrap();
    assert_eq!(config.network.address, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(config.network.port, 8300);
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
    assert!(config.power.dry_run);
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
    assert_eq!(config.network.port, 8301);
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
    assert_eq!(config.network, Config::default().network);
    assert_eq!(config.logging, Logging::default());
    assert!(config.storage.data_root.ends_with("../.oscope/data"));
    assert!(config.storage.scratch_root.ends_with("../.oscope/scratch"));
    assert_eq!(config.version, Some(CONFIG_VERSION));
}

/// The version-1 fixture `name`, copied into `folder` as `oscope.toml`.
fn fixture(folder: &Path, name: &str) -> PathBuf {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1");
    let path = folder.join("oscope.toml");
    std::fs::copy(fixtures.join(name), &path).unwrap();
    path
}

/// The comments in `text`, in order.
fn comments(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .collect()
}

#[test]
fn a_version_1_file_reads_as_version_2_s() {
    let folder = tempfile::tempdir().unwrap();
    let config = Config::load(&fixture(folder.path(), "full.toml")).unwrap();
    assert_eq!((config.version, config.network.port), (Some(1), 8300));
    let devices = config.devices_config().unwrap();
    let simulator = &devices.simulator;
    let (rate, sample) = (simulator.rate, simulator.sample.as_str());
    assert_eq!(
        (rate, sample, simulator.samples.len()),
        (2.0, "my cells", 1)
    );
    assert_eq!((devices.camera.as_str(), simulator.backlash), ("sim", 3.0));
    assert!(config.power.dry_run && config.jobs.workers == 3 && config.shm.slots == 4);
    let minimal = Config::load(&fixture(folder.path(), "minimal.toml")).unwrap();
    assert_eq!((minimal.version, minimal.network.port), (Some(1), 8301));
}

#[test]
fn migrating_a_file_keeps_its_comments_its_meaning_and_the_old_file() {
    let folder = tempfile::tempdir().unwrap();
    let path = fixture(folder.path(), "full.toml");
    let old = std::fs::read_to_string(&path).unwrap();
    let before = Config::load(&path).unwrap();
    let changed = migrate_file(&path).unwrap();
    let moved = [
        "[server] is now [network]",
        "[devices.simulator] is now [simulator]",
    ];
    assert_eq!(changed, moved);
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.starts_with("version = 2\n"), "{new}");
    for header in ["[network]", "[simulator]", "[[simulator.samples]]"] {
        assert!(new.contains(&format!("\n{header}\n")), "{header}: {new}");
    }
    assert!(!new.contains("[server]") && !new.contains("[devices.simulator"));
    assert_eq!(comments(&new), comments(&old));
    let after = Config::load(&path).unwrap();
    assert_eq!(after.version, Some(2));
    let unversioned = |config: Config| Config {
        version: None,
        ..config
    };
    assert_eq!(unversioned(after), unversioned(before));
    let kept = std::fs::read_to_string(folder.path().join("oscope.toml.v1.bak")).unwrap();
    assert_eq!(kept, old);
    // Once is enough: again, nothing changes, nor is anything kept.
    std::fs::remove_file(folder.path().join("oscope.toml.bak")).unwrap();
    assert!(migrate_file(&path).unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), new);
    assert!(!folder.path().join("oscope.toml.bak").exists());
}

#[test]
fn a_version_this_oscope_does_not_read_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    for (text, why) in [
        (
            "version = 3\n",
            "version 3: this OScope reads versions 1 to 2",
        ),
        ("version = \"two\"\n", "isn't a whole number"),
        (
            "version = 2\n[devices.simulator]\nrate = 2.0\n",
            "[devices.simulator] is version 1's",
        ),
    ] {
        let error = Config::load(&write(folder.path(), text)).unwrap_err();
        assert!(matches!(error, ConfigError::Version { .. }), "{error:?}");
        assert!(error.to_string().contains(why), "{text}: {error}");
    }
    // Version 2 has no [server].
    let text = "version = 2\n[server]\nport = 8300\n";
    let error = Config::load(&write(folder.path(), text)).unwrap_err();
    assert!(matches!(error, ConfigError::Invalid { .. }), "{error:?}");
    assert!(error.to_string().contains("server"), "{error}");
}

#[test]
fn the_settings_file_writes_only_what_changed_and_keeps_the_last() {
    let folder = tempfile::tempdir().unwrap();
    let path = fixture(folder.path(), "full.toml");
    let old = std::fs::read_to_string(&path).unwrap();
    let file = TomlFile(path.clone());
    let mut saved = file.read().unwrap();
    assert_eq!(saved.system.data_root, folder.path().join("data"));
    saved.system.port = 8302;
    saved.system.address = "::1".parse().unwrap();
    saved.system.scratch_root = "fast".into();
    saved.simulator.sample = "H&E tissue".to_owned();
    file.write(&saved).unwrap();
    // Migrated, the changes made in place, and the rest as it was.
    let new = std::fs::read_to_string(&path).unwrap();
    assert!(new.starts_with("version = 2\n"), "{new}");
    assert_eq!(new.lines().count(), old.lines().count() + 1, "{new}");
    for line in [
        "address = \"::1\"",
        "port = 8302",
        "scratch_root = \"fast\"",
        "sample = \"H&E tissue\"",
        "rate = 2.0",
        "data_root = \"data\"",
    ] {
        assert!(new.contains(&format!("\n{line}\n")), "{line}: {new}");
    }
    assert_eq!(comments(&new), comments(&old));
    let mut wanted = saved.clone();
    wanted.system.scratch_root = folder.path().join("fast");
    assert_eq!(file.read().unwrap(), wanted);
    // The old file beside it: the last one, and version 1's.
    let beside = |suffix: &str| {
        let backup = folder.path().join(format!("oscope.toml{suffix}"));
        std::fs::read_to_string(backup).unwrap()
    };
    assert_eq!(
        (beside(".bak"), beside(".v1.bak")),
        (old.clone(), old.clone())
    );
    saved.system.port = 8303;
    file.write(&saved).unwrap();
    assert_eq!((beside(".bak"), beside(".v1.bak")), (new, old));
    // A whole number stays one.
    saved.simulator.samples[0].seed = 9;
    file.write(&saved).unwrap();
    assert_eq!(file.read().unwrap().simulator.samples[0].seed, 9);
    // What wouldn't read back as asked is refused, and the file kept.
    let before = std::fs::read_to_string(&path).unwrap();
    saved.simulator.focus = f64::NAN;
    let refused = file.write(&saved).unwrap_err();
    assert!(refused.contains("differently"), "{refused}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert!(!folder.path().join("oscope.toml.new").exists());
}
