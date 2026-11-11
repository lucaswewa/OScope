# U02: Path parameters for endpoints

- Status: In review
- Branch / PR: `u/u02-path-params` in `teta-wot` / [lucaswewa/teta-wot#2](https://github.com/lucaswewa/teta-wot/pull/2)
- Size: estimated ~540, re-sized to L before starting (owner's decision) · measured **745** hand-written changed lines (+681 −64). Docs and `Cargo.lock` aren't counted.

## Summary

Custom endpoints in `teta-wot` can now take values from the path ([spec](../phases.md#u02)):

- **Syntax.** A segment can be a parameter, `{name}`, and the last segment a wildcard, `{*rest}`.
- **Precedence.** When several routes match, the most specific answers: a literal beats a parameter, which beats a wildcard.
- **Ambiguity.** Endpoints that would match exactly the same requests are rejected when the server is built.
- **Handlers** read the values through a new `PathParams` extractor.
- **OpenAPI** lists them as `in: path` parameters.

## What was built

All of it is in `teta-wot`.

| Change | Where |
|---|---|
| **Validating path syntax.** `path_problem` accepts `{name}` segments and a final `{*name}`. It rejects:<ul><li>a parameter inside a segment (`{dataset}_files`);</li><li>an unclosed or empty brace;</li><li>a name that isn't letters, digits and `_` (not starting with a digit);</li><li>a wildcard that isn't last;</li><li>a name used twice.</li></ul>Each error is reported when the runtime is built | `crates/teta-wot-core/src/endpoint.rs` |
| **The route table.** `Segment` is now `Literal`, `Param(name)` or `Wildcard(name)`. The framework's own `{id}` routes use the same parser.<ul><li>**Matching** returns every parameter, by name.</li><li>**`find` picks the most specific matching route,** comparing segments left to right (literal < parameter < wildcard), where it used to pick the first match. Between equals the first added still wins, so the framework's routes behave as before.</li><li>**The endpoint conflict check** compares route *shapes*, ignoring parameter names, where it used to compare literal paths.</li></ul> | `crates/teta-wot-http/src/routes.rs` |
| **`PathParams`.** An axum extractor with `get`, `parse::<T>` and `iter`. Values are percent-decoded, and a value that isn't UTF-8 once decoded is a `400`. It is exported as `teta_wot::http::PathParams`. axum's own `Path` can't work, because `teta-wot` routes requests itself | `crates/teta-wot-http/src/endpoint.rs`, `handlers.rs`, `lib.rs`; `crates/teta-wot/src/lib.rs` |
| **Request plumbing.** A custom endpoint's parameters are put in the request's extensions before its handler runs. The framework's routes still get their single ID, as before | `crates/teta-wot-http/src/handlers.rs` |
| **OpenAPI.** Each parameter is an `in: path`, required string parameter. A wildcard is written `{rest}`, with a description saying it spans segments, because OpenAPI has no wildcard | `crates/teta-wot-http/src/openapi.rs` |
| **The example `path-parameters`:**<ul><li>a slide scanner's tiles at `tiles/{level}/{tile}`;</li><li>its files at `files/{*path}`, at any depth;</li><li>the literal `files/index.txt` beating the wildcard;</li><li>the parameters OpenAPI lists.</li></ul> | `examples/path-parameters/` |
| **Docs:** the guide's structure page, the `custom-endpoint` README, and the CHANGELOG | |
| **New dependency:** `percent-encoding` 2.3 (MIT/Apache-2.0). It was already in the build through axum, so `Cargo.lock` only gains the edge | `Cargo.toml`, `crates/teta-wot-http/Cargo.toml` |

## How to try it

```sh
cargo run -p path-parameters
```

It prints six requests and their answers. Examples:

- `GET /slide/tiles/2/3_4.txt → 200 OK: the tile at level 2, column 3, row 4`
- `GET /slide/files/notes%2F2026-11-11.txt → 200 OK: Rescanned row 4.`: the wildcard, with an encoded `/`.
- `GET /slide/files/index.txt` lists the files: the literal beats the wildcard.

It ends with the path parameters OpenAPI lists for both templates.

## Design notes and deviations from the plan

- **Parameters are whole segments only.** The spec's goal mentions `dzi/{dataset}_files/{level}/{tile}`, which has a parameter inside a segment. That syntax is rejected, with a message saying parameters must be whole segments:
  - partial-segment matching would complicate precedence and ambiguity for little gain;
  - an endpoint can take the whole segment (`{tile}` = `3_4.jpeg`) and parse it, as the example does.
  - OScope's deep-zoom routes are its own axum routes, merged through U03, so they don't depend on this choice. If P40 needs it, the DZI layout can use `dzi/{dataset}/image.dzi`, whose tiles are then at `dzi/{dataset}/image_files/{level}/{col}_{row}.jpg` (whole segments only).
- **Precedence is by specificity, not order.** The table used to take the first match in the order routes were added. Precedence now comes from the segments, so the order endpoints are declared in no longer matters. The framework's own routes never overlap, so their behaviour is unchanged; the existing route tests still pass as they were.
- **"Wrong method" uses the same precedence.** When no route has the request's method, the 405 names the most specific route that matched the path.
- **A wildcard needs a non-empty rest.** `files/{*rest}` doesn't match `files/`. Empty parameters never matched before, and still don't.
- **Two errors, at two times:**
  - **The runtime's build** catches malformed syntax. It can, because the core sees each endpoint's path.
  - **The router's build** catches ambiguity (`RouteError::EndpointConflict`), because only the router sees every route, the framework's included.
- **The OpenAPI "snapshot" is an expected-JSON assertion.** `insta` is a workspace dependency, but no crate uses it, and adding it would pull new crates into the build.
- **Size.** 745 lines against an estimate of ~540 (+38%):
  - the router and its tests: 201, against ~155;
  - the example: 142, against ~105;
  - the core validation and its tests: 96, against ~50.

## Tests

- **`teta-wot-core`:**
  - accepted paths, including parameters and a final wildcard;
  - nine malformed paths, each with its explanation checked.
- **`routes` unit tests:**
  - the existing tests, updated for named parameters;
  - precedence, with routes added least specific first, so insertion order can't decide;
  - recognising the same shape regardless of parameter names.
- **`tests/path_params.rs`**, over real HTTP requests:
  - which endpoint answers, and its parameters;
  - percent-decoding, including `%2F`;
  - a `400` for invalid UTF-8;
  - `404` for empty parameters;
  - same-shape endpoints rejected when the router is built;
  - malformed syntax rejected when the runtime is built;
  - the OpenAPI parameters.
- **Mutation checks:**
  - reversing the precedence comparison makes `literals_beat_parameters_and_parameters_beat_wildcards` fail;
  - disabling the shape check makes `endpoints_of_the_same_shape_are_rejected_when_the_server_is_built` fail;
  - both files were restored.
- **CI replayed locally,** since GitHub Actions minutes are still out:

  | Job | Result |
  |---|---|
  | Ubuntu, fresh LF clone of `825b9ea` in WSL | All 43 steps passed (fmt, clippy, tests, docs, docs pages, 36 examples, the guide); 73 test suites ok |
  | Windows | All 43 steps passed, the guide included; 73 test suites ok |

## Follow-ups and known gaps

- **No typed extraction.** `PathParams` doesn't deserialise into a struct, as axum's `Path<T>` does. `get` and `parse` cover current needs; serde support can come later if endpoints want it.
- **Thing Description links** to an endpoint with parameters carry the template as their `href`, because a TD link can't be a template. Endpoints with parameters are best left unlinked.

Next phase: U03 (router composition, custom listeners, HTTP/2).

## Clean room

All of it was written for this phase. The precedence rule (literal, then parameter, then wildcard) is the common one in HTTP routers, described here in our own words. Nothing was copied.
