# ADR-0045: The app's connection, its sessions, and media under security

- Status: Accepted
- Date: 2026-11-18
- Phase: W1.3

## Context

- **The app meets its server three ways:**
  - in a browser, served by the server itself: the same origin;
  - in development, through the dev server, which passes `/api` on;
  - in the desktop app, from a webview at an origin of its own.
- **With accounts, the server has security** ([ADR-0031](0031-authentication-and-the-control-lease.md)):
  - a session's token in each request's `Authorization` header, or in a WebSocket's first message;
  - a control lease that every change of state needs.

  ADR-0031 left open how an image is shown under security: an `<img>` can't send a header.
- **teta-wot v0.2.0's CORS lets every origin in,** with credentials, and has no setting to narrow it.
- **The server describes its API without security** when it prints it, so the app's types had no `auth` Thing to call ([ADR-0043](0043-generated-api-types-and-the-typed-client.md)).

## Decision

**1. The connection** (`web/src/api/connection/connection.ts`):
- **It watches `/health`,** which is open to anyone, every 2 s.
- **Its states:** connecting, connected, reconnecting, refused, and a server whose version the app doesn't know.
- **A version is known** when its major and minor numbers are the app's. It is read once the server lets the app read it: at once without accounts, after the login with them.

**2. The session** (`connection/session.ts`):
- **Logging in gives a token.** It goes with every call and every fetch of a medium, in the `Authorization` header, and as each WebSocket's first message. Never a cookie, never in an address.
- **The host keeps it for the browser's session,** the tab, so that a reload stays logged in. `api/` can't touch storage ([ADR-0039](0039-the-web-app-its-layers-state-and-routing.md)), so the login feature hands it to the host.
- **A 401 while logged in ends the session,** whether the server ended it, or it went idle, or it was logged out elsewhere. The app goes to the login, then back to where the user was.
- **Without accounts,** there is no session to have: everyone is let in.

**3. Media under security,** settling ADR-0031's open point:
- **Images, tiles and downloads are fetched with the token,** and shown as blobs. `fetched` adds the header, as `call` does.
- **No cookie, and no token in a URL,** which logs, histories and other pages could keep.

**4. The control lease** (`connection/lease.ts`):
- **`useLease`:** who holds it, who asked, and taking it, asking for it, handing it over and giving it up. It is read through W1.2's store, again at each `lease_changed` ([ADR-0044](0044-things-in-the-app-live-properties-and-events.md)).
- **The warning:** the holder is warned a minute before the lease lapses. That is counted from the last change this app made, or from when the app got the lease.
- **`useCanControl()`** is what every control asks: always yes without accounts, and with them, yes while this app holds the lease.

**5. Uploads and downloads with their progress** (`api/transfer.ts`):
- a download is read from its stream as it comes;
- an upload goes by `XMLHttpRequest`, the only way a browser tells an upload's progress.

**6. The server's origins:** `[network] origins` lists the origins, besides its own, whose pages may call the server. A request whose `Origin` is neither is refused, 403, before it is served, a WebSocket's upgrade among them.
- **A page's own origin** is an `Origin` whose host and port are the request's `Host`. The dev server's proxy keeps the browser's `Host`, so its requests count as the server's own.
- **The desktop app lists its webview's origin.**
- **Requests without an `Origin`** are let in: other clients, and a browser's requests for its own pages.
- **teta-wot's CORS still answers preflights,** before this layer, so the request that follows a preflight is the one refused.

**7. The printed API has security on,** with no account, so that `--print-openapi` describes the `auth` Thing, and the app's types cover the login and the lease.

## Consequences

- **Gains:**
  - The app knows when it has its server, and when that server is another release.
  - A token never leaves the headers and the sockets' first messages.
  - Every control asks one question, which is right with or without accounts.
  - A page elsewhere can't drive the instrument through a user's browser.
- **Costs:**
  - A picture under security is a fetch and a blob, not a plain `<img src>`: its URL must be revoked when it is no longer shown.
  - The lease's warning is counted from this app's own changes. Another tab of the same account keeps the lease without this one knowing.
  - Pages at listed origins are trusted as the server's own.
- **To watch:**
  - **teta-wot gaining a setting for its CORS:** then the layer can go.
  - **Many tiles under security:** a blob for each costs memory; the deep-zoom viewer (W6.4) will need to drop them as they leave the view.

## Alternatives considered

- **A cookie for the session:** a browser sends it with an `<img>`, but it brings cross-site request forgery, and webviews treat cookies across origins each their own way.
- **The token in media's addresses:** simple for an `<img>`, but addresses end up in logs, histories and `Referer` headers.
- **A service worker adding the header to media requests:** it works in a browser, but not in every webview.
- **Every origin let in, as teta-wot does:** without accounts, any page the user opens could then drive the instrument.
