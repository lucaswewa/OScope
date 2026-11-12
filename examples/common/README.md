# ex-common

What the examples share (P04). It is a library, not an example, so it has no `--smoke` of its own.

- **`Example::from_args(name, port)`** reads an example's command line:
  - **`--port N`:** the example listens on `N`. Its own port, from 8211 to 8219, is the default;
  - **`--smoke`:** the example runs headless on a port the system picks, with a temporary data root.

  Otherwise the data and scratch roots are under `.oscope/{name}/`. It also starts the example's logs:
  - **where:** the console, and daily files in the data root's `logs/`;
  - **what:** `info` and above, and OScope's own records at every level (`LOG_LEVEL`).
- **`Example::serve(api, page, check)`** serves the example's API with its logs, and its page folder at the same origin, through `oscope_api::ApiBuilder::pages`.
  - **Normally:** it prints the address to open, and serves until Ctrl-C.
  - **With `--smoke`:** it runs `check` against the server, stops it, and prints `smoke: ok` if the check passed.
- **`Example::from_args_with(name, port, options)`** also takes the example's own options, each a name and a number, such as `--runs 20`. `Example::number(option)` gives them.
- **`get(address, path)`** and **`post(address, path, json)`** are a small HTTP client for the smoke checks. They read binary answers too, such as images.

[`ex-server-hello`](../ex-server-hello/README.md), [`ex-logs`](../ex-logs/README.md) and [`ex-demosaic`](../ex-demosaic/README.md) use them.
