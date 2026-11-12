# ex-common

What the examples share (P04). It is a library, not an example, so it has no `--smoke` of its own.

- **`Example::from_args(name, port)`** reads an example's command line:
  - **`--port N`:** the example listens on `N`. Its own port, from 8211 to 8219, is the default;
  - **`--smoke`:** the example runs headless on a port the system picks, with a temporary data root.

  Otherwise the data and scratch roots are under `.oscope/{name}/`.
- **`Example::serve(api, page, check)`** serves the example's API with its page folder at the same origin, through `oscope_api::ApiBuilder::pages`.
  - **Normally:** it prints the address to open, and serves until Ctrl-C.
  - **With `--smoke`:** it runs `check` against the server, stops it, and prints `smoke: ok` if the check passed.
- **`get(address, path)`** is a small HTTP client for the smoke checks.

[`ex-server-hello`](../ex-server-hello/README.md) uses all three.
