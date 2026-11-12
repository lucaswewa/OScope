//! What the examples share: serving an example's page beside OScope's API,
//! at one origin, and its `--smoke` checks over HTTP.
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

use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use oscope_api::{ApiBuilder, Roots};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
    _temporary: Option<tempfile::TempDir>,
}

impl Example {
    /// Reads the command line: `--port N` and `--smoke`.
    pub fn from_args(name: &str, port: u16) -> anyhow::Result<Self> {
        let mut args = std::env::args().skip(1);
        let (mut chosen, mut smoke) = (None, false);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => smoke = true,
                "--port" => chosen = Some(args.next().unwrap_or_default().parse()?),
                other => anyhow::bail!("`{other}`: the options are --port N and --smoke"),
            }
        }
        let temporary = smoke.then(tempfile::tempdir).transpose()?;
        let folder = match &temporary {
            Some(folder) => folder.path().to_owned(),
            None => PathBuf::from(".oscope").join(name),
        };
        Ok(Self {
            name: name.to_owned(),
            port: chosen.unwrap_or(if smoke { 0 } else { port }),
            smoke,
            roots: Roots {
                data: folder.join("data"),
                scratch: folder.join("scratch"),
            },
            _temporary: temporary,
        })
    }

    /// Serves `api`, with the files in `page` at the same origin. Without
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
        let api = api.pages(page.as_ref()).build()?;
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
    /// Its body, as text.
    pub body: String,
}

impl Answer {
    /// The body, as JSON.
    pub fn json(&self) -> anyhow::Result<serde_json::Value> {
        serde_json::from_str(&self.body).context("a JSON body")
    }
}

/// `GET path` from the server at `address`, over HTTP/1.1, for answers
/// with a length (not chunked), as the API's are.
pub async fn get(address: SocketAddr, path: &str) -> anyhow::Result<Answer> {
    let mut stream = TcpStream::connect(address).await?;
    let request = format!("GET {path} HTTP/1.1\r\nhost: {address}\r\nconnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut text = String::new();
    stream.read_to_string(&mut text).await?;
    let (head, body) = text.split_once("\r\n\r\n").context("an HTTP answer")?;
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok());
    let content_type = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-type")
            .then(|| value.trim().to_owned())
    });
    Ok(Answer {
        status: status.context("a status code")?,
        content_type: content_type.unwrap_or_default(),
        body: body.to_owned(),
    })
}
