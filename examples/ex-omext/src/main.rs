//! `ex-omext`: packs a dummy extension, installs it through the API as a
//! client would, and lists it. Then it serves a page for installing,
//! enabling, disabling and uninstalling extensions, and starts its server
//! again when the `system` Thing's `restart` asks: what loads them.

use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, ensure};
use ex_common::{Answer, Example, get, post, post_bytes};
use oscope_api::Api;
use oscope_ext::Extensions;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;

/// The example's port: the first of the second range for manual checks.
const PORT: u16 = 8230;

/// The dummy extension's ID.
const ID: &str = "org.oscope.examples.hello";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let example = Example::from_args("ex-omext", PORT)?;
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    for folder in [&example.roots.data, &example.roots.scratch] {
        std::fs::create_dir_all(folder).with_context(|| folder.display().to_string())?;
    }
    let package = example.roots.scratch.join(format!("{ID}-0.1.0.omext"));
    let manifest = oscope_ext::pack(&here.join("hello"), &package)?;
    let sha256 = oscope_ext::sha256(&package)?;
    println!(
        "ex-omext: packed {ID} {} into {}",
        manifest.version,
        package.display()
    );
    println!("ex-omext: its SHA-256 is {sha256}");
    let package = std::fs::read(&package)?;

    let listener = TcpListener::bind(("127.0.0.1", example.port))
        .await
        .with_context(|| format!("can't listen on port {}", example.port))?;
    let address = listener.local_addr()?;
    println!("ex-omext: open http://{address}/");
    let (stop, stopped) = watch::channel(false);
    let checks = match example.smoke {
        true => Some(tokio::spawn(async move {
            let checked = smoke(address, &here.join("hello"), package).await;
            let _ = stop.send(true);
            checked
        })),
        false => {
            tokio::spawn(async move {
                if let Err(error) = tour(address, &package).await {
                    eprintln!("ex-omext: {error:#}");
                }
            });
            tokio::spawn(async move {
                oscope_api::shutdown_signal().await;
                let _ = stop.send(true);
            });
            None
        }
    };
    serve(&example, &here.join("page"), listener, stopped).await?;
    if let Some(checks) = checks {
        checks.await??;
        println!("smoke: ok");
    }
    Ok(())
}

/// Serves the API and the page on `listener` until `stopped`. Each time the
/// `system` Thing's `restart` asks, it builds the server again, as a start
/// does: what loads the installed extensions. It listens again on the same
/// port, as soon as the system lets it.
async fn serve(
    example: &Example,
    page: &Path,
    mut listener: TcpListener,
    stopped: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let address = listener.local_addr()?;
    loop {
        let extensions = Extensions::open(example.roots.data.join("extensions"))?;
        let api = Api::builder(&example.roots)
            .logs(example.logs.clone())
            .pages(page)
            .extensions(extensions)
            .build()?;
        let restart = api.restart();
        let (asked, mut until) = (Arc::clone(&restart), stopped.clone());
        let shutdown = async move {
            tokio::select! {
                () = async { let _ = until.wait_for(|stop| *stop).await; } => {}
                () = asked.requested() => {}
            }
        };
        api.serve(listener, shutdown).await?;
        if !restart.is_requested() || *stopped.borrow() {
            return Ok(());
        }
        println!("ex-omext: starting again, as asked");
        let deadline = Instant::now() + Duration::from_secs(10);
        listener = loop {
            match TcpListener::bind(address).await {
                Ok(listener) => break listener,
                Err(error) if Instant::now() > deadline => {
                    return Err(error).context(format!("can't listen on {address} again"));
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        };
    }
}

/// Posts a package to the upload route.
async fn install(address: SocketAddr, package: &[u8]) -> anyhow::Result<Answer> {
    let path = "/api/v1/extensions/install";
    post_bytes(address, path, "application/octet-stream", package).await
}

/// An extension, in a line.
fn line(extension: &Value) -> String {
    let manifest = &extension["manifest"];
    let version = |v: &Value| v.as_str().map_or_else(|| "none".to_owned(), str::to_owned);
    format!(
        "{} {} by {}: loaded {}, the next start {}",
        manifest["id"].as_str().unwrap_or("?"),
        manifest["version"].as_str().unwrap_or("?"),
        manifest["publisher"].as_str().unwrap_or("?"),
        version(&extension["loaded"]),
        version(&extension["next"]),
    )
}

/// Installs the package as a client would, and lists what is installed.
async fn tour(address: SocketAddr, package: &[u8]) -> anyhow::Result<()> {
    let installed = install(address, package).await?;
    let said = match installed.status {
        201 => "installed".to_owned(),
        200 => "installed already".to_owned(),
        status => format!("refused, {status}: {}", installed.body),
    };
    println!("ex-omext: POST /api/v1/extensions/install: {said}");
    let listed = get(address, "/api/v1/extensions/installed").await?.json()?;
    for extension in listed.as_array().into_iter().flatten() {
        println!("ex-omext:   {}", line(extension));
    }
    if get(address, "/api/v1/extensions/restart_needed")
        .await?
        .json()?
        == true
    {
        println!("ex-omext: a restart loads it: the page's Start again, or");
        println!("          curl -X POST http://{address}/api/v1/system/restart");
    }
    Ok(())
}

/// A zip file of `entries`, each a name and its bytes.
fn zip_of(entries: &[(&str, &[u8])]) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// The `--smoke` checks: installed, then again unchanged; refused for
/// another API and for an entry that would land outside its folder; then
/// loaded after a restart, with the page served.
async fn smoke(address: SocketAddr, source: &Path, package: Vec<u8>) -> anyhow::Result<()> {
    let first = install(address, &package).await?;
    ensure!(
        first.status == 201,
        "installed: {} {}",
        first.status,
        first.body
    );
    let again = install(address, &package).await?;
    ensure!(
        again.status == 200 && again.json()? == first.json()?,
        "again: {}",
        again.body
    );

    let manifest = std::fs::read_to_string(source.join("manifest.toml"))?;
    let newer = manifest.replace("oscope-api = \"^1.0\"", "oscope-api = \"^9.0\"");
    let module = std::fs::read(source.join("hello/__init__.py"))?;
    let panel = std::fs::read(source.join("panels/greet.json"))?;
    let refusals = [
        (
            zip_of(&[
                ("manifest.toml", newer.as_bytes()),
                ("hello/__init__.py", &module),
                ("panels/greet.json", &panel),
            ])?,
            "it needs the extension API ^9.0",
        ),
        (
            zip_of(&[
                ("manifest.toml", manifest.as_bytes()),
                ("../escaped.txt", b"out"),
            ])?,
            "\\\"../escaped.txt\\\" leaves the package",
        ),
    ];
    for (bytes, said) in refusals {
        let refused = install(address, &bytes).await?;
        ensure!(
            refused.status == 400 && refused.body.contains(said),
            "{said}: {}",
            refused.body
        );
    }
    let listed = get(address, "/api/v1/extensions/installed").await?.json()?;
    ensure!(
        listed[0]["next"] == "0.1.0" && listed[0]["loaded"].is_null(),
        "{listed}"
    );

    let restart = post(address, "/api/v1/system/restart", &json!({})).await?;
    ensure!(
        restart.status == 201,
        "restart: {} {}",
        restart.status,
        restart.body
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let listed = get(address, "/api/v1/extensions/installed").await;
        if let Ok(answer) = listed
            && answer
                .json()
                .is_ok_and(|listed| listed[0]["loaded"] == "0.1.0")
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "not loaded 30 s after the restart"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let provided = get(address, "/api/v1/extensions/provided").await?.json()?;
    ensure!(
        provided["nodes"]["hello.greet"] == ID,
        "provided: {provided}"
    );
    let page = get(address, "/").await?;
    ensure!(
        page.status == 200 && page.body.contains("Extensions"),
        "the page"
    );
    ensure!(
        get(address, "/app.js").await?.status == 200,
        "the page's script"
    );
    Ok(())
}
