//! `.omext` packages: what is wrong with a manifest, unpacking with care,
//! refusing what doesn't suit this OScope, installing a package again, the
//! changes each start makes, two extensions providing one name, and packing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use oscope_ext::{Contribution, ExtError, Extensions, Manifest, PLATFORM, PLATFORMS, Package};
use semver::Version;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

const ID: &str = "org.example.hello";

/// A Python extension's manifest, which suits every platform.
fn manifest(id: &str, version: &str, node: &str) -> String {
    format!(
        r#"id = "{id}"
name = "Hello"
version = "{version}"
license = "MIT"
publisher = "Example Lab"
oscope-api = "^1.0"
kind = "python"

[provides]
nodes = ["{node}"]

[python]
requires-python = ">=3.12"
entry = "hello"
"#
    )
}

/// `text`, a Python manifest, made a Rust one with `libraries`.
fn rust(text: &str, libraries: &str) -> String {
    let python = "[python]\nrequires-python = \">=3.12\"\nentry = \"hello\"\n";
    let text = text.replace("kind = \"python\"", "kind = \"rust\"");
    text.replace(python, &format!("[rust.libraries]\n{libraries}\n"))
}

/// A zip file of `entries`, each a name and its contents, stored as they
/// are, so that tests can change what an entry says about itself.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A package of the manifest `text` and `hello/__init__.py`.
fn package_of(text: &str) -> Vec<u8> {
    zip_of(&[
        ("manifest.toml", text.as_bytes()),
        ("hello/__init__.py", b""),
    ])
}

/// Changes what the entry `name` says it unpacks to, in its local header
/// (at 22, the name at 30) and in the central directory (at 24, the name
/// at 46).
fn say_size(zip: &mut [u8], name: &str, size: u32) {
    for (signature, size_at, name_at) in [(b"PK\x03\x04", 22, 30), (b"PK\x01\x02", 24, 46)] {
        for at in 0..zip.len() - 4 {
            if &zip[at..at + 4] == signature && zip[at + name_at..].starts_with(name.as_bytes()) {
                zip[at + size_at..at + size_at + 4].copy_from_slice(&size.to_le_bytes());
            }
        }
    }
}

/// Writes `bytes` to the file `name` in `folder`.
fn file(folder: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

/// Every file under `folder`.
fn files(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        match path.is_dir() {
            true => found.extend(files(&path)),
            false => found.push(path),
        }
    }
    found
}

fn version(text: &str) -> Option<Version> {
    Some(Version::parse(text).unwrap())
}

#[test]
fn a_manifest_says_what_is_wrong_with_it() {
    let good = manifest(ID, "0.1.0", "hello.greet");
    let parsed = Manifest::parse(&good).unwrap();
    assert_eq!(
        parsed.provided().collect::<Vec<_>>(),
        [(Contribution::Nodes, "hello.greet")]
    );
    parsed.check_host().unwrap();
    let kind = "kind = \"python\"";
    let cases = [
        (
            "id = \"org.example.hello\"",
            "id = \"Org.Example\"",
            "isn't reverse DNS",
        ),
        (
            "id = \"org.example.hello\"",
            "id = \"hello\"",
            "isn't reverse DNS",
        ),
        ("version = \"0.1.0\"", "version = \"0.1\"", "line 3: "),
        ("name = \"Hello\"", "name = \" \"", "name is empty"),
        (kind, "kind = \"java\"", "unknown variant `java`"),
        (
            kind,
            "kind = \"rust\"",
            "a rust package has a [rust] table, and no other",
        ),
        (
            kind,
            "kind = \"python\"\ncolour = \"red\"",
            "unknown field `colour`",
        ),
        (
            "nodes = [\"hello.greet\"]",
            "nodes = []",
            "[provides] names nothing",
        ),
        (
            "nodes = [\"hello.greet\"]",
            "nodes = [\"a.b\", \"a.b\"]",
            "is there twice",
        ),
        (
            "nodes = [\"hello.greet\"]",
            "nodes = [\"hello-greet\"]",
            "isn't dotted names",
        ),
        (
            "nodes = [\"hello.greet\"]",
            "widgets = [\"a\"]",
            "unknown variant `widgets`",
        ),
        (
            "nodes = [\"hello.greet\"]",
            "nodes = [\"hello.greet\"]
drivers = [\"fake\"]",
            "provides.drivers \"fake\" is one of OScope's own drivers",
        ),
        (
            "entry = \"hello\"",
            "entry = \"hello-world\"",
            "isn't a module's dotted name",
        ),
        (
            kind,
            "kind = \"python\"\nassets = [\"../panel.json\"]",
            "leaves the package",
        ),
        (
            kind,
            "kind = \"python\"\nassets = [\"C:/panel.json\"]",
            "or one of \\ : *",
        ),
    ];
    for (old, new, expected) in cases {
        let text = good.replace(old, new);
        assert_ne!(text, good, "{old}");
        let error = Manifest::parse(&text).unwrap_err();
        assert!(matches!(error, ExtError::Manifest(_)), "{new}: {error}");
        assert!(error.to_string().contains(expected), "{new}: {error}");
    }
    let libraries = [
        ("", "[rust.libraries] names no library"),
        (
            "macos-arm64 = \"lib/hello.dylib\"",
            "the platforms are windows-x86_64 and linux-x86_64",
        ),
        (
            "linux-x86_64 = \"/lib/libhello.so\"",
            "\"/lib/libhello.so\" is absolute",
        ),
        ("windows-x86_64 = 'lib\\hello.dll'", "or one of \\ : *"),
    ];
    for (libraries, expected) in libraries {
        let error = Manifest::parse(&rust(&good, libraries))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{libraries}: {error}");
    }
    let both = "windows-x86_64 = \"lib/hello.dll\"\nlinux-x86_64 = \"lib/libhello.so\"";
    let parsed = Manifest::parse(&rust(&good, both)).unwrap();
    parsed.check_host().unwrap();
    let files: Vec<_> = parsed.files().collect();
    assert_eq!(files, ["lib/libhello.so", "lib/hello.dll"]);

    // A newer API's manifest: it says so, even when it has fields this
    // OScope doesn't know.
    let newer = good.replace("^1.0", "^2.0");
    let error = Manifest::parse(&newer).unwrap().check_host().unwrap_err();
    let said = "it needs the extension API ^2.0, and this OScope offers 1.0.0";
    assert_eq!(error.to_string(), said);
    let error = Manifest::parse(&format!("{newer}\n[gpu]\nneeds = true\n")).unwrap_err();
    assert_eq!(error.to_string(), said);
}

#[test]
fn a_package_that_could_escape_or_clash_is_refused_and_leaves_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let extensions = Extensions::open(folder.path().join("extensions")).unwrap();
    let text = manifest(ID, "0.1.0", "hello.greet");
    let with = |name: &str| zip_of(&[("manifest.toml", text.as_bytes()), (name, b"evil")]);
    let mut symlink = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    symlink.start_file("manifest.toml", options).unwrap();
    symlink.write_all(text.as_bytes()).unwrap();
    symlink
        .add_symlink("hello/evil.txt", "../../../evil.txt", options)
        .unwrap();
    let symlink = symlink.finish().unwrap().into_inner();
    let mut oversized = zip_of(&[
        ("manifest.toml", text.as_bytes()),
        ("hello/a.bin", &[0; 16]),
        ("hello/b.bin", &[0; 16]),
    ]);
    say_size(&mut oversized, "hello/a.bin", 0xFFFF_FFF0);
    say_size(&mut oversized, "hello/b.bin", 0xFFFF_FFF0);
    let mut understated = zip_of(&[
        ("manifest.toml", text.as_bytes()),
        ("hello/big.bin", &[7; 1000]),
    ]);
    say_size(&mut understated, "hello/big.bin", 10);
    let asset = text.replace(
        "kind = \"python\"",
        "kind = \"python\"\nassets = [\"panels/a.json\"]",
    );
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (with("../evil.txt"), "\"../evil.txt\" leaves the package"),
        (with("hello/../../evil.txt"), "leaves the package"),
        (with("/evil.txt"), "\"/evil.txt\" is absolute"),
        (with("C:/evil.txt"), "or one of \\ : *"),
        (with("hello\\..\\..\\evil.txt"), "or one of \\ : *"),
        (with("hello/./evil.txt"), "has an empty part"),
        (with("hello//evil.txt"), "has an empty part"),
        (with("hello/NUL.txt"), "keeps for a device"),
        (with("hello/evil.txt."), "ending in a dot or a space"),
        (
            with("Manifest.TOML"),
            "is there twice, perhaps in another case",
        ),
        (
            with(".oscope/venv/evil.txt"),
            "is a name OScope keeps for itself",
        ),
        (symlink, "\"hello/evil.txt\" is a symbolic link"),
        (oversized, "it unpacks to more than 4294967296 bytes"),
        (
            understated,
            "the entry \"hello/big.bin\": File is larger than its declared",
        ),
        (zip_of(&[("hello/__init__.py", b"")]), "no manifest.toml"),
        (b"not a zip file at all".to_vec(), "not a zip file"),
        (
            package_of(&asset),
            "names \"panels/a.json\", which it doesn't have",
        ),
    ];
    for (bytes, expected) in cases {
        let path = file(folder.path(), "package.omext", &bytes);
        let error = extensions.install(&path).unwrap_err();
        assert!(matches!(error, ExtError::Package(_)), "{expected}: {error}");
        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
    // Nothing was installed, nothing is left unpacking, nothing got out.
    assert!(extensions.list().is_empty());
    let all = files(folder.path());
    assert_eq!(all, [folder.path().join("package.omext")], "{all:?}");
}

#[test]
fn a_package_for_another_api_or_platform_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let extensions = Extensions::open(folder.path().join("extensions")).unwrap();
    let newer = manifest(ID, "0.1.0", "hello.greet").replace("^1.0", "^2.0");
    let error = extensions.install(&file(folder.path(), "newer.omext", &package_of(&newer)));
    let error = error.unwrap_err().to_string();
    assert_eq!(
        error,
        "it needs the extension API ^2.0, and this OScope offers 1.0.0"
    );
    let platform = PLATFORM.expect("OScope's platforms");
    let other = PLATFORMS.into_iter().find(|p| *p != platform).unwrap();
    let text = rust(
        &manifest(ID, "0.1.0", "hello.greet"),
        &format!("{other} = \"lib/hello\""),
    );
    let bytes = zip_of(&[("manifest.toml", text.as_bytes()), ("lib/hello", b"\0")]);
    let error = extensions.install(&file(folder.path(), "other.omext", &bytes));
    let error = error.unwrap_err();
    assert!(matches!(error, ExtError::NoBuild { .. }), "{error}");
    let said = format!("it has no library for {platform}, only for {other}");
    assert_eq!(error.to_string(), said);
    assert!(extensions.list().is_empty());
    assert!(!folder.path().join("extensions").join(ID).exists());
}

#[test]
fn installing_a_package_again_changes_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("extensions");
    let extensions = Extensions::open(&root).unwrap();
    let text = manifest(ID, "0.1.0", "hello.greet");
    let path = file(folder.path(), "hello.omext", &package_of(&text));
    let (first, unpacked) = extensions.install(&path).unwrap();
    assert!(unpacked);
    assert_eq!(
        (first.enabled, &first.loaded, &first.next),
        (true, &None, &version("0.1.0"))
    );
    assert_eq!(first.sha256, oscope_ext::sha256(&path).unwrap());
    assert_eq!(
        fs::read_to_string(root.join(ID).join("0.1.0/manifest.toml")).unwrap(),
        text
    );
    let state = fs::read(root.join(ID).join("state.json")).unwrap();
    let (again, unpacked) = extensions.install(&path).unwrap();
    assert!(!unpacked);
    assert_eq!(again, first);
    assert_eq!(fs::read(root.join(ID).join("state.json")).unwrap(), state);
    assert!(extensions.restart_needed());

    // The same version from another package: a conflict.
    let other = zip_of(&[
        ("manifest.toml", text.as_bytes()),
        ("hello/__init__.py", b"x = 1"),
    ]);
    let error = extensions
        .install(&file(folder.path(), "other.omext", &other))
        .unwrap_err();
    assert!(matches!(error, ExtError::Conflict(_)), "{error}");
    assert!(
        error.to_string().contains("give this one another version"),
        "{error}"
    );

    // Started again, it loads; and installing the package again changes
    // nothing still.
    let extensions = Extensions::open(&root).unwrap();
    let (again, unpacked) = extensions.install(&path).unwrap();
    assert!(!unpacked);
    assert_eq!(
        (&again.loaded, &again.next),
        (&version("0.1.0"), &version("0.1.0"))
    );
    assert!(!extensions.restart_needed());
}

#[test]
fn each_change_takes_effect_at_the_next_start() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("extensions");
    let install = |extensions: &Extensions, version: &str| {
        let bytes = package_of(&manifest(ID, version, "hello.greet"));
        let path = file(folder.path(), &format!("hello-{version}.omext"), &bytes);
        extensions.install(&path).unwrap().0
    };
    let extensions = Extensions::open(&root).unwrap();
    install(&extensions, "0.1.0");
    assert!(extensions.loaded().is_empty() && extensions.registry().is_empty());

    // Started again: loaded, with what it provides.
    let extensions = Extensions::open(&root).unwrap();
    assert_eq!(extensions.loaded()[0].folder, root.join(ID).join("0.1.0"));
    assert_eq!(
        extensions.registry()[&Contribution::Nodes]["hello.greet"],
        ID
    );
    assert!(!extensions.restart_needed());

    // Disabled: loaded until the next start, which leaves it out.
    let disabled = extensions.disable(ID).unwrap();
    assert_eq!((disabled.loaded, disabled.next), (version("0.1.0"), None));
    assert!(extensions.restart_needed());
    let extensions = Extensions::open(&root).unwrap();
    assert!(extensions.loaded().is_empty() && !extensions.get(ID).unwrap().enabled);
    extensions.enable(ID).unwrap();
    let extensions = Extensions::open(&root).unwrap();
    assert_eq!(extensions.loaded().len(), 1);

    // A new version unpacks beside the loaded one. The next start loads it,
    // and removes the old.
    let upgraded = install(&extensions, "0.2.0");
    assert_eq!(
        (upgraded.loaded, upgraded.next),
        (version("0.1.0"), version("0.2.0"))
    );
    assert!(root.join(ID).join("0.1.0").is_dir() && root.join(ID).join("0.2.0").is_dir());
    let extensions = Extensions::open(&root).unwrap();
    assert_eq!(extensions.get(ID).unwrap().loaded, version("0.2.0"));
    assert!(!root.join(ID).join("0.1.0").exists());

    // Uninstalled while loaded: removed at the next start.
    let uninstalled = extensions.uninstall(ID).unwrap().unwrap();
    assert!(uninstalled.uninstalled && uninstalled.next.is_none());
    assert!(matches!(extensions.enable(ID), Err(ExtError::Conflict(_))));
    assert!(root.join(ID).exists());
    let extensions = Extensions::open(&root).unwrap();
    assert!(extensions.list().is_empty() && !root.join(ID).exists());

    // Not loaded: removed at once.
    install(&extensions, "0.3.0");
    assert_eq!(extensions.uninstall(ID).unwrap(), None);
    assert!(!root.join(ID).exists());
    assert!(matches!(
        extensions.uninstall(ID),
        Err(ExtError::Unknown(_))
    ));

    // What an unfinished install left goes at the next start.
    fs::create_dir_all(root.join(".incoming/unpack-left")).unwrap();
    Extensions::open(&root).unwrap();
    assert!(!root.join(".incoming").exists());
}

#[test]
fn of_two_extensions_providing_a_name_the_first_by_id_loads() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("extensions");
    let extensions = Extensions::open(&root).unwrap();
    for id in ["org.b.second", "org.a.first"] {
        let bytes = package_of(&manifest(id, "1.0.0", "shared.node"));
        extensions
            .install(&file(folder.path(), &format!("{id}.omext"), &bytes))
            .unwrap();
    }
    let extensions = Extensions::open(&root).unwrap();
    let ids: Vec<_> = extensions
        .loaded()
        .iter()
        .map(|l| l.manifest.id.as_str())
        .collect();
    assert_eq!(ids, ["org.a.first"]);
    let second = extensions.get("org.b.second").unwrap();
    let said = "org.a.first provides nodes `shared.node` too, and loads first";
    assert_eq!((second.next, second.problem.as_deref()), (None, Some(said)));
    assert!(!extensions.restart_needed());

    // With the first disabled, the next start loads the second.
    extensions.disable("org.a.first").unwrap();
    let second = extensions.get("org.b.second").unwrap();
    assert_eq!((second.next, second.problem), (version("1.0.0"), None));
    assert!(extensions.restart_needed());
}

#[test]
fn a_folder_packs_to_the_same_bytes_each_time_and_installs() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("hello");
    fs::create_dir_all(source.join("hello")).unwrap();
    fs::create_dir_all(source.join("panels")).unwrap();
    let assets = "kind = \"python\"\nassets = [\"panels/greet.json\"]";
    let text = manifest(ID, "0.1.0", "hello.greet").replace("kind = \"python\"", assets);
    fs::write(source.join("manifest.toml"), &text).unwrap();
    fs::write(source.join("hello/__init__.py"), "GREETING = 'hello'\n").unwrap();
    fs::write(source.join("panels/greet.json"), "{}").unwrap();
    let (one, two) = (
        folder.path().join("one.omext"),
        folder.path().join("two.omext"),
    );
    let packed = oscope_ext::pack(&source, &one).unwrap();
    // A day older, the files pack the same.
    let day_ago = SystemTime::now() - Duration::from_secs(86_400);
    for name in ["manifest.toml", "hello/__init__.py"] {
        let file = fs::File::options()
            .write(true)
            .open(source.join(name))
            .unwrap();
        file.set_modified(day_ago).unwrap();
    }
    oscope_ext::pack(&source, &two).unwrap();
    assert_eq!(fs::read(&one).unwrap(), fs::read(&two).unwrap());
    assert_eq!(Package::open(&one).unwrap().manifest(), &packed);

    let extensions = Extensions::open(folder.path().join("extensions")).unwrap();
    extensions.install(&one).unwrap();
    let unpacked = folder.path().join("extensions").join(ID).join("0.1.0");
    for name in ["manifest.toml", "hello/__init__.py", "panels/greet.json"] {
        let (got, put) = (fs::read(unpacked.join(name)), fs::read(source.join(name)));
        assert_eq!(got.unwrap(), put.unwrap(), "{name}");
    }

    // Refused: a symbolic link, where one can be made (Windows needs
    // developer mode), and an asset that isn't there.
    let link = source.join("hello/link.py");
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink("__init__.py", &link).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file("__init__.py", &link).is_ok();
    if made {
        let error = oscope_ext::pack(&source, &one).unwrap_err().to_string();
        assert!(
            error.contains("\"hello/link.py\" is a symbolic link"),
            "{error}"
        );
        fs::remove_file(&link).unwrap();
    }
    fs::remove_file(source.join("panels/greet.json")).unwrap();
    let error = oscope_ext::pack(&source, &one).unwrap_err().to_string();
    assert!(
        error.contains("names \"panels/greet.json\", which isn't there"),
        "{error}"
    );
}
