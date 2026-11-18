# ADR-0039: The web app: its layers, state and routing

- Status: Accepted
- Date: 2026-11-17
- Phase: W0.1

## Context

- **Milestone 2 is the app** (plan §8): one Vue 3 codebase, built for the browser and, from W9, for a Tauri 2 desktop app. goal.md names its stack: Vue 3, TypeScript, Vite, reka-ui, Pinia and Material Symbols, with `microscope`'s look.
- **Two things break such an app when they leak:**
  - a component library used everywhere, so that changing it means changing every tab;
  - calls to the platform (the network, storage, windows) made anywhere, so that the desktop build has no one place to replace them.
- **The server serves the app.** [ADR-0006](0006-control-plane-and-media-plane.md) gives the API's origin its pages, and under security a page from another origin can't send what the API needs.
- **`microscope` kept the same two rules** by lint, and they held through its milestone. Its ideas may be reused (ADR-0002).

## Decision

**1. The app is `web/`,** a package of its own beside the Rust workspace: Vue 3, TypeScript in strict mode, Vite, Pinia, Vue Router and vue-i18n, on Node 24 with npm. `desktop/` will build the same sources (W9).

**2. `src/` has five layers,** and ESLint keeps what each may import:

| Layer | Holds | May import |
|---|---|---|
| `ui/` | The components, on reka-ui | Nothing of the app's |
| `api/` | The server: typed calls, live state, streams | Nothing of the app's |
| `host/` | The platform: storage, dialogs, windows | Nothing of the app's |
| `features/<tab>/` | A tab, or a part of one | `ui/`, `api/`, `host/`, and other features |
| `app/` | The shell: routes, navigation, the words | All of them |

- **Only `ui/` imports reka-ui.** A tab uses the app's components, so the library behind them can change in one place.
- **Only `api/` reaches the network,** and only `host/` the platform: outside them, lint refuses `fetch`, `WebSocket`, `EventSource`, the browser's storage, `window.open`, and absolute URLs.
- **A feature takes what it needs as arguments** where that keeps it testable: the first page's version line is given the function that asks for the version.

**3. Routes go by hash,** `#/control`. The same build then runs from a server's root, from a folder, and inside the desktop app, with nothing for any of them to rewrite. Each page is a chunk loaded when first opened.

**4. The build is relative** (`base: "./"`), for the same reason.

**5. State is Pinia's,** a store a Thing or a concern, on the live properties W1.2 brings. A component's own state stays in the component.

**6. Every string comes from vue-i18n** (D29), in English for now.

**7. The server serves the built app:** `oscope-server --web FOLDER`, or `[web] root`, gives the folder to ADR-0006's pages. The app and the API then share an origin. In development, Vite serves the app on port 8280, and passes `/api` and `/health` on to a server.

## Consequences

- **Gains:**
  - The desktop build replaces `host/`, and nothing else knows.
  - A component's look and behaviour live in `ui/` alone.
  - A rule that is broken fails lint, with the reason and this ADR's number.
- **Costs:**
  - A feature can't call `fetch` for a quick try: it adds a function to `api/`.
  - Hash routes aren't pretty, and the server can't see them.
- **To watch:**
  - Features importing each other: allowed for now, and to be looked at as the tabs grow.
  - The lint rules name globals: a new way to the network, such as `navigator.sendBeacon`, needs adding.

## Alternatives considered

- **History routing:** clean paths, but the server and the desktop app would each need to send every path to `index.html`.
- **One flat `src/`,** with conventions and no lint: `microscope`'s retrospective and its lint rules say the rules hold only when checked.
- **A monorepo tool** (pnpm workspaces, Nx) for `web/` and `desktop/`: two packages don't need one.
