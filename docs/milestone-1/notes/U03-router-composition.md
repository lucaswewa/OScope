# U03: Router composition, custom listeners, HTTP/2

- Status: In review
- Branch / PR: `u/u03-router-composition` in `teta-wot` / [lucaswewa/teta-wot#3](https://github.com/lucaswewa/teta-wot/pull/3)
- Size: estimated ~725, re-sized to L before starting (owner's decision, rather than splitting) · measured **867** hand-written changed lines (+844 −23). Docs and `Cargo.lock` aren't counted.

## Summary

A `teta-wot` server can now carry an application's own routes and layers, and serve over TLS with HTTP/2 ([spec](../phases.md#u03)).

- **Merging routes.** `merge` adds axum routes in front of the Things' routes. They get the server's credentials, CORS, layers and graceful shutdown.
- **Layers.** `layer` wraps every route.
- **Shutdown.** A `ShutdownToken` lets merged handlers that hold connections open end them when shutdown starts.
- **Listeners.** `serve_with_listener` serves on any axum `Listener`.
- **TLS** (feature `tls`) provides a `TlsListener` and `server_config`, with HTTP/2 negotiated by ALPN.

This is what OScope's media plane needs (plan §5.4): binary WebSockets, deep-zoom tiles and downloads beside the Things, under one authentication, and HTTP/2 for remote browsers (D33).

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **`router_with(runtime, options, routes, wrap, shutdown)`.** `router` now calls it with nothing merged.<ul><li>The merged routes get a credentials layer (`security::guard`), because the Things' routes check credentials inside their own dispatch, which merged routes don't go through.</li><li>Then `wrap` adds the application's layers around everything.</li><li>Then a `ShutdownToken` extension, and CORS outermost.</li></ul> | `crates/teta-wot-http/src/lib.rs`, `security.rs` |
| **`ShutdownToken`.** A `watch`-based token with `shut_down`, `cancelled` and `is_shutting_down`, which is also an extractor. Outside a server it never completes | `crates/teta-wot-http/src/shutdown.rs` |
| **The builder.** `ThingServerBuilder::merge(Router)`, `layer(L)` (axum's own bounds; later layers wrap earlier ones) and `shutdown_token()`. `ThingServer::shutdown_token()` | `crates/teta-wot-server/src/lib.rs` |
| **Serving.** `serve_with` (TCP, with mDNS as before) and the new `serve_with_listener<L: axum::serve::Listener>` share one serve loop.<ul><li>Shutdown triggers the token, which also ends axum's graceful wait, before closing the broker.</li><li>The log line for TCP is unchanged: `serving N Things on http://…`.</li></ul> | `crates/teta-wot-server/src/lib.rs` |
| **TLS** (feature `tls`):<ul><li>`server_config(certificate_chain_pem, private_key_pem)` builds a rustls configuration with the `ring` provider, offering `h2` then `http/1.1` by ALPN.</li><li>`TlsListener` accepts TCP connections and completes each handshake in its own task, with a 10-second limit, so one slow client can't stall the others. It closes its port when dropped.</li><li>`rustls` is re-exported.</li></ul> | `crates/teta-wot-server/src/tls.rs`; `teta_wot::server::tls` |
| **HTTP/2.** axum is built with its `http2` feature, so its server (hyper's automatic builder) speaks HTTP/2 to clients that negotiate it, and to plain-text (h2c) clients | `Cargo.toml` |
| **The example `custom-routes`:** a merged health check, a WebSocket of ticks that ends on shutdown, a layer, and a bearer token | `examples/custom-routes/` |
| **Docs:** a guide chapter, *Your own routes, layers and TLS*, and the CHANGELOG | `docs/guide/src/composition.md` |

## How to try it

```sh
cargo run -p custom-routes
```

It prints:

- the merged `/health`: `401` without the token, `200 ok` with it;
- the Thing's `/lamp/on`, `200`, stamped by the same layer;
- three ticks from the merged WebSocket;
- how long after the shutdown started the WebSocket closed and the server stopped: under a millisecond on the owner's PC.

For TLS: `cargo test -p teta-wot-server --features tls --test tls`.

## Design notes and deviations from the plan

- **Merged routes come first.** A request whose *path* they match goes to them, whatever the method, which is axum's own rule. That's simple and predictable, but a merged route can hide a Thing's route at the same path. The guide says not to do that. teta-wot can't detect it, because axum doesn't expose a router's paths.
- **Credentials apply to every merged route; there's no "public" opt-out.** U05's pluggable authentication will replace the guard's check, and can add one if OScope needs public merged routes.
- **Layer order:** the credentials guard is innermost, around the merged routes only. The application's layers wrap everything, then the shutdown extension, then CORS outermost. So even a `401` from a merged route carries CORS headers and the application's layers.
- **The TLS example is the test.** The spec lists "a TLS example with a self-signed certificate" under Tests. It's `tests/tls.rs`, which generates a certificate with `rcgen` and serves a Thing with it, rather than part of `custom-routes`: a TLS client would have doubled the example's size. The guide shows the server side.
- **ring, not aws-lc-rs.** rustls' default crypto provider, aws-lc-rs, needs CMake and NASM to build on Windows. Both are permissively licensed. `tokio-rustls` is used with `default-features = false` and `ring`, `tls12`.
- **`serve_with_listener` doesn't advertise over mDNS,** because advertising needs a TCP port; `serve_with` still does.
- **h2c.** HTTP/2 over plain TCP is now accepted too, because axum's server detects it. Browsers never use it, so nothing changes for them.
- **One dependency was pinned.** `rustls-pki-types` is held at 1.15.1, because 1.16.0 was released the day before (2026-11-10).
- **New crates, all checked:**
  - 45 new crates entered `Cargo.lock`. Each is at least two weeks old (the newest, `rustls-webpki`, is from 2026-10-20), and each is MIT, Apache-2.0, ISC or BSD-3-Clause.
  - `h2` (MIT) is now in every build.
  - rustls, `rustls-webpki`, ring and friends come only with `tls`.
  - The ASN.1, X.509 and `time` crates come from `rcgen` and are test-only.
- **Size:** 867 lines against ~725 (+20%), close to L's 900. The tests (254), the TLS module (150) and the example (161) each ran over their estimates.

## Tests

- **`tests/composition.rs`:**
  - **Credentials, CORS and layers.** Merged routes and a Thing's property both answer `401` without the bearer token. With it, both answer `200`, carry the layer's header, and echo the CORS origin.
  - **Shutdown.** A merged WebSocket, watching `ShutdownToken`, closes when the server shuts down, and the server stops within 10 seconds despite a 60-second grace period.
- **`tests/tls.rs`:**
  - **HTTP/2.** A client offering only `h2` gets it, and sends 20 concurrent requests over one connection, all `HTTP/2 200`.
  - **HTTP/1.1.** A client offering only `http/1.1` gets `HTTP/1.1 200`.
- **Mutation checks:**
  - Without the server triggering the token, the WebSocket test fails after its 10-second limit.
  - Without the guard on merged routes, the credentials test fails.
  - Both files were restored.
- **CI replayed locally:**

  | Job | Result |
  |---|---|
  | Ubuntu, a fresh LF clone of `6941b22` in WSL | All 44 steps passed (fmt, clippy, tests, docs, docs pages, 37 examples, the guide); 76 test suites ok |
  | Windows | All 44 steps passed, the guide included; 76 test suites ok |

## Follow-ups and known gaps

- **Public merged routes.** U05 should let an application mark merged routes public, or give the guard a principal, if OScope needs either.
- **Hidden Thing routes.** Overlaps between merged routes and Thing routes aren't detected (see above).
- **No certificate management.** A `TlsListener` takes its configuration once; reloading certificates without a restart, and generating them, are left to the application. OScope's P49 decides how its certificates are made.

Next phase: U04 (binary WebSocket streams).

## Clean room

All of it was written for this phase, against the axum, hyper, rustls and tokio-rustls documentation. Nothing was copied.
