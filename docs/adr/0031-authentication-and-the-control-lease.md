# ADR-0031: Authentication and the control lease

- Status: Accepted
- Date: 2026-11-16
- Phase: P49

## Context

- **Plan §5.18 and D22 want safe remote use:**
  - named accounts, with argon2-hashed passwords and equal rights in M1;
  - sessions as bearer tokens, a WebSocket's in its first message;
  - the local desktop app proving itself with its per-boot token (ADR-0015);
  - 127.0.0.1 by default, and on the network only with an account and TLS;
  - one controller at a time.
- **teta-wot v0.2.0 already has the hooks** (U03, U05):
  - an `Authenticator`, which names a request's principal;
  - an `Authorizer`, which is told whether a request changes state;
  - the first-message check for WebSockets;
  - a TLS listener on rustls.
- **The examples and tests run without security,** and should stay as they are.
- **Who holds control** decides what may change: hardware actions, property writes, every job, recipes, annotations, settings, extensions and power. Watching and reading stay open to anyone logged in.

## Decision

**1. Security is on when the server has accounts.** `ApiBuilder::security` turns it on, with an authenticator, an authoriser and the `auth` Thing. Without it, as in the examples, everyone is anonymous and may do everything.

**2. Accounts** are kept in `{data_root}/accounts.json`, readable only by the server's account (ADR-0015's private files):
- **Names:** 1 to 64 of `a`–`z`, `0`–`9`, `.`, `_` and `-`, starting with a letter. `local` is the desktop app's.
- **Passwords:** 8 to 1024 characters, kept as argon2id PHC strings, with argon2's default parameters (19 MiB, 2 passes).
- **An unknown name** is checked against a decoy hash, so it takes as long as a known one.
- **The command line:** `oscope-server account add|remove|passwd|list`. A password is asked for twice on a terminal, unseen, or read from stdin's first line otherwise.

**3. Sessions.**
- **Logging in:** `POST /api/v1/auth/login` with `{"name", "password"}` gives `{"token", "name"}`: 32 random bytes in hex. The server keeps only the token's SHA-256.
- **Using it:** `Authorization: Bearer …` on each request, or in a WebSocket's first message.
- **Ending it:** `POST /api/v1/auth/logout` ends it, and so does going unused longer than `[security] session_idle_minutes` (480).

**4. The authenticator.**
- **Open:** logging in, and every path outside `/api/`: pages and `/health`. A browser can't send a token for a page.
- **Under `/api/`,** everything else needs a session's token or the local one, or gets a 401 with `WWW-Authenticate: Bearer realm="oscope"`.
- **The local token** is the desktop app's principal, `local`.

**5. The control lease.**
- **Held:** one principal holds it, or no one.
- **The authoriser** lets anyone logged in read and watch. A request that changes state needs the lease, with the `auth` Thing's own requests apart. teta-wot decides what changes state: writes, actions, cancels, and endpoints and routes with other methods than GET and HEAD.
- **The `auth` Thing's actions:**
  - `acquire_control`, when no one has it;
  - `request_control`, which its holder sees, or which gives it if it is free;
  - `hand_over_control` to an account or `local`;
  - `release_control`;
  - `take_control`, for `local` alone: the desktop app can always take it.
- **Idle:** its holder's changes keep it; idle longer than `[security] lease_idle_minutes` (10), they lose it.
- **What the `auth` Thing shows:** `lease`, `me` and `accounts`, and the event `lease_changed`.

**6. The network and TLS.**
- **An account first:** listening on an address that isn't loopback needs an account, or the server refuses to start, saying so.
- **TLS:**
  - It serves through teta-wot's `TlsListener`, `Api::serve_tls`, with HTTP/2 for clients that negotiate it.
  - Its certificate is `[security] certificate` and `key`, PEM. Without them, on the network, a self-signed one is made once with rcgen and kept in `{data_root}/tls`, its key private. It names `localhost`, the loopback addresses, the address, and the computer's name if the environment gives it.
  - On loopback, TLS is used when a certificate is given.

**7. The dependencies:**
- `argon2` 0.6.0;
- `rcgen` 0.14.10;
- `rpassword` 7.5.4;
- teta-wot's `tls` feature, which brings rustls 0.23 and `ring`.

Their licences are on ADR-0002's list (`ring`: Apache-2.0 and ISC).

## Consequences

- **Gains:**
  - **A test made from the Thing Descriptions** checks that every affordance that changes state refuses a caller without the lease, so a new Thing is covered without a list to keep.
  - **Nothing changes without accounts:** the examples, the tests and a development server on loopback.
  - **The desktop app** is never locked out.
- **Costs:**
  - **The browser's media.** `<img>` tags and `EventSource` can't send a bearer token, so a web client under security fetches media and events with `fetch`, and shows images as blobs. Cookies or tokens in URLs are M2's choice to make, for the web app.
  - **Sessions are in memory:** a restart logs everyone out, and frees the lease.
  - **A self-signed certificate** makes a browser warn until it is trusted, or replaced by one the site gives.
- **To watch:**
  - **Actions that change nothing,** such as `simulation/in_focus`, need the lease too, as teta-wot counts every action as a change. A viewer who needs one is a reason to mark it in teta-wot.
  - **Rights:** every account has equal rights in M1. Roles would go in the authoriser, from the principal's attributes.

## Alternatives considered

- **Cookies for sessions:** browsers send them by themselves, images included, but they need CSRF protection on every state-changing request, and the desktop app and scripts don't want them. Bearer tokens are what teta-wot's WebSocket check takes.
- **Security always on,** a default account made at first start: it would change every example and test, and a default password is worse than none on loopback.
- **A lease per Thing or device:** finer, but D32 asks for one controller at a time, and the M1 app has one operator.
- **bcrypt or scrypt:** argon2id is OWASP's first choice for password storage, and the plan names it.
