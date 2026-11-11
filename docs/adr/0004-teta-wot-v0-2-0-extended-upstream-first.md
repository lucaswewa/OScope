# ADR-0004: Depend on `teta-wot` v0.2.0, extended upstream first

- Status: Accepted
- Date: 2026-11-11
- Phase: P00

## Context

goal.md requires [`teta-wot`](https://github.com/lucaswewa/teta-wot) as OScope's HTTP interface. `teta-wot` is a Rust framework for W3C Web of Things servers, written by the owner.

### What `teta-wot` v0.1.0 provides

Its last release is v0.1.0, at commit 135f9ff. It provides:

- Things with typed properties, actions and events;
- Thing Descriptions and OpenAPI output;
- cancellable action invocations with logs;
- device actors and dependency injection;
- settings persistence;
- MJPEG streams;
- server-sent events and WebSocket observation;
- simple authentication.

### What it lacks for OScope

**For the media plane** (plan §5.4):

- **Fixed paths only.** Endpoints match fixed paths, with no path parameters, so deep-zoom tile URLs (`…/{level}/{col}_{row}.jpg`) don't fit.
- **No way to add the application's own routes.** An application can't merge axum routes and layers under the server's authentication, CORS and graceful shutdown. Running its own serve loop would duplicate `teta-wot`'s shutdown order.
- **Text-only streaming.** WebSockets and server-sent events carry JSON text only, so binary frames can't be sent. Each subscriber's buffer drops messages after five.
- **HTTP/1.1 only.** Browsers open at most six connections per server, which throttles tile loading.

**For security and platforms:**

- **Simple authentication only:** one bearer token or one Basic login, with no principal visible to affordances and no hook for authorisation. Accounts and the control lease (plan §5.18) can't be built on it.
- **Windows only:** Windows is the only supported and CI-tested platform.
- **Contradictory licence:** its `LICENSE` file says MIT, while `Cargo.toml` and the guide say it has no licence.

**Two gaps that OScope handles itself:**

- **Jobs.** Actions have no progress, queue or persistence. OScope's job service (D28) handles long work, and its actions return a `JobRef`.
- **Things are fixed when the server is built,** so extensions load at start-up (D11).

### The owner's choice

While planning, the owner chose to close the first set of gaps upstream before OScope depends on `teta-wot` (D27).

## Decision

**1. Stage 0** of Milestone 1 makes six phases in the `teta-wot` repository, each with its own pull request and approval:

| Phase | Change |
|---|---|
| U01 | Linux support and Ubuntu CI; the licence metadata; a `CLAUDE.md` for `teta-wot` |
| U02 | Path parameters for endpoints |
| U03 | Router composition, custom listeners such as TLS, and HTTP/2 over TLS |
| U04 | Binary WebSocket streams: the newest message wins, with parameters per subscriber |
| U05 | Pluggable authentication and authorisation |
| U06 | Release v0.2.0, following `teta-wot`'s release checklist, 24-hour soak included |

Their specs are in [`phases.md`](../milestone-1/phases.md#stage-0-teta-wot-v020-upstream).

**2. Pinning.** OScope pins the release:

```toml
teta-wot = { git = "https://github.com/lucaswewa/teta-wot", tag = "v0.2.0" }
```

- **Only released tags on `main`.** OScope's `main` always pins a released tag.
- **Testing an unreleased fix** is done through a temporary `[patch]` pointing at a local `teta-wot`, and only on a branch. The fix then ships as a v0.2.x release.

**3. One crate depends on `teta-wot`.**

- Only `oscope-api` does (plan §5.2).
- It uses `teta-wot`'s re-export of axum rather than a separate axum dependency, so their versions can't drift apart.

**4. Compatibility.**

- Upstream changes are additive where possible.
- A breaking change gets migration notes, as `teta-wot`'s release checklist requires.
- `microscope` pins v0.1.0 and is unaffected.

**5. Where things are recorded.**

- Upstream phases' notes, with their design decisions, are kept in OScope: `docs/milestone-1/notes/UNN-short-name.md`.
- User-facing changes go into `teta-wot`'s CHANGELOG and guide.

**6. If an upstream feature stalls,** OScope may mount a temporary equivalent of its own. A new ADR records that, and a later phase removes it once the feature is released.

## Consequences

- **Gains:**
  - Generic features land where every `teta-wot` user benefits, and OScope's interface crate stays thin.
  - Each upstream feature is proven by its own `teta-wot` example and tests before OScope relies on it.
- **Costs:**
  - P01 waits for Stage 0, including a release with a 24-hour soak.
  - Work is coordinated across two repositories.
  - OScope can't use a `teta-wot` change until it is released.

## Alternatives considered

- **Pin v0.1.0 and work around the gaps inside OScope,** with its own serve loop and routes: no upstream dependency, but OScope would duplicate `teta-wot` internals that drift on every upgrade.
- **Release each feature as soon as it is ready** (v0.2, v0.3, …): less waiting up front, but more switching between repositories and more releases.
- **Track `teta-wot`'s `main` by git revision:** the fastest loop, but builds aren't reproducible from a release.
