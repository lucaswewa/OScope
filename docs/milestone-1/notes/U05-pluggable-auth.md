# U05: Pluggable authentication

- Status: Done
- Branch / PR: `u/u05-pluggable-auth` in `teta-wot` / [lucaswewa/teta-wot#5](https://github.com/lucaswewa/teta-wot/pull/5)
- Size: planned M, re-sized to L by the owner, estimated ~840 · measured **1,202** hand-written changed lines (+1,123 −79). That is **over L's 900**: the library is 832, the tests 227 and the example 143. Docs and `Cargo.lock` aren't counted.

## Summary

A `teta-wot` application can now decide for itself who is calling and what each caller may do, and the code each request runs knows who called ([spec](../phases.md#u05)). OScope's login and control lease (D22), and its rule that a client without the lease can't change state (D32), will be an authenticator and an authoriser.

- **An `Authenticator`** turns a request's parts into a `Principal`, or refuses it with a `Rejection` (`401` with a challenge).
- **An `Authorizer`** allows or denies a principal an `Access`: the Thing, the affordance, the `Operation`, and whether it changes state. Denial is a `403`.
- **The principal reaches** getters, setters, actions, custom endpoints, merged routes and binary-stream producers.
- **WebSockets without credentials** authenticate with their first message.
- **The old Basic and bearer schemes** are an authenticator too, and behave as before.

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **`Principal`:** an optional id and JSON attributes.<ul><li>`Principal::current()` gives the caller: from the invocation's scope inside an action, otherwise from a task-local set for the request, otherwise anonymous.</li><li>`scope` and `sync_scope` run code as a principal.</li></ul> | `crates/teta-wot-core/src/principal.rs` |
| **The runtime carries it.**<ul><li>Each invocation's scope keeps its caller, as `ActionCtx::principal`, for as long as the action runs, after the request has been answered.</li><li>Blocking property work runs in the caller's scope.</li></ul> | `context.rs`, `runtime.rs`, `property.rs` |
| **The traits and types:** `Authenticator` (async), `Authorizer` (sync), `Access`, `Operation`, `Rejection`, and `Auth`, the pair in the new `HttpOptions::auth`. `Security` implements `Authenticator`: Basic's principal is the username, and bearer's is `bearer` | `crates/teta-wot-http/src/auth.rs` |
| **The check:**<ul><li>Every request that isn't public is admitted or refused, described as an `Access`, and answered inside its principal's scope.</li><li>Routes merged into the server are checked by a layer, as `Operation::Route`.</li><li>TDs, discovery and the docs stay public.</li></ul> | `handlers.rs`, `lib.rs`, `routes.rs`, `auth.rs` |
| **First-message authentication.** A WebSocket upgrade without an `Authorization` header, to the observation WebSocket or a binary stream, is accepted. Its first message, `{"authorization": "…"}`, is then checked as the header would have been. A failure, or no message within 10 s, closes the socket with 1008. | `auth.rs`, `observe.rs`, `binary.rs` |
| **Custom WebSocket endpoints** can do the same:<ul><li>`EndpointSpec::websocket` marks one;</li><li>its handler receives a `FirstMessageAuth` and calls `authenticate` on it.</li></ul> | `crates/teta-wot-core/src/endpoint.rs`, `auth.rs` |
| **Binary streams:** `Subscriber::principal()`; the producer runs in the principal's scope | `binary.rs` |
| **Setting it up:**<ul><li>`ThingServerBuilder::authenticator` and `authorizer`;</li><li>exports from `teta_wot::server` and `teta_wot::http::FirstMessageAuth`.</li></ul> | `crates/teta-wot-server/src/lib.rs`, `crates/teta-wot/src/lib.rs` |
| **The TDs** describe the authenticator's `security_scheme()`, if it gives one | `auth.rs`, `handlers.rs` |
| **The example `custom-auth`:** API keys for an operator and a viewer, and a logbook that signs its entries | `examples/custom-auth/` |
| **Docs:**<ul><li>a guide section, *Your own authentication and authorisation*;</li><li>the *Limits* note now says WebSockets can send a bearer token;</li><li>the examples table, whose count was out of date (35, now 39);</li><li>the CHANGELOG.</li></ul> | `docs/guide/src/security.md`, `examples.md`, `CHANGELOG.md` |

## How to try it

```sh
cargo run -p custom-auth
```

It prints:

```text
nobody: reading → 401 Unauthorized, writing → 401 Unauthorized
   ben: reading → 200 OK, writing → 403 Forbidden
   ada: reading → 200 OK, writing → 201 Created
        the logbook: ["ada: ada was here"]
```

## Design notes and deviations from the plan

- **The size is over budget.** It was estimated at ~840 against M, and the owner chose one PR re-sized to L, knowing it might pass 900. It measured 1,202.
  - **Where the lines went:**
    - `auth.rs` (378 lines, of which 106 are comments and blank lines);
    - the dispatch's mapping of every route to an `Access` (about 50 lines);
    - first-message authentication through three paths (observation, binary streams, custom endpoints);
    - a 227-line integration test.
  - **If the owner prefers,** it can be split before merging:
    - U05a: the traits, the principal and the HTTP checks;
    - U05b: first-message WebSocket authentication and the principal in streams.
- **The principal is ambient, not an argument.** `Principal::current()` reads a task-local, so getters, setters and endpoints need no new parameter, and the macros are unchanged.
  - **Actions also get it from `ActionCtx`,** because they run in their own task after the request has been answered; the invocation's scope keeps it.
  - **Code that spawns its own tasks** must pass it on with `Principal::scope`.
- **The authenticator is async** (it may ask another service), and **the authoriser is sync,** because it runs on every request. An authoriser that needs data can have the authenticator put it in the principal's attributes.
- **What "changes state" means:**
  - writing or resetting properties, invoking actions and cancelling them;
  - custom endpoints and merged routes with any method but `GET`, `HEAD` or `OPTIONS`;
  - reading invocations is not a change.

  A route that the mapping doesn't name counts as a change, so new endpoints fail closed.
- **First-message authentication applies only without a header.** An upgrade with an `Authorization` header is checked as usual, and refused with `401` or `403` before upgrading. The first message's value is what the header would have held.
- **Only endpoints that opt in defer.** A custom WebSocket endpoint that doesn't know about this still answers `401` without credentials.
  - **`FirstMessageAuth` became public while the docs were being written.** `EndpointSpec::websocket` said the handler "must do that check itself", but the type it needed was crate-private. Without it, an application's own endpoint marked `websocket` would have let unauthenticated sockets in.
  - **It was renamed** from `Deferred` to `FirstMessageAuth` and given a public `authenticate`.
- **One behaviour change.** On a server that requires credentials, the observation WebSocket used to refuse an upgrade without them with `401`. It now accepts the upgrade and waits for the first message. The CHANGELOG says so.
- **One API break.** `HttpOptions` gained a public field, `auth`, so code that builds it with a struct literal must add it. The CHANGELOG says how.
- **`check` returns a `Rejection`, not a `Response`.** Clippy's `result_large_err` flagged the large error type. The response is built at the edge.

## Tests

- **Integration** (`crates/teta-wot/tests/auth.rs`):
  - **The setup.** A `Notebook` Thing with:
    - a property and its setter;
    - an action;
    - an endpoint;
    - a binary stream;
    - a merged route.

    The server has an API-key authenticator (an operator and a viewer) and an authoriser that lets viewers only look.
  - **Rejection codes:**
    - no key → `401` with the authenticator's challenge, and the TD stays public;
    - the viewer reads (`200`), but writing and invoking are `403`;
    - a merged route that changes nothing is allowed;
    - the operator writes.
  - **The principal reaches every kind of affordance:**
    - the setter records `alice`;
    - the endpoint and the merged route answer `alice`;
    - the action's output is `alice/alice`, from `ActionCtx::principal` and from `Principal::current()`.
  - **WebSockets:**
    - a binary stream opened without a header authenticates as `bob` with its first message, and its producer sends `bob`;
    - a bad key in the first message closes both the binary stream and the observation WebSocket with 1008.
  - **The old schemes:** a server with `Security::bearer` describes `bearer_sc` in its TD.
- **The old schemes unchanged:** the existing Basic and bearer tests in `teta-wot-http` and `teta-wot-server` pass unmodified. The one unit test that used the removed `unauthorized` helper now builds its `401` through `Rejection`.
- **Mutation checks:**
  - **Ignoring the authoriser's answer** fails the rejection test.
  - **Running invocations as anonymous** fails the affordance test.
  - **Never deferring WebSockets** fails the WebSocket test.
  - All files were restored.
- **CI replayed locally:**

  | Job | Result |
  |---|---|
  | Ubuntu, a fresh LF clone of `f102701` in WSL | All 46 steps passed (fmt, clippy, tests, docs, docs pages, 39 examples, the guide); 80 test suites ok |
  | Windows | All 46 steps passed, the guide included; 80 test suites ok |

## Follow-ups and known gaps

- **Custom schemes aren't described** unless the authenticator's `security_scheme()` gives one. The example's `Key` scheme gives none, so its TDs say `nosec` although keys are required. An authenticator can describe it as W3C TD's `apikey` scheme, in the `Authorization` header. OScope's (P49) should.
- **Cookies work everywhere.** An authenticator receives the whole request, cookies included. A session cookie therefore covers `EventSource`, MJPEG `<img>` tags and WebSockets, which can't send headers. OScope's login (D22) can use one for browsers, with first-message authentication for clients that hold a token.
- **The observation WebSocket is authorised as a whole** (`Operation::Observe` on the Thing), not per property or event it subscribes to.
- **No limit on failed attempts.** Nothing throttles or locks out repeated failures; OScope's login (P49) needs that.
- **Merged routes' WebSockets** need credentials on the upgrade request (a header or a cookie): the merge layer doesn't defer.

Next phase: U06 (release v0.2.0).

## Clean room

All of it was written for this phase, against the axum, tokio and W3C TD 1.1 documentation. Nothing was copied.
