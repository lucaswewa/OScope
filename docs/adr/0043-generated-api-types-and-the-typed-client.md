# ADR-0043: Generated API types and the typed client

- Status: Accepted
- Date: 2026-11-18
- Phase: W1.1

## Context

- **The server describes its API itself.** `oscope-server --print-openapi` prints an OpenAPI 3.1 document, made from its Things' affordances and its routes' schemas: 259 paths, 351 operations and 312 schemas.
- **The app calls the server from `src/api/` only** ([ADR-0039](0039-the-web-app-its-layers-state-and-routing.md)). Until W1.1, each call was an address and a cast, written by hand. A change on the server would break the app silently, at run time.
- **The media plane isn't in the document** ([ADR-0006](0006-control-plane-and-media-plane.md)): deep-zoom tiles, volumes, a project's files, and the logs' live tail. Neither is the answer of `jobs/job/{id}`, which the document gives no schema.
- **The server fails in several shapes:**
  - problem details (RFC 9457), with a device's refusal of a property's value as their title ([ADR-0034](0034-driver-errors-over-teta-wot.md));
  - its checks' 422s, a list of what was refused and where;
  - `{"detail": "…"}`, for what isn't there;
  - an action's invocation that ends in `error`, its refusal's kind and status leading its detail, "busy (409): …".

  A call can also get no answer at all.
- **openapi-typescript** is the generator the plan names (MIT). Its newest release, 7.13.0, declares TypeScript `^5` as a peer, and the app uses TypeScript 6.

## Decision

**1. The types are generated, and committed.** `npm run api:types` turns `oscope-server --print-openapi` into `web/src/api/generated/openapi.ts` with openapi-typescript, a development tool.
- **Why committed:** the app builds and type-checks without a server, and a pull request shows the API's changes as a diff.
- **Generated, not written:** the file is left out of lint and formatting, and out of the size budget's count.

**2. A stale file fails the checks.** `npm run api:check` makes the types again in memory and compares them with the file, line endings aside. It names the first line that differs. The replays and CI run it once the server is built.

**3. openapi-typescript runs on the app's TypeScript.** An npm `overrides` entry gives it the app's TypeScript as its peer. A TypeScript it couldn't use would show twice: in the stale check, and in the type check of the 21,891 lines it writes.

**4. The calls are made by openapi-fetch** (MIT, 6 KB, in the app), typed by the generated `paths`: each path, its parameters, its body and its answer. Around it, `src/api/client.ts` adds:
- **`call(start, limits)`:** a time limit, 10 s unless told, and the caller's abort, made one signal. An answer that isn't a success, or none, is thrown as an `ApiError`;
- **`fetched(address, read, limits)`:** the same for the routes the document lacks;
- **`Server(base)`:** the server that serves the app by default, its addresses relative to the page, or a server elsewhere by its URL.

**5. One error model.** An `ApiError` has:
- its `failure`: refused, invalid, missing, unauthorised, forbidden, cancelled, failed, server, unreachable, timeout or aborted;
- the server's message;
- a device's `refusal` and its `status`, from ADR-0012's table;
- the `fields` a 422 refused, each with its place in the body;
- the `body` as the server gave it, for a form's `refused` (W0.6).

It is read from each of the server's shapes above, and from a call that had no answer.

**6. Actions are typed by their paths.** An action's input and output types are read from its generated operation, so a wrong input, or a path that isn't an action, fails the type check.
- **`start`** sends the input, and is answered 201 with the invocation.
- **`follow`** looks at `action_invocations/{id}` until the invocation ends: 50 ms after the start, then each wait half as long again, up to 1 s.
- **`cancel`** sends a `DELETE`.
- **`act`** does all three, and gives the output or throws the error. A caller that aborts cancels the invocation on the server too.

**7. Jobs are followed by hand-written types.** An action that hands a job over answers with a `JobRef` ([ADR-0016](0016-the-job-service.md)). `finished(ref)` follows `jobs/job/{id}` to its end. Its `Job` type is written here, since the document has none.

**8. The media plane has wrappers written by hand,** in `src/api/media.ts`:
- deep zoom: the descriptor, the status, and the tiles;
- volumes: the metadata, slices and bricks, their binary header read;
- a project's files, whole or in a range;
- the logs' live tail.

**9. What isn't generated is kept honest by contract tests.** `tests/e2e/contract.spec.ts` calls a real server, each test its own: each media wrapper, the `Job` type, an action followed to completion, to a failure and cancelled, and the error model. The unit tests read the server's own answers, recorded by `scripts/record-answers.mjs`.

## Consequences

- **Gains:**
  - A change to the server's API fails the type check wherever the app used the old shape, before it ships.
  - Every failure reads the same way, whichever shape the server gave it.
  - Each call has a time limit, and stops when its caller does.
- **Costs:**
  - A generated file of 752 KB in the repository, which changes with the API.
  - The generator's TypeScript peer is overridden.
  - Actions are followed by looking, not told: up to 1 s late at their end.
  - The media plane and the job's answer are typed by hand, and kept honest only by the contract tests.
  - openapi-fetch is in the bundle.
- **To watch:**
  - **TypeScript 7,** a compiler without the JavaScript API openapi-typescript builds on. If the generator can't follow it, a generator of our own can be written from the schemas W0.6 already reads.
  - **A job's answer, or a media route, described in the document:** then its hand-written type goes.

## Alternatives considered

- **Types written by hand:** the drift this ADR is for.
- **Types made at each build, not committed:** the app's build would then need a server, and a pull request wouldn't show what changed in the API.
- **A typed fetch of our own instead of openapi-fetch:** some 150 lines of types for paths, parameters and bodies, which openapi-fetch already has, and tests.
- **A generator of our own:** possible, from W0.6's reading of schemas, but a TypeScript writer for 259 paths is the generator's whole job. It is kept as the way out if openapi-typescript stops working with TypeScript.
- **Following actions by events:** W1.2 brings the WebSocket's live properties and events. Looking at the invocation is what every action supports now, and what the spec asks.
