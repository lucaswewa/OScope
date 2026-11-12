# ADR-0006: Control plane and media plane

- Status: Accepted
- Date: 2026-11-12
- Phase: P04

## Context

OScope's clients carry two kinds of traffic. The clients are the M2 web and desktop apps, the examples' pages, and scripts.

- **Control:** reading and setting the instrument's state, starting work, and following events. It should be typed and self-describing. M2 generates its TypeScript types from the OpenAPI document. `teta-wot`'s Things provide all of this: Thing Descriptions, properties, actions, events, and an OpenAPI document.
- **Media:** preview frames, deep-zoom tiles, volume bricks, downloads and uploads (plan §5.4). These are binary, high-rate, or plain HTTP with paths of their own, such as `{dataset}_files/{level}/{col}_{row}.jpg`, or `Range` requests. Things don't model them well.

Three things constrain the design:

- **`teta-wot` v0.2.0 can merge an application's axum routes** in front of the Things' routes (U03). They get the same credentials check or authenticator (U05), CORS, layers and graceful shutdown. Its `ShutdownToken` tells a handler that holds a connection open when to end it.
- **ADR-0005:** only `oscope-api` may depend on `teta-wot`, and it reaches axum only through `teta-wot`'s re-export.
- **The examples' pages** need the API at the same origin, with no CORS and no second server.

## Decision

1. **One server, one origin.** `oscope-api`'s `Api` builds a single `teta-wot` server, which serves:

   | Part | Where | What |
   |---|---|---|
   | **The control plane** | `/api/v1/{thing}/…` | The Things. A Thing's Description is at `/api/v1/{thing}/`. `teta-wot` serves the OpenAPI document at `/openapi.json` and its docs pages at `/docs` and `/redoc`, at the root. |
   | **The media plane** | `/api/v1/media/…` | OScope's own axum routes, merged into the server. The downloads and uploads of §5.4 will be merged the same way. |
   | **Health** | `/health` | `{"status": "ok"}` while the server answers. The instrument's health is the `status` Thing's (§5.19). |
   | **Pages** | `/` and each file's own path | A folder's files, each read when it is asked for. The examples use this, and M2's web build can. A file whose path the API uses is refused: `health`, `openapi.json`, `docs`, `redoc`, and anything under `api/`, `docs/` or `.well-known/`. |

2. **The media plane, health and pages share the Things' authentication, CORS and shutdown.** With an authenticator, every one of their requests is checked. Thing Descriptions and the OpenAPI document stay public, so that clients can find out what to send.
3. **A media handler that holds a connection open watches the `ShutdownToken`.** It ends the connection as soon as the shutdown starts: a WebSocket closes with code 1001, going away. Shutting down then takes milliseconds, not the 5 s grace period.
4. **The API describes itself from code.** `oscope-server --print-openapi` and `--print-td` print the OpenAPI document and the Thing Descriptions without serving. M2 generates its types from them, and reviewers can read them.
5. **`oscope-server` owns the configuration** and reaches `teta-wot` only through `oscope-api`.
   - **The file** is TOML (plan §13, point 9), with three tables: `[server]` (address and port), `[storage]` (data and scratch roots) and `[devices]` (kept for P16).
   - **Unknown keys are refused,** with the line.
   - **Relative folders are relative to the file.** The command line overrides the port and the data root.
   - **The defaults:** it listens on `127.0.0.1:8210`, which only this computer can reach (D34, §5.18).

## Consequences

- **Easier:**
  - one origin, so neither the apps nor the examples need CORS;
  - one authentication path and one shutdown;
  - generated client types for the whole control plane.
- **The media plane isn't in the OpenAPI document or the Thing Descriptions.** Each media route is documented where it is built, as P17 will document the preview protocol. Clients write their media code by hand.
- **Browsers can't authenticate merged WebSockets yet.** The guard on merged routes reads the `Authorization` header, and browsers can't set headers on a WebSocket. `teta-wot`'s first-message authentication (`FirstMessageAuth`) is given only to a Thing's custom WebSocket endpoints. Authentication (ADR-0031) must choose one of these:
  - an upstream change, so that merged routes can use first-message authentication;
  - the preview served as a custom endpoint of a Thing;
  - OScope's authenticator letting a media upgrade through, with the handler checking the first message itself.
- **With an authenticator, `/health` and the pages need credentials too,** since every merged route is checked. OScope's authenticator sees each request's path, so it can let those through by path.
- **Pages can't use the API's paths,** and the server refuses to start if a page would hide one.

## Alternatives considered

- **A second server, on its own port, for media.** It would need a second authentication path, CORS between two origins, and its own shutdown. Not chosen.
- **Media as custom endpoints and binary streams of Things** (U02, U04). Each route would sit under a Thing's path, and generic HTTP is simpler with axum: tiles with structured file names, and downloads with `Range`. A single stream may still be a Thing's binary stream where that fits.
- **Things at the root, with no prefix.** Their paths would mix with pages and media. `/api/v1` keeps a versioned API apart from them.
- **`teta-wot`'s JSON configuration file.** It names Things to build from a registry, while OScope builds its Things in code from its own configuration. TOML is easier to write by hand, with comments.
