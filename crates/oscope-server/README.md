# oscope-server

The program that runs the microscope. It reads its configuration, then serves OScope's API until it is asked to stop:

- **the control plane:** the Things, under `/api/v1/`;
- **the media plane:** under `/api/v1/media/`;
- **`/health`.**

For now there are two Things, `system` and `logs`:

- **`system`:** OScope's version and Git commit, the operating system, the processor, the memory, the uptime, the data and scratch roots with their free space, and the scratch space's health;
- **`logs`:** the server's log, with filters and paging, the daily files, and the live tail at `/api/v1/media/logs`.

[ADR-0006](../../docs/adr/0006-control-plane-and-media-plane.md) describes the layout, and [ADR-0009](../../docs/adr/0009-logging.md) the logs.

## How to try it

From a clean checkout, with [rustup](https://rustup.rs/) installed:

```sh
cargo run -p oscope-server
```

The first run installs the pinned toolchain and fetches the dependencies. The server then creates its folders, `.oscope/data` and `.oscope/scratch`, and prints:

```text
oscope-server 0.1.1 serving on http://127.0.0.1:8210/
```

In another terminal:

```sh
curl http://127.0.0.1:8210/health                   # {"status":"ok"}
curl http://127.0.0.1:8210/api/v1/system/           # the system Thing's Thing Description
curl http://127.0.0.1:8210/api/v1/system/memory     # {"total_bytes":…,"available_bytes":…}
curl http://127.0.0.1:8210/api/v1/media/ping        # pong
curl "http://127.0.0.1:8210/api/v1/logs/records?limit=5"   # the five newest records
```

**The logs** go to stderr, and to daily files in `.oscope/data/logs/`, one record per line as JSON.

Or open `http://127.0.0.1:8210/docs` for the OpenAPI document's pages. Ctrl-C stops the server.

[`ex-server-hello`](../../examples/ex-server-hello/README.md) shows the same Thing on a page.

## The command line

| Option | What it does |
|---|---|
| `--config FILE` | Reads the configuration from a TOML file. Without it, the defaults |
| `--port N` | Listens on port `N` in place of the configuration's. 0 lets the system choose, and the serving line says which |
| `--data-root FOLDER` | Uses another data root, relative to where you run the program |
| `--print-openapi` | Prints the OpenAPI document and exits |
| `--print-td` | Prints each Thing's Thing Description, by name, and exits |
| `--managed` | Runs under a supervisor, such as the desktop app, by the sidecar contract below |
| `service install`, `uninstall`, `run` | Windows service mode, below |
| `ext pack FOLDER [--out FILE]` | Packs a folder with a `manifest.toml` into an extension's package, `{id}-{version}.omext` here unless `--out` names it, and prints its size and SHA-256. The same folder packs to the same bytes each time ([ADR-0018](../../docs/adr/0018-the-omext-package-format.md)) |
| `--version`, `--help` | Prints the version or the usage |

**Exit codes:**
- 0 when it stopped as asked;
- **75 when it stopped to be started again:** the `system` Thing's `restart` asks for it;
- 2 for a wrong command line or configuration file, a wrong log level among them, with a message that says what is wrong and where;
- 1 if the server can't serve, for example because the port is taken.

**Extensions** are installed in `{data_root}/extensions`, and load at start-up ([ADR-0018](../../docs/adr/0018-the-omext-package-format.md)): the `extensions` Thing installs, enables, disables and uninstalls them, each for the next start. A start finishes those changes first: it removes uninstalled extensions and versions replaced. Rust extensions' libraries load then ([ADR-0019](../../docs/adr/0019-the-rust-extension-abi.md)). Python extensions run with the configuration's `[python]` settings ([ADR-0020](../../docs/adr/0020-the-python-extension-runtime.md)): installing one makes its environment with uv, so uv must be installed; its worker starts on the first run of one of its nodes. The configuration's `[devices]` may name the cameras, stages and lights the extensions provide, as `camera = "example.fakecam"`, so the server opens extensions before devices ([ADR-0036](../../docs/adr/0036-extension-drivers-and-panels.md)). The `extensions` Thing serves their panels.

**The job history** is kept in `{data_root}/index.sqlite` ([ADR-0016](../../docs/adr/0016-the-job-service.md)). Jobs a stop cut short are marked *interrupted* at the next start.

**Projects** are kept in `{data_root}/projects`, each in a folder with its acquisitions, results and reports, their images as OME-Zarr v3, and listed in `{data_root}/index.sqlite` beside the jobs ([ADR-0021](../../docs/adr/0021-projects-ome-zarr-and-the-index.md)). The `projects` Thing lists, searches and pages them for the Gallery; makes, renames, tags, describes and deletes them; and serves their thumbnails. Its `rebuild_index` action makes the index again from the folders, which are the truth.

**Recordings** of frames as the camera gives them go to `{scratch_root}/recordings`, in segment files with an index, until a `projects.ingest` job writes them into a project's acquisition and deletes them ([ADR-0022](../../docs/adr/0022-scratch-recording-and-ingest.md)). When the server starts, it empties `{scratch_root}/tmp` and recovers the recordings a crash cut short. The `system` Thing's `scratch` property says whether there is room to record.

**Captures and files** (P30, [ADR-0037](../../docs/adr/0037-importers-exporters-and-the-abi-s-third-version.md)):

- **The `capture` Thing** takes a single frame, or a burst of 1 to 10,000, into a new acquisition, as a job, with a display rendition if asked.
- **Imports and exports:** the `projects.import` job reads TIFF, its stacks, PNG and JPEG into acquisitions, and `projects.export` writes OME-TIFF, 16-bit TIFF and PNG. `GET /api/v1/projects/formats` lists the formats, the extensions' among them.
- **Downloads:** `GET /api/v1/data/{project}/{path}` serves a project's files, with Range requests.

**Each start writes a new local token,** 32 random bytes in hex, to `{data_root}/run/local.token`. Only the account the server runs as can read it: mode 0600 on Linux, a one-entry access list on Windows. It is for the desktop app, which proves itself with it once authentication comes (ADR-0031).

## The sidecar contract

`--managed` is how a supervisor, such as the desktop app, runs the server ([ADR-0015](../../docs/adr/0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md)):

- **It listens on any free port,** unless `--port` gives one.
- **Its only line on stdout,** once it is ready, is JSON; its logs go to stderr:

  ```json
  {"ready":true,"version":"0.1.1","port":53647,"pid":14316,"token":"C:\\oscope\\data\\run\\local.token","shm":"preview-14316"}
  ```

- **Closing its stdin stops it,** as Ctrl-C does.
- **Exit code 75 asks to be started again;** 0 means it stopped as asked.
- **It publishes previews to a shared-memory ring,** named in `shm`, for the desktop app's fast path.

[`ex-sidecar`](../../examples/ex-sidecar/README.md) is a small supervisor; [`ex-shm-reader`](../../examples/ex-shm-reader/README.md) reads the ring.

## Service mode

**On Windows,** from an administrator's console:

```powershell
oscope-server --config C:\oscope\oscope.toml service install    # registers the service "oscope"
Start-Service oscope
Stop-Service oscope
oscope-server service uninstall
```

- **`install` records** the configuration, the data root as an absolute path, and `--port` if given. `--name` names the service.
- **The service runs as LocalSystem,** started on demand, and is started again 2 s after it fails or exits with 75.
- **`service run`** is what the service manager starts.

**On Linux,** systemd runs the server as it is: [`packaging/linux/oscope.service`](../../packaging/linux/oscope.service), with a configuration such as [`packaging/linux/oscope.toml`](../../packaging/linux/oscope.toml). [`packaging/README.md`](../../packaging/README.md) says how to install both, and how to try each service.

## The configuration file

[`configs/oscope.toml`](../../configs/oscope.toml) is a sample. Every key is optional:

```toml
version = 2                       # the file's version (P52)

[network]
address = "127.0.0.1"             # only this computer can connect
port = 8210

[storage]
data_root = ".oscope/data"        # projects and their data
scratch_root = ".oscope/scratch"  # frame logs and caches

[logging]
level = "info"                    # tracing's EnvFilter syntax, such as "info,oscope_api=debug"
ring = 50000                      # records kept in memory, for queries and the tail
keep_days = 30                    # days of daily files kept, in data_root/logs

[devices]                         # each device's driver (P16)

[simulator]                       # the simulator's settings (P16)
sample = "H&E tissue"

[jobs]                            # the job service (P20)
workers = 2                       # compute jobs at once
gpu_mib = 6000                    # GPU memory they share; not counted when not given

[shm]                             # the desktop app's preview ring (P19)
enabled = true                    # by default only with --managed
name = "preview"                  # by default; preview-{pid} with --managed
slots = 3
slot_bytes = 8388608              # the largest preview

[python]                          # Python extensions (P26)
uv = "C:/tools/uv.exe"            # found when not given: OSCOPE_UV, then PATH, then ~/.local/bin
sdk = "python/oscope-ext"         # the SDK's source; this source tree's by default
wheelhouse = "wheels"             # install from these wheels alone, offline
call_timeout_s = 120              # how long a node may run
idle_s = 300                      # how long an idle worker is kept
```

- **Relative folders** are relative to the file's own folder.
- **An unknown key** is refused, with its line.
- **Versions (P52):**
  - A file without `version` is version 1's, as P01–P51 wrote them: `[server]` for `[network]`, and `[devices.simulator]` for `[simulator]`. It is still read, migrated as it is read.
  - `oscope-server --config FILE config migrate` rewrites it as version 2's. It keeps the file's comments, and the old file beside it as `FILE.v1.bak`.
  - **The `settings` Thing** changes the file too. It writes only what changed, and keeps the file before as `FILE.bak`.

```sh
cargo run -p oscope-server -- --config configs/oscope.toml --port 8211
```
