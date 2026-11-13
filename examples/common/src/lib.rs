//! What the examples share: serving an example's page beside OScope's API,
//! at one origin, and its `--smoke` checks over HTTP; and, for the examples
//! that supervise a server, running one as a [`sidecar`].
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! let example = ex_common::Example::from_args("ex-hello", 8211)?;
//! let api = oscope_api::Api::builder(&example.roots);
//! example.serve(api, "page", |address| async move {
//!     anyhow::ensure!(ex_common::get(address, "/health").await?.status == 200);
//!     Ok(())
//! }).await
//! # }
//! ```

use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use oscope_api::logs::{self, LogConfig, LogStore};
use oscope_api::{ApiBuilder, Roots};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub mod sidecar;

/// What examples record: `info` and above, and OScope's own records at
/// every level, so that a `trace` note written with the `logs` Thing's
/// `emit` is kept.
pub const LOG_LEVEL: &str = "info,oscope=trace";

/// An example, as its command line asks it to run.
#[derive(Debug)]
pub struct Example {
    /// Its name, such as `ex-server-hello`.
    pub name: String,
    /// Its port: its own, `--port N`, or one the system picks for
    /// `--smoke`.
    pub port: u16,
    /// Whether it runs its `--smoke` checks, then exits.
    pub smoke: bool,
    /// Its data and scratch roots, under `.oscope/{name}/`, or for
    /// `--smoke` under a temporary folder that is removed at the end.
    pub roots: Roots,
    /// Its logs, at [`LOG_LEVEL`], on the console and in daily files in
    /// `{data root}/logs`. Its server's `logs` Thing serves them.
    pub logs: LogStore,
    numbers: BTreeMap<String, usize>,
    _temporary: Option<tempfile::TempDir>,
}

impl Example {
    /// Reads the command line, `--port N` and `--smoke`, and starts the
    /// logs.
    pub fn from_args(name: &str, port: u16) -> anyhow::Result<Self> {
        Self::from_args_with(name, port, &[])
    }

    /// As [`from_args`](Self::from_args), and the example's own options,
    /// each a name and a number, such as `--runs 20`; [`number`](Self::number)
    /// gives them.
    pub fn from_args_with(name: &str, port: u16, numbers: &[&str]) -> anyhow::Result<Self> {
        let mut args = std::env::args().skip(1);
        let (mut chosen, mut smoke, mut given) = (None, false, BTreeMap::new());
        while let Some(arg) = args.next() {
            let mut value = || args.next().with_context(|| format!("{arg} needs a number"));
            match arg.as_str() {
                "--smoke" => smoke = true,
                "--port" => chosen = Some(value()?.parse()?),
                own if numbers.contains(&own) => {
                    given.insert(own.to_owned(), value()?.parse()?);
                }
                other => {
                    anyhow::bail!("`{other}`: the options are --port N, --smoke, and {numbers:?}")
                }
            }
        }
        let temporary = smoke.then(tempfile::tempdir).transpose()?;
        let folder = match &temporary {
            Some(folder) => folder.path().to_owned(),
            None => PathBuf::from(".oscope").join(name),
        };
        let roots = Roots {
            data: folder.join("data"),
            scratch: folder.join("scratch"),
        };
        let logs = LogStore::new(&LogConfig {
            folder: Some(roots.data.join("logs")),
            ..LogConfig::default()
        })?;
        logs::install(&logs, LOG_LEVEL)?;
        Ok(Self {
            name: name.to_owned(),
            port: chosen.unwrap_or(if smoke { 0 } else { port }),
            smoke,
            roots,
            logs,
            numbers: given,
            _temporary: temporary,
        })
    }

    /// The number given with one of the example's own options, if it was.
    pub fn number(&self, option: &str) -> Option<usize> {
        self.numbers.get(option).copied()
    }

    /// Serves `api`, with the example's logs and the files in `page` at the
    /// same origin. Without
    /// `--smoke` it serves until Ctrl-C. With it, it serves until `check`,
    /// given the server's address, finishes, and returns its result.
    pub async fn serve<C, F>(
        self,
        api: ApiBuilder,
        page: impl AsRef<Path>,
        check: C,
    ) -> anyhow::Result<()>
    where
        C: FnOnce(SocketAddr) -> F,
        F: Future<Output = anyhow::Result<()>>,
    {
        for folder in [&self.roots.data, &self.roots.scratch] {
            std::fs::create_dir_all(folder).with_context(|| folder.display().to_string())?;
        }
        let api = api.logs(self.logs.clone()).pages(page.as_ref()).build()?;
        let listener = TcpListener::bind(("127.0.0.1", self.port))
            .await
            .with_context(|| format!("can't listen on port {}", self.port))?;
        let address = listener.local_addr()?;
        println!("{}: open http://{address}/", self.name);
        if !self.smoke {
            return Ok(api.serve(listener, oscope_api::shutdown_signal()).await?);
        }
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(api.serve(listener, async {
            let _ = stopped.await;
        }));
        let checked = check(address).await;
        let _ = stop.send(());
        serving.await??;
        checked?;
        println!("smoke: ok");
        Ok(())
    }
}

/// An answer to [`get`].
#[derive(Debug, Clone)]
pub struct Answer {
    /// Its status code.
    pub status: u16,
    /// Its `Content-Type`, or empty.
    pub content_type: String,
    /// Its body, as text; bytes that aren't UTF-8, as in an image, are
    /// replaced.
    pub body: String,
}

impl Answer {
    /// The body, as JSON.
    pub fn json(&self) -> anyhow::Result<Value> {
        serde_json::from_str(&self.body).context("a JSON body")
    }
}

/// `GET path` from the server at `address`, over HTTP/1.1, for answers
/// with a length (not chunked), as the API's are.
pub async fn get(address: SocketAddr, path: &str) -> anyhow::Result<Answer> {
    request(address, "GET", path, None).await
}

/// `POST path` with a JSON body, as [`get`].
pub async fn post(address: SocketAddr, path: &str, body: &Value) -> anyhow::Result<Answer> {
    request(address, "POST", path, Some(body)).await
}

/// `PUT path` with a JSON body, as [`get`].
pub async fn put(address: SocketAddr, path: &str, body: &Value) -> anyhow::Result<Answer> {
    request(address, "PUT", path, Some(body)).await
}

async fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> anyhow::Result<Answer> {
    let mut stream = TcpStream::connect(address).await?;
    let body = body.map(Value::to_string).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\n\
         content-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    // The head is text; the body may be an image.
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("an HTTP answer")?;
    let head = String::from_utf8_lossy(&bytes[..end]);
    let body = String::from_utf8_lossy(&bytes[end + 4..]);
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok());
    let content_type = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-type")
            .then(|| value.trim().to_owned())
    });
    Ok(Answer {
        status: status.context("a status code")?,
        content_type: content_type.unwrap_or_default(),
        body: body.into_owned(),
    })
}
