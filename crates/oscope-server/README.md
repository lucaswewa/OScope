# oscope-server

The program that runs the microscope. It reads its configuration, then serves OScope's API until it is asked to stop:

- **the control plane:** the Things, under `/api/v1/`;
- **the media plane:** under `/api/v1/media/`;
- **`/health`.**

For now there are two Things, `system` and `logs`:

- **`system`:** OScope's version and Git commit, the operating system, the processor, the memory, the uptime, and the data and scratch roots with their free space;
- **`logs`:** the server's log, with filters and paging, the daily files, and the live tail at `/api/v1/media/logs`.

[ADR-0006](../../docs/adr/0006-control-plane-and-media-plane.md) describes the layout, and [ADR-0009](../../docs/adr/0009-logging.md) the logs.

## How to try it

From a clean checkout, with [rustup](https://rustup.rs/) installed:

```sh
cargo run -p oscope-server
```

The first run installs the pinned toolchain and fetches the dependencies. The server then creates its folders, `.oscope/data` and `.oscope/scratch`, and prints:

```text
oscope-server 0.1.0 serving on http://127.0.0.1:8210/
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
| `--version`, `--help` | Prints the version or the usage |

**Exit codes:**
- 2 for a wrong command line or configuration file, a wrong log level among them, with a message that says what is wrong and where;
- 1 if the server can't serve, for example because the port is taken.

## The configuration file

[`configs/oscope.toml`](../../configs/oscope.toml) is a sample. Every key is optional:

```toml
[server]
address = "127.0.0.1"             # only this computer can connect
port = 8210

[storage]
data_root = ".oscope/data"        # projects and their data
scratch_root = ".oscope/scratch"  # frame logs and caches

[logging]
level = "info"                    # tracing's EnvFilter syntax, such as "info,oscope_api=debug"
ring = 50000                      # records kept in memory, for queries and the tail
keep_days = 30                    # days of daily files kept, in data_root/logs

[devices]                         # kept for the device Things (P16)
```

- **Relative folders** are relative to the file's own folder.
- **An unknown key** is refused, with its line.

```sh
cargo run -p oscope-server -- --config configs/oscope.toml --port 8211
```
