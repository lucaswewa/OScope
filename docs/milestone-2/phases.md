# Milestone 2 phase specifications

Companion to the [implementation plan](../implementation-plan.md), whose [§8](../implementation-plan.md#8-milestone-2-phase-overview) lists these phases with their status. Milestone 2 is the app: one Vue 3 codebase in `web/`, built for the browser. It runs on the simulated instrument throughout. Stage W9, a desktop app, was dropped after W7 (ADR-0063).

Each phase lists:

- its goal, dependencies, first estimate and ADRs;
- its scope;
- how to **try it**, which every phase has;
- when it's done;
- how it's tested.

## How Milestone 2's phases work

What [§6 of the plan](../implementation-plan.md#6-how-phases-work) says holds, with these differences.

- **IDs** name the stage and the phase in it: `W3.2` is the second phase of stage W3. A split adds a letter: `W3.2b`.
  - The branch is `m2/w3.2-short-name`, and the pull request's title `W3.2: <title>`.
  - The notes are `docs/milestone-2/notes/W3.2-short-name.md`.
- **Estimates, not budgets.** Milestone 1's S, M and L didn't fit its phases ([its look back](../milestone-1/retrospective.md)).
  - **A first estimate** stands in each spec: hand-written lines, to the nearest hundred.
  - **A second estimate** is written when the phase starts, in five parts: code, tests, styles, the try-it's page and end-to-end test, and glue. It goes in the plan's change log, in a commit of its own, before the code.
  - **The owner is asked before the code** when the second estimate is over 2,500 lines, or a third over the first.
- **Try it** stands where Milestone 1 had examples. The app is the example:

  ```sh
  cargo run --release -p oscope-server -- --config configs/oscope.toml   # the simulated instrument, on 8210
  npm --prefix web run dev                                               # the app, on http://127.0.0.1:8280/
  ```

  Each spec names the route to open and what to do and see there. The notes show it too, in screenshots of both themes that the phase's end-to-end test takes.
- **Tests** are of four kinds:
  - **unit:** Vitest, for logic and components;
  - **contract:** the client against a real server;
  - **end to end:** Playwright, the built app served by a server of the test's own, on a port the system picks, with its own data root;
  - **visual:** screenshots against baselines, in both themes.

  Milestone 1's rules hold: tests wait for conditions; anything that detects or supervises is broken on purpose once; touched suites run five times on two cores.
- **Server work** that the app needs is in the phase that needs it, marked **Server**, with its Rust tests. The generated types follow it in the same pull request.
- **Every string** goes through vue-i18n (D29). **Every control** works from the keyboard, and passes axe.
- **Clean room:** nothing is copied from `microscope`. Its look is described in our own words in `look.md` (W0.2), and built from that.
- **Checkpoints** G to L end the stages W3 to W8. Each has a demo script in its phase's notes, and a checklist for the owner's own run.

## Contents

- [Stage W0: Foundations](#stage-w0-foundations): [W0.1](#w0-1) [W0.2](#w0-2) [W0.3](#w0-3) [W0.4](#w0-4) [W0.5](#w0-5) [W0.6](#w0-6) [W0.7](#w0-7)
- [Stage W1: Client plumbing](#stage-w1-client-plumbing): [W1.1](#w1-1) [W1.2](#w1-2) [W1.3](#w1-3) [W1.4](#w1-4) [W1.5](#w1-5)
- [Stage W2: Shell](#stage-w2-shell): [W2.1](#w2-1) [W2.2](#w2-2)
- [Stage W3: Control and Live View](#stage-w3-control-and-live-view): [W3.1](#w3-1) [W3.2](#w3-2) [W3.3](#w3-3) [W3.4](#w3-4) [W3.5](#w3-5) [W3.6](#w3-6) [W3.7](#w3-7) [W3.8](#w3-8)
- [Stage W4: Operations](#stage-w4-operations): [W4.1](#w4-1) [W4.2](#w4-2) [W4.3](#w4-3)
- [Stage W5: Scan Workflow](#stage-w5-scan-workflow): [W5.1](#w5-1) [W5.2](#w5-2) [W5.3](#w5-3) [W5.4](#w5-4)
- [Stage W6: Gallery](#stage-w6-gallery): [W6.1](#w6-1) [W6.2](#w6-2) [W6.3](#w6-3) [W6.4](#w6-4)
- [Stage W7: Workspace](#stage-w7-workspace): [W7.1](#w7-1) [W7.2](#w7-2) [W7.3](#w7-3) [W7.4](#w7-4) [W7.5](#w7-5) [W7.6](#w7-6) [W7.7](#w7-7) [W7.8](#w7-8) [W7.9](#w7-9)
- [Stage W8: Settings](#stage-w8-settings): [W8.1](#w8-1) [W8.2](#w8-2) [W8.3](#w8-3) [W8.4](#w8-4) [W8.5](#w8-5) [W8.6](#w8-6)
- [Stage W9: Desktop (dropped)](#stage-w9-desktop-dropped)
- [Stage W10: Quality](#stage-w10-quality): [W10.1](#w10-1) [W10.2](#w10-2) [W10.3](#w10-3) [W10.4](#w10-4)

---

## Stage W0: Foundations

The workspace, the look, and the components every tab is made of. Until W2 there is no shell: these phases are tried in a gallery, `#/dev/gallery`, which only development builds have.

<a id="w0-1"></a>
### W0.1: The web workspace and its checks

**Goal.** `web/` exists: a Vue 3 app that builds, is checked as the Rust workspace is, and shows the server's version.
**Depends on** — · **Estimate** 900 · **ADRs** 0039, 0040

**Scope** (`web/`, `scripts/`, `oscope-server`)
- **The stack:** Vue 3, TypeScript in strict mode, Vite, Pinia, Vue Router with hash routing, vue-i18n. Node 24 and npm, with `package-lock.json` committed.
- **The layers,** which ESLint keeps (ADR-0039):
  - `src/ui/`, the components, and the only place that imports reka-ui;
  - `src/api/`, the server; `src/host/`, the platform;
  - `src/features/<tab>/`, a folder a tab; `src/app/`, the shell.
- **Tauri-ready rules:** outside `api/` and `host/`, no `fetch`, `WebSocket`, `localStorage`, `window.open` or absolute URLs.
- **The checks:** `typecheck` (vue-tsc), `lint` (ESLint, and Prettier's check), `test:unit` (Vitest), `build`, and `test:e2e` (Playwright: Chromium for every test, Firefox and WebKit for those marked as the main path) (ADR-0040).
- **Packages' licences and ages:** a script checks every installed package's licence against ADR-0002's list, as `cargo-deny` does for crates. A package is added with `npm install --before=<two weeks ago>`.
- **Server:** `oscope-server --web FOLDER`, and `[web] root` in the configuration, serve a folder as ADR-0006's pages, so the built app and the API share an origin.
- **The dev loop:** `npm run dev` serves on port 8280, and proxies `/api`, with its WebSockets, to a server.
- **The replays and CI** run the web checks: a `web` job in `ci.yml`, and steps in `scripts/ci-local.sh` and `ci-clone.sh`. WSL gains Node 24 and Playwright's browsers, with the owner's word. CLAUDE.md lists the checks.
- **A first page:** the app's name, and the server's version from `system/version`, or that the server can't be reached.

**Try it** — Start the server and `npm run dev`, and open `http://127.0.0.1:8280/`: it shows OScope and the server's version. Stop the server: within a few seconds it says the server can't be reached. Start it again: the version returns.

**Done when**
- Both replays run the web checks, on Windows and on Linux.
- The end-to-end test runs the built app from its own `oscope-server --web`.

**Tests**
- Unit: the version line's three states.
- End to end: the page against a server, and with the server stopped.
- The licence script fails on a package whose licence isn't allowed, and each lint rule on a fixture that breaks it.
- Rust: `--web` serves the folder's files, refuses paths the API uses, and never serves outside the folder.

<a id="w0-2"></a>
### W0.2: Design tokens, themes and icons

**Goal.** The look, as tokens: `microscope`'s palette, accent and density, recreated in our own words (D1), in light, dark and system themes.
**Depends on** W0.1 · **Estimate** 1,100 · **ADRs** 0041 · **Spec-first**

**Scope** (`web/src/ui/tokens/`, `docs/milestone-2/look.md`)
- **`look.md`:** the look, written from the running `microscope` app and not from its source: the colour roles, the magenta accent, compact density, type, and the rail → control pane → view layout.
- **Tokens,** as CSS custom properties:
  - colour roles: surfaces, text, borders, the accent, and the states info, success, warning and danger;
  - the type scale, spacing, radii, elevation, the focus ring;
  - motion, which `prefers-reduced-motion` turns off.
- **Themes:** light, dark, and system, which follows the computer as it changes. Accent colours to choose from. Compact density.
- **Contrast:** each pair of text and background the tokens allow meets WCAG 2.2's AA, computed from the tokens by its formula.
- **Icons:** Material Symbols as SVGs in the bundle, through an `Icon` component and a registry of the icons used. None is fetched at run time.
- **Base styles:** a reset, body type, scrollbars, selection.
- **The theme store:** theme and accent, kept by the browser for now. W8.1 keeps them with the account.
- **The gallery,** `#/dev/gallery`: swatches, type, spacing and icons.

**Try it** — Open `#/dev/gallery`. Switch the theme and the accent. With *system*, change the computer's theme: the page follows.

**Tests**
- Every allowed pair's contrast; a token that fails it fails the test.
- Themes: the root's attribute, and `matchMedia`'s changes followed.
- An icon that isn't in the registry fails the type check.
- Visual: the gallery in both themes, the first baselines.

<a id="w0-3"></a>
### W0.3: Components I: actions and inputs

**Goal.** The controls that the tabs are made of: themed, and usable from the keyboard.
**Depends on** W0.2 · **Estimate** 2,000 · **ADRs** 0042

**Scope** (`web/src/ui/`)
- **On reka-ui's primitives,** which nothing outside `src/ui/` imports (ADR-0042).
- **The components:**
  - Button, with its variants, sizes and a busy state; IconButton; ToggleButton, alone and in groups;
  - Checkbox, a Radio group, Switch;
  - TextField, TextArea;
  - NumberField: a unit beside it, a range and a step, arrow keys and the wheel, committed by Enter or on leaving, and an invalid state that says why;
  - Slider, with its number; Select; Combobox;
  - Field: a label, a hint, an error; Tooltip.
- **Each one:**
  - follows the WAI-ARIA Authoring Practices' keyboard pattern for its kind;
  - has disabled and read-only states: the lease's read-only mode (W2.1) uses them;
  - takes its own words from vue-i18n.
- **A gallery page** for each.

**Try it** — Open `#/dev/gallery/inputs`, and use every control with the keyboard alone, in both themes.

**Tests**
- Unit: each component's keys, states and events.
- End to end: axe on each gallery page.
- Visual: each page in both themes.

<a id="w0-4"></a>
### W0.4: Components II: overlays and feedback

**Goal.** What appears over the page, and what tells the user something happened.
**Depends on** W0.3 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/ui/`)
- **Overlays:** Dialog, and a confirmation whose dangerous kind is typed to confirm; Popover; Menu; ContextMenu.
- **Feedback:** Toasts, queued, with levels and an action; Banner; Progress as a bar and a ring, with and without a known end; Skeleton; EmptyState; Badge; Tag.
- **Focus** is trapped in a dialog and returned on closing it. Escape closes. The page behind doesn't scroll.
- **Toasts** are announced through a live region, and stay while hovered or focused.

**Try it** — Open `#/dev/gallery/overlays`: open each overlay from the keyboard, and check where focus goes when it closes. Raise toasts of each level.

**Tests**
- Unit: focus trapping and return, Escape, the toast queue.
- End to end: axe with each overlay open.
- Visual: each in both themes.

<a id="w0-5"></a>
### W0.5: Components III: layout and data

**Goal.** What `microscope` lacked: tabs, splitters, data tables and trees, with the rest of the layout's pieces.
**Depends on** W0.4 · **Estimate** 2,000 · **ADRs** —

**Scope** (`web/src/ui/`)
- **Layout:** Tabs; Splitter, with panes that resize and keep their sizes; ScrollArea; Toolbar; Card; Collapsible.
- **Data:**
  - DataTable: columns that sort and resize, row selection, a sticky header;
  - a virtual list under it, for 100,000 rows: the Logging and Jobs tabs need it;
  - Tree; Pagination; a key-and-value list.
- **Keys:** the grid pattern for the table, the tree pattern for the tree, and the splitter's handle moved by arrows.

**Try it** — Open `#/dev/gallery/data`: sort and scroll a table of 100,000 rows, resize the panes, and walk the tree with the arrow keys.

**Tests**
- Unit: sorting, selection, the virtual list's window, the tree's keys, the splitter's sizes kept.
- End to end: axe; scrolling 100,000 rows stays under a frame's budget, measured.
- Visual: each in both themes.

<a id="w0-6"></a>
### W0.6: Forms from JSON Schema

**Goal.** A form drawn from a JSON Schema, for everything the server describes that way: scan parameters, node parameters, job parameters, extension panels.
**Depends on** W0.5 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/ui/schema-form/`)
- **`SchemaForm`** takes a schema and a value, and gives the changed value and its faults.
- **The subset** the server's schemas use, as `schemars` writes them:
  - objects, numbers and integers with ranges, strings, enums, booleans, arrays, optional values;
  - `oneOf` with a tag; `$ref` within the document;
  - defaults, titles and descriptions.
- **Validation:** in the client for that subset. The server's refusals (422) are put on the fields they name.
- **A construct outside the subset** shows as JSON to edit, and never as nothing.

**Try it** — Open `#/dev/gallery/forms`: pick one of the server's schemas, a scan pattern's or a node's, and fill it in. Enter a value out of range: the field says why.

**Tests**
- Unit: each construct, defaults, faults.
- Contract: every schema the server publishes, in its Thing Descriptions, `scan/patterns`, `preview/nodes` and `jobs/kinds`, draws without falling back to JSON; a new construct on the server fails this test.

<a id="w0-7"></a>
### W0.7: Charts

**Goal.** The few charts the app needs, drawn by the app: lines, histograms and points.
**Depends on** W0.5 · **Estimate** 1,200 · **ADRs** — · **Spec-first**

**Scope** (`web/src/ui/charts/`)
- **On a canvas,** coloured from the tokens, with no charting dependency.
- **Kinds:** line and area, histogram, scatter; several series; a band between two series, for percentiles.
- **Axes:** ticks at round numbers (Heckbert's nice numbers), in the value's unit; linear and logarithmic.
- **Reading:** a cursor with the value under it; zoom and pan; reset.
- **Fast enough** for a 4,096-bin histogram redrawn ten times a second.
- **A table** of the same numbers behind each chart, for screen readers.

**Try it** — Open `#/dev/gallery/charts`: a live histogram, a line profile and a focus curve, from recorded data. Zoom, pan, and read values off the cursor.

**Tests**
- Unit: ticks for awkward ranges, log scales, the cursor's nearest point.
- Visual: each kind in both themes.
- The histogram's redraw time, measured.

---

## Stage W1: Client plumbing

How the app talks to the server: typed calls, live state, the connection and the lease, the platform, and the preview stream.

<a id="w1-1"></a>
### W1.1: Generated API types and the typed client

**Goal.** Calls to the server are typed from the server's own description, and can't drift from it.
**Depends on** W0.1 · **Estimate** 1,300 · **ADRs** 0043

**Scope** (`web/src/api/`)
- **`npm run api:types`** turns `oscope-server --print-openapi` into TypeScript with `openapi-typescript`. The file is committed, and the checks fail when it is stale (ADR-0043).
- **The client,** over `fetch`:
  - paths, parameters and bodies typed from it;
  - errors as one typed model: the status, the server's message, and a device's refusal (ADR-0034);
  - time limits, and aborts.
- **Actions:** started, then followed to their end through `action_invocations/{id}`, or cancelled.
- **Jobs:** an action's `JobRef` followed through `jobs/job/{id}`.
- **The media plane's routes** aren't in OpenAPI: tiles, volumes, downloads, the log's tail. Each gets a typed wrapper written by hand, and a contract test.

**Try it** — `npm run api:types` after changing a Thing's property on the server: the type check then fails wherever the app used the old shape. `#/dev/gallery/api` calls an action and shows its invocation's states.

**Tests**
- Contract: each wrapper against a real server; an action followed to completion, to a failure, and cancelled.
- The stale check: a changed OpenAPI document fails it.
- Unit: the error model, from recorded answers.

<a id="w1-2"></a>
### W1.2: Things, live properties and events

**Goal.** A Thing's state in the app is live: read once, then kept by the server's events.
**Depends on** W1.1 · **Estimate** 1,500 · **ADRs** 0044

**Scope** (`web/src/api/things/`)
- **The Thing Descriptions,** read once from `thing_descriptions/`: each Thing's properties, actions and events, and what may be written.
- **`useProperty`, `useAction`, `useEvent`:**
  - a property is read at its first use, and kept by its Thing's WebSocket;
  - without the WebSocket, it is read again at intervals;
  - a write shows at once, and goes back if the server refuses it, with the reason at the field.
- **One WebSocket a Thing in use,** shared, opened again with back-off. State older than a few seconds without the server is marked stale.
- **Pinia stores,** a Thing each, on these.
- **Under security,** the token goes in the WebSocket's first message (ADR-0031).

**Try it** — `#/dev/gallery/things` shows the stage's position and status. Move the stage with `curl`: the page follows without reloading. Stop the server: the values are marked stale, and recover when it returns.

**Tests**
- Contract: a property followed through writes from another client; events in order; the WebSocket reopened after the server restarts.
- Unit: the write that is refused goes back; staleness.
- Broken on purpose: with the WebSocket's handler removed, the follow test fails.

<a id="w1-3"></a>
### W1.3: Connection, login and the control lease

**Goal.** The app knows whether it has a server, who is using it, and whether it may change anything.
**Depends on** W1.2 · **Estimate** 1,900 · **ADRs** 0045

**Scope** (`web/src/api/connection/`, `web/src/features/login/`)
- **The connection:** the page's own origin, or a server's address in development and in the desktop app. `/health` is watched. Its states: connecting, connected, reconnecting, refused, and a server whose version the app doesn't know.
- **Login,** when the server has accounts:
  - the page, and the token kept for the session;
  - logging out;
  - a session that ended returns to the login, then to where the user was.
- **The lease** (`auth`): who holds it, taking it, asking for it, handing it over, giving it up, and a warning before it lapses.
  - `useCanControl()` is what every control asks.
  - A server without accounts always answers yes.
- **Media under security** (ADR-0045, settling ADR-0031's open point): images, tiles and downloads are fetched with the token and shown as blobs. No cookies, and no tokens in URLs.
- **Uploads and downloads,** with progress, for later phases.
- **Server:** the origins a browser may call from, for the dev server and the desktop app, as `[network] origins`.

**Try it** — Add an account (`oscope-server account add`), restart, and open the app: it asks for the login. Open a second browser: the first holds the lease, and the second's controls are read-only until it asks and the first hands over.

**Tests**
- End to end, with two browser contexts: login, a wrong password, the lease taken, asked for and handed over, a session that ends.
- Contract: an image fetched with the token; without it, 401.
- Unit: the connection's states.

<a id="w1-4"></a>
### W1.4: The host adapter

**Goal.** Everything the platform provides goes through one interface, so that the desktop app replaces it in one place.
**Depends on** W1.3 · **Estimate** 800 · **ADRs** 0046

**Scope** (`web/src/host/`)
- **`HostAdapter`** (ADR-0046):
  - storage by key;
  - dialogs to save and to open a file; the clipboard; notifications;
  - the window's title, full screen, links opened outside the app;
  - what the platform can do: the shared-memory preview and tiles from disk, both absent in a browser.
- **The browser's implementation.**
- **W0.1's lint rules** now name the adapter as the way.
- The theme and the connection keep their state through it.

**Try it** — `#/dev/gallery/host`: save a file, open one, copy text, raise a notification, go full screen.

**Tests**
- Unit: the browser's implementation against a faked `window`.
- End to end: a download reaches the disk; storage survives a reload.

<a id="w1-5"></a>
### W1.5: The preview client

**Goal.** The live stream, decoded off the main thread and drawn at the display's rate (D6, ADR-0014).
**Depends on** W1.3 · **Estimate** 1,800 · **ADRs** 0047

**Scope** (`web/src/api/preview/`)
- **A Web Worker** holds the WebSocket:
  - it reads ADR-0014's header, and refuses a version it doesn't know;
  - it acknowledges, so the server holds at most two frames for it;
  - it decodes with `createImageBitmap` and hands the bitmap over, the newest replacing one not yet drawn.
- **The renderer:** WebGL2, a textured quad drawn at the display's refresh, with pan and zoom as a transform. A 2D canvas where WebGL2 is missing.
- **Its parameters:** the viewport, the ROI, quality and the greatest rate, sent whole each time.
- **P18's data:** the saturation mask, histograms, profiles, ROI statistics and the focus value, decoded and published to the app.
- **Its numbers:** frames drawn a second, latency from the exposure's end to the draw, frames replaced.
- It reconnects, and rests while the tab is hidden.

**Try it** — `#/dev/gallery/preview`: the simulated camera, live. Its readout shows the rate and the latency. Resize the window: the frames' size follows.

**Done when**
- At 1920 pixels wide, the app draws every frame the simulated camera gives (14.5 a second), with the latency recorded in the notes. W10.3 turns those numbers into budgets.

**Tests**
- Unit: the header, from recorded messages; a wrong version; acknowledgements.
- Contract: frames arrive, and an ROI gets the pixels asked for.
- End to end: the rate drawn against the rate sent, on two cores.
- Broken on purpose: without the acknowledgement, the latest-wins test fails.

---

## Stage W2: Shell

<a id="w2-1"></a>
### W2.1: The shell and navigation

**Goal.** The app's frame: the rail, the routes, and the layout every tab shares.
**Depends on** W0.5, W1.4 · **Estimate** 1,400 · **ADRs** 0048

**Scope** (`web/src/app/`)
- **The NavRail,** as goal.md lists it: Control and Live View, Scan Workflow, Workspace, Gallery, Settings, Jobs, Logging, About and Power, with the status indicator's place.
- **Routes** by hash, each tab a chunk loaded when first opened.
- **The layout:** rail → control pane → view, with a splitter whose place is kept.
- **Keys:** the rail as a toolbar; a list of shortcuts under `?`; focus moved to the tab's heading on a change of route; a link that skips to the content.
- **The read-only mode** (ADR-0048): without the lease, every control is disabled, with one line saying why and the way to ask for control.
- **Failures:** each tab catches its own errors and offers to reload it; an action the server refuses raises a toast with the server's words.
- The window's title follows the tab.

**Try it** — Open the app: the rail, and nine empty tabs. Walk them with the keyboard. With an account and a second browser holding the lease, the first's controls are read-only.

**Tests**
- End to end: each route opens; focus after navigation; the read-only mode on a sample control; axe on the shell.
- Unit: the shortcuts' registry, the error boundary.
- Visual: the shell in both themes.

<a id="w2-2"></a>
### W2.2: Status, connection and lease in the shell

**Goal.** The rail says how the instrument is, whether the server is there, and who is in control.
**Depends on** W2.1 · **Estimate** 1,100 · **ADRs** —

**Scope** (`web/src/app/`, `web/src/features/status/`)
- **The instrument status indicator** (`status/instrument`, `changed`): ok, degraded or fault, with each subsystem and its message a click away.
- **The connection:** its state in the rail, and a page in place of the tabs when there is no server.
- **The lease:** its holder in the rail; another's request raised as a toast that hands over.
- **Notifications:** a drawer of the toasts raised, newest first. W4.1 adds the jobs' ends.

**Try it** — Start the server: the indicator says *fault*, the stage waiting to be homed. Home the stage with `curl`: it turns to *ok*. Stop the server: the tabs give way to the reconnecting page.

**Tests**
- End to end: the indicator follows a fault injected in the simulator, and its clearing; the reconnecting page comes and goes.
- Unit: the worst subsystem decides the colour.

---

## Stage W3: Control and Live View

The first tab, and the largest: the camera, the stage, the lights, the live image and what is drawn over it. It ends with Checkpoint G.

<a id="w3-1"></a>
### W3.1: The live viewer

**Goal.** The live image in its tab: fitted, zoomed and panned, with full resolution where the user looks.
**Depends on** W1.5, W2.2 · **Estimate** 1,500 · **ADRs** 0049

**Scope** (`web/src/features/live/`)
- **The viewer,** on W1.5's client:
  - fit to the pane; zoom about the cursor; pan by dragging;
  - zoomed in, it asks for that region at full resolution, and shows the sensor's pixels as squares past one-to-one.
- **Streaming:** start and stop (`camera/start_streaming`, `stop_streaming`). Its states are shown: not streaming, waiting for a frame, live, and stale.
- **Readouts:**
  - the rate and the latency;
  - the zoom;
  - the place under the cursor, in the sensor's pixels and in the sample's µm (`objective/pixel_at_sample`).
- **The scale bar,** from the objective fitted. Full screen. The view saved as a PNG, which is the preview and not the raw frame.

**Try it** — Open `#/control`: the simulated tissue, live. Zoom into a nucleus until the pixels show. The scale bar shortens as you zoom.

**Tests**
- End to end: frames are drawn; zooming asks for a region, and what comes back is that region.
- Unit: the transform between the pane, the image and the sensor; the scale bar's round lengths.
- Visual: the viewer's chrome, on a frozen frame.

<a id="w3-2"></a>
### W3.2: Camera controls

**Goal.** Every setting of the camera in the control pane, within what the camera says it can do.
**Depends on** W3.1 · **Estimate** 1,400 · **ADRs** —

**Scope** (`web/src/features/control/camera/`)
- **Settings:** exposure, gain, pixel format, the mode with its binning (`set_mode`), the ROI, the trigger, and the temperature shown. Ranges and steps come from `capabilities`.
- **Auto exposure and white balance** (`exposure/auto`, `white_balance`): each a job, with what it found (`last`, `last_white_balance`).
- **What is saved** (`saved`, `settings`), and putting it back.
- **A refusal** from the camera (ADR-0034) is shown at its field, in the camera's words.

**Try it** — Halve the exposure: the image darkens, and the rate rises if the exposure had held it down. Run auto exposure: the job appears, and the exposure moves to what it found.

**Tests**
- End to end: each setting changes the camera; an out-of-range value is refused at the field; auto exposure runs and its value is taken.
- Unit: the fields built from a recorded `capabilities`.

<a id="w3-3"></a>
### W3.3: Stage controls

**Goal.** Move the stage: to a place, by a step, by jogging, from the keyboard, and by pointing at the image.
**Depends on** W3.1 · **Estimate** 1,700 · **ADRs** —

**Scope** (`web/src/features/control/stage/`)
- **What it shows:** the position in the display's unit, whether it moves, and its status: idle, moving, homing, on hold, or in alarm with the reason.
- **Moves:** to a place (`move_to`), by a step (`move_by`), and a jog pad for X, Y and Z with its step sizes and continuous jogging (`jog`). The speed (`speed`). Home.
- **Halt** is always in reach: a button that never scrolls away, and Escape.
- **An alarm** shows its reason, and `clear_alarm`. After a start, the stage waits to be homed, and the pane says so.
- **Limits** come from `capabilities`. A move past them is refused before it is sent.
- **From the keyboard:** with the viewer focused, the arrows jog X and Y, and Page Up and Page Down Z, by the step shown.
- **By pointing:** a double click centres that place, and a drag pans the stage, through the active calibration. Without one, the control says what is missing.

**Try it** — Home the stage. Jog with the pad, then with the arrow keys: the image slides as the stage moves. Double-click a nucleus: it comes to the centre. Start a long move and press Escape: it stops.

**Tests**
- End to end, on the simulator at real distances: a move arrives; a jog moves while held and stops on release; halt stops a move; a double click centres the place within the calibration's error.
- Unit: the limits' check; the keys' map.
- Broken on purpose: with halt's call removed, the halt test fails.

<a id="w3-4"></a>
### W3.4: Illumination and the objective

**Goal.** The lights, and which objective is fitted.
**Depends on** W3.2 · **Estimate** 900 · **ADRs** —

**Scope** (`web/src/features/control/`)
- **Lights:** each channel from `capabilities`, on or off, and its intensity (`set_light`); what is saved.
- **The objective:** the catalogue, and the one fitted (`fit`). Changing it changes the pixel's size at the sample, and with it the scale bar and every µm the tab shows.
- **After a change,** the app offers the objective's check (`calibration/check_objective`), as P32 and P33 left it to.

**Try it** — Dim the transmitted light: the image darkens. Fit the 50× objective: the scale bar and the cursor's µm change, and the app offers the check.

**Tests**
- End to end: a light's intensity changes the image's mean; fitting an objective changes the scale.
- Unit: the lights' list from a recorded `capabilities`.

<a id="w3-5"></a>
### W3.5: Overlays I: crosshair, ROI, grid and profile

**Goal.** Layers drawn over the live image, placed in the image's own coordinates.
**Depends on** W3.1, W0.7 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/live/overlays/`)
- **Layers,** each switched on and off, and remembered: the crosshair, a grid, the scale bar.
- **The ROI:** drawn, moved and resized on the image. It becomes the camera's ROI, or the region the statistics are of.
- **The line profile:** a line drawn on the image, and the raw frame's values along it (P18), charted as they change.
- **ROI statistics:** the mean, the deviation, the least and the greatest, a channel each.
- **Drawn as SVG** in image coordinates, so the layers hold their place under zoom and pan, and their handles work from the keyboard.

**Try it** — Draw an ROI over a nucleus: its statistics follow as you move it. Draw a line across the tissue's edge: the profile shows the step.

**Tests**
- End to end: an ROI drawn in the pane is the region the server reports; the profile's ends match the line's.
- Unit: the geometry under zoom and pan; the handles' keys.
- Visual: each layer on a frozen frame.

<a id="w3-6"></a>
### W3.6: Overlays II: histogram, saturation, false colour and the look

**Goal.** See the frame's values, and set how the preview shows them.
**Depends on** W3.5 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/live/`)
- **The histogram:** 12 bits, a channel each, linear or logarithmic, with its percentiles marked (P18), live.
- **Saturation:** the pixels at the sensor's limit marked on the image, from the mask.
- **False colour:** a colour map applied in the fragment shader, from the display's choice.
- **The look** (`preview/look`): the white balance's gains, with auto white balance's result offered; the black and white points, dragged on the histogram; gamma. Reset.
- **The focus value:** a number and a short trace, to focus by hand.

**Try it** — Raise the exposure until the histogram piles up at the right: the saturated pixels light up. Drag the white point down: the image brightens, and the frame's values don't change.

**Tests**
- End to end: the mask's pixels are where the raw frame is saturated; a look's change reaches the preview.
- Unit: the histogram's bins and percentiles, from recorded messages; the colour maps.
- Visual: the histogram and the false colour.

<a id="w3-7"></a>
### W3.7: Autofocus, capture and the live recipe

**Goal.** The tab's actions: focus, keep a frame, and choose what the preview runs.
**Depends on** W3.6 · **Estimate** 1,400 · **ADRs** —

**Scope** (`web/src/features/control/`)
- **Autofocus** (`autofocus/run`, `measures`, `defaults`): a job, followed through W1.1's client, with its curve plotted from `last`: P34's follow-up.
- **Capture** (`capture/single`, `burst`): into a project chosen or made there, with its name and a rendition, and a link to the record.
- **The live recipe:** a saved recipe, or none (`preview/set_live_recipe`). Its cost is shown against `recipe_budget_ms`. References are captured (`capture_reference`). W7 edits recipes.

**Try it** — Defocus by jogging Z, and run autofocus: the curve appears with its peak, and the image sharpens. Capture a frame into a new project. Choose a recipe: the preview changes.

**Tests**
- End to end: autofocus returns to the simulator's true focus within its tolerance; a capture makes a record; a recipe is set and cleared.
- Unit: the curve from a recorded `last`.

<a id="w3-8"></a>
### W3.8: The free-roam mosaic and the tile viewer

**Goal.** A mosaic that builds as the stage is jogged (D13), in the tile viewer that the scans and the Gallery will use.
**Depends on** W3.3 · **Estimate** 1,500 · **ADRs** 0050

**Scope** (`web/src/ui/tile-viewer/`, `web/src/features/live/mosaic/`)
- **`TileViewer`** wraps OpenSeadragon once (ADR-0050):
  - tiles fetched with the token (W1.3);
  - its controls drawn with the app's components; a scale bar; a navigator;
  - the image's coordinates as the stage's µm.
- **The mosaic** (`live_mosaic/begin`, `halt`, `clear`, `state`): it grows as the stage moves, with the field of view marked.
- A double click on the mosaic sends the stage there.
- Beside the live view, in a split.

**Try it** — Start the mosaic and jog around: tiles fill in behind the field's marker. Double-click a far corner: the stage goes, and the mosaic grows on the way.

**Tests**
- End to end: jogging adds tiles where the stage went; a click moves the stage to that place.
- Unit: the coordinates between the viewer and the stage.
- Visual: the viewer's chrome.

**Checkpoint G** — a demo script and a checklist in W3.8's notes: open the app, home the stage, jog, zoom to full resolution, watch the histogram and the saturation as the exposure changes, run autofocus, capture a frame, and build a mosaic by jogging. It is Milestone 1's Checkpoint A, in the app.

---

## Stage W4: Operations

Jobs, logs, power, and the instrument's state in full. It ends with Checkpoint H.

<a id="w4-1"></a>
### W4.1: Jobs

**Goal.** Every long piece of work, watched and steered from one tab.
**Depends on** W2.2 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/features/jobs/`)
- **The queue and the history** (`jobs/queue`, `jobs/list` by pages, `jobs/job/{id}`), kept live by `jobs/changed`.
- **A job:** its kind, title, account and state; its progress, with the stage it is in and when it should end; its results as links; its error.
- **Steering,** with the lease: cancel, pause and resume, and the queue's order, by dragging or from the keyboard.
- **A job's own log,** from `logs/records`.
- **In the shell:** the hardware job that is running, and each job's end as a notification.
- **Submitting by kind** (`jobs/kinds`, with SchemaForm), in development builds.

**Try it** — Queue three captures and an autofocus. Reorder them, pause one, cancel another. Open a finished job: its results link to its record.

**Tests**
- End to end: the queue follows jobs submitted from elsewhere; cancel, pause and reorder reach the server; an end raises its notification.
- Unit: progress and its ETA; the history's paging.

<a id="w4-2"></a>
### W4.2: Logging

**Goal.** The server's log, searched and followed.
**Depends on** W2.2 · **Estimate** 1,300 · **ADRs** —

**Scope** (`web/src/features/logging/`)
- **Records** (`logs/records`): filtered by level, time, source and text, in a virtual list; a record opened shows its fields.
- **Levels** from trace to critical, coloured from the tokens' states, with a count of each.
- **The live tail,** from the media plane: it holds while the user scrolls up, and follows again at the end.
- **The daily files** (`logs/files`), listed and downloaded.

**Try it** — Filter to warnings from the stage, then move the stage past a limit with `curl`: the refusal appears at the tail. Download today's file.

**Tests**
- End to end: a record emitted (`logs/emit`) appears at the tail and under its filter; scrolling up holds the tail.
- Unit: the filters' query; the list's window over 100,000 records.

<a id="w4-3"></a>
### W4.3: About, Power and the status in full

**Goal.** What the software is, how the instrument is, and the three power actions.
**Depends on** W4.1 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/about/`, `power/`, `status/`)
- **About** (`system`): the version and commit, the operating system, the processor, the memory, the uptime. The licences: the server's notices, and the app's own, made from its packages at build.
- **Power** (`power`): restart the service, reboot the host, shut it down.
  - Each is confirmed by typing its name.
  - Each is refused while a hardware job runs, unless forced.
  - `dry_run` is shown when set.
  - After a restart, the app reconnects by itself.
- **The status in full:** each subsystem's state, message and details: the GPU, the disks with the scratch space and its recordings waiting, the jobs, the extensions.
- In development builds, the simulator's faults (`simulation/inject_fault`), to see each state.

**Try it** — Restart the service from the Power tab: the app shows it reconnecting, then returns to the tab. Inject a camera fault: the status's page and the rail's indicator show it.

**Tests**
- End to end: a restart, with the app back afterwards; reboot and shutdown against `dry_run`, with their confirmations; a refusal while a job runs.
- Unit: the confirmation's typing; the notices' list.
- Broken on purpose: without the confirmation's check, the power test fails.

**Checkpoint H** — a demo script and a checklist in W4.3's notes: queue and steer jobs, find a record in the log, follow the tail, inject and clear a fault, and restart the service.

---

## Stage W5: Scan Workflow

Plan a scan over a picture of the slide, run it, and watch it. It ends with Checkpoint I.

<a id="w5-1"></a>
### W5.1: Scan plans

**Goal.** Choose a pattern and a region, and see the plan before anything moves.
**Depends on** W3.8, W0.6 · **Estimate** 1,800 · **ADRs** —

**Scope** (`web/src/features/scan/`)
- **Patterns** (`scan/patterns`): the built-in planners and the extensions' workflows, each with its form (SchemaForm).
- **The region:** a rectangle drawn on the overview or the mosaic; or corners taken from where the stage is; within the stage's limits.
- **The overview** (`scan/overview`, `last_overview`): a picture of the slide at low magnification, made as a job.
- **The plan** (`scan/plan`): its tiles drawn over the overview in the TileViewer, in their order, with the rows, the columns and the overlap.
- **Its estimates:** tiles, time, and room on the scratch disk.
- **Its warnings:** tiles past the limits, no calibration, too little room.

**Try it** — Open `#/scan`. Make the overview, draw a rectangle over the tissue, and choose *snake*: the tiles appear in order, with the time they will take. Change the overlap: the plan follows.

**Tests**
- End to end: a region drawn gives the server's plan for that region; each pattern's form submits.
- Unit: the tiles' drawing from a recorded plan; the warnings.
- Visual: a plan over the overview.

<a id="w5-2"></a>
### W5.2: Running a scan

**Goal.** Run the plan, and follow it tile by tile.
**Depends on** W5.1, W4.1 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/features/scan/`)
- **Run** (`scan/run`): the project and the name; how to focus (fixed, autofocus every so many tiles, or a focus map); the settle time.
- **Progress** (`scan/progress`): each tile marked on the plan as it is taken, the live mosaic growing under it, and when it should end. Pause and cancel.
- **A fault** is shown with its tile.
- **At its end:** the acquisition's record; stitching it, as a job (P39); opening it in the Gallery.

**Try it** — Run the plan: tiles tick off in snake order while the mosaic fills in. Pause and resume. When it ends, stitch it.

**Tests**
- End to end, on the simulator: a scan of the default sample runs to its end; pause holds the stage; cancel leaves what was taken; the stitch job is queued.
- Unit: progress on the plan, from recorded messages.

<a id="w5-3"></a>
### W5.3: The histo scan

**Goal.** The slide scanner's flow: find the tissue, choose what to scan, focus across it, and scan.
**Depends on** W5.2 · **Estimate** 1,200 · **ADRs** —

**Scope** (`web/src/features/scan/histo/`)
- **The steps,** in order, each kept when going back: the overview; the tissue found (P37); the regions proposed, then accepted, edited or redrawn; the focus map's points; the run.
- **The focus map:** its points on the overview, each one's focus measured, and the surface fitted through them shown.

**Try it** — Start a histo scan on the H&E slide: the tissue's islands are outlined. Drop one, redraw another, measure the focus map, and run.

**Tests**
- End to end: the proposed regions cover the simulator's tissue; an edited region is what is scanned.
- Unit: the steps' state; the focus surface from recorded points.

<a id="w5-4"></a>
### W5.4: Z-scans, stacks and positions

**Goal.** The patterns that go through focus, or visit a list of places.
**Depends on** W5.2 · **Estimate** 1,200 · **ADRs** —

**Scope** (`web/src/features/scan/`)
- **A z-scan** at one place: its range and step, with the count of planes and the data's size.
- **A tiled z-stack:** the same, over a region.
- **Positions:** a list of places, added from where the stage is, named, reordered, and visited in turn.

**Try it** — Plan a z-stack of 21 planes over a small region of the cell sample, and run it. Add three positions by jogging to each, and scan them.

**Tests**
- End to end: a z-stack's planes are at the heights asked for; the positions are visited in their order.
- Unit: the sizes' arithmetic; the list's editing.

**Checkpoint I** — a demo script and a checklist in W5.4's notes: make an overview, run a histo scan of the default slide, watch the mosaic, and stitch it. It is Milestone 1's Checkpoint D, in the app.

---

## Stage W6: Gallery

Projects as cards, what each holds, and its images at any zoom. It ends with Checkpoint J.

<a id="w6-1"></a>
### W6.1: Projects

**Goal.** Every project as a card, found and kept in order (D16).
**Depends on** W2.2 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/features/gallery/`)
- **Cards** (`projects/list`, by pages; `projects/thumbnail/{id}`), kept live by `projects/changed`: the name, the thumbnail, the tags, the sample, the dates, and what it holds.
- **Finding:** by text and by tag; sorted by name or date.
- **Changing,** with the lease: create, rename, tag, describe (`set_metadata`), the sample (`set_sample`), and delete, confirmed with what would go.
- **The grid** is walked with the arrow keys. It says so when it is empty, and shows skeletons while it loads.

**Try it** — Open `#/gallery`: the projects made so far, as cards. Tag two, search for the tag, rename one. Capture a frame in another window: its project's card updates.

**Tests**
- End to end: a project made elsewhere appears; search, tag and rename reach the server; delete asks first.
- Unit: the grid's keys; the paging.
- Visual: the grid, and its empty state.

<a id="w6-2"></a>
### W6.2: A project's records

**Goal.** What a project holds, with each record's story.
**Depends on** W6.1 · **Estimate** 1,400 · **ADRs** —

**Scope** (`web/src/features/gallery/project/`)
- **The project's page** (`projects/project/{id}`): its acquisitions, its results and its reports.
- **A record:** its thumbnail (`thumbnail/{id}/{record}`); its metadata, the camera's settings, the objective, the stage's place, the calibration set, the account and the time; its size and its pyramid's levels.
- **A result's provenance:** the recipe, its inputs and their hashes (P48).
- **Downloads** (`data/{project}/{path}`), through the host adapter, resumed by ranges.
- **Deleting** a record; rebuilding the index (`rebuild_index`).

**Try it** — Open a scanned project: its acquisition and its stitched result, with the recipe that made the result. Download a file, and stop and resume it.

**Tests**
- End to end: each kind of record is listed with its metadata; a download's bytes match the server's file; a resumed download completes.
- Unit: the provenance's display, from a recorded result.

<a id="w6-3"></a>
### W6.3: Imports, exports and uploads

**Goal.** Files go in from the user's computer, and come out in the formats other software reads.
**Depends on** W6.2, W4.1 · **Estimate** 1,500 · **ADRs** 0051

**Scope** (`web/src/features/gallery/`, `oscope-api`)
- **Server:** an upload route for the client's files, which P30 left to M2 (ADR-0051). Files are staged in the scratch space, limited in size, need the lease, and are removed once used or after a day.
- **Import** (`projects/formats`): TIFF and its stacks, PNG and JPEG, from uploaded files, as a job (`projects.import`).
- **Export:** OME-TIFF, 16-bit TIFF and PNG, as a job (`projects.export`), then its download.
- **Recordings waiting** in the scratch space (`system/scratch`): each ingested into a project (`projects.ingest`), or left.

**Try it** — Drag a TIFF from the desktop onto a project: it uploads with its progress, and the import's job makes an acquisition. Export a record as OME-TIFF and open it elsewhere.

**Tests**
- Rust: the route's limits, the lease, a staged file's removal, and a path that tries to leave the staging folder.
- End to end: an upload, then its import; an export, then its download.
- Contract: an upload cut short leaves nothing staged.

<a id="w6-4"></a>
### W6.4: The deep-zoom viewer

**Goal.** Any image in a project, from the whole slide down to its pixels (D33).
**Depends on** W6.2, W3.8 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/gallery/viewer/`)
- **The TileViewer on a record,** with tiles made on demand (ADR-0028): a navigator, the scale bar, the cursor's place in µm, full screen.
- **Display:** the black and white points, gamma, and a colour map for one channel.
- **Stacks:** a slider through the planes.
- **Two records side by side,** their views linked.
- **A region not seen before** shows its coarse level at once, then sharpens.

**Try it** — Open a stitched slide: zoom from the whole mosaic to single nuclei. Open the scan's raw acquisition beside it, and pan both together.

**Done when**
- A full-screen view of a region never seen before is sharp within §5.20's 150 ms on the owner's computer, recorded in the notes.

**Tests**
- End to end: tiles arrive for each level; a stack's slider changes the plane; linked views move together.
- Unit: the display's transform; the coordinates' readout.
- Visual: the viewer on a fixed slide.

**Checkpoint J** — a demo script and a checklist in W6.4's notes: find the scanned slide's project, read its records' metadata, view the mosaic at every zoom, export a region, and import a TIFF.

---

## Stage W7: Workspace

Where data is processed, analysed and reported: recipes drawn as graphs, analyses with their provenance, annotations, cells and stains, 3D, and reports. It ends with Checkpoint K.

<a id="w7-1"></a>
### W7.1: Recipes over the API

**Goal.** The server keeps recipes and checks them, so that an editor has something to talk to.
**Depends on** W1.1 · **Estimate** 1,200 · **ADRs** 0052

**Scope** (`oscope-api`, `oscope-pipeline`)
- **Server:** the `pipeline` Thing that plan §5.4 lists, and P21 left for the editor (ADR-0052):
  - `nodes`: the catalogue, the stock nodes' and the extensions', each with its ports and its parameters' schema. `preview/nodes` stays, and gives the same;
  - `check`: a recipe's faults, all of them at once, each with the node and the port it is at;
  - the library: recipes listed, read, saved and deleted by name, in the data root;
  - `run`: a recipe on records, as a job.
- The generated types, and the client's wrappers.

**Try it** — With `curl`: save a recipe, list it, and check a broken one: the answer names both of its faults, and where each is.

**Tests**
- Rust: the library's round trip; names that would leave its folder refused; `check` on recipes with a cycle, a type mismatch and a missing input, all reported together.
- Contract: the client's wrappers.

<a id="w7-2"></a>
### W7.2: The graph editor: the canvas

**Goal.** A recipe drawn and edited as a graph (D12).
**Depends on** W7.1, W2.2 · **Estimate** 2,000 · **ADRs** 0053

**Scope** (`web/src/features/workspace/editor/`)
- **Vue Flow** (ADR-0053):
  - nodes from the catalogue, through a palette that searches;
  - ports coloured by their type, and a connection between unlike types refused as it is dragged;
  - delete, duplicate, an automatic layout, a minimap.
- **Undo and redo;** copy and paste.
- **From the keyboard:** focus moves between nodes and ports, and a connection can be made without the mouse.
- **The recipe's JSON** and the graph are one: each loads from the other without loss. A recipe's file is imported and exported.
- **`check`** runs as the graph changes. Its faults show on the nodes and ports they name.

**Try it** — Open `#/workspace`. Drag in a demosaic, a filter and an output, and connect them. Connect two unlike ports: it is refused. Break the graph: the fault shows where it is.

**Tests**
- Unit: the graph to the recipe and back, for every stock recipe; undo and redo; the types' rule.
- End to end: a graph built by hand passes `check`; a broken one shows its faults; the keyboard's path.
- Visual: a graph in both themes.

<a id="w7-3"></a>
### W7.3: The graph editor: parameters and the library

**Goal.** Set each node's parameters, keep recipes, and see one run on the live image.
**Depends on** W7.2, W0.6 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/workspace/editor/`)
- **A node's form** (SchemaForm) in the control pane, with its defaults, and what differs from the saved recipe.
- **The library:** open, save, save under another name, delete. Leaving unsaved changes asks first.
- **Live:** the recipe set as the live recipe (`preview/set_live_recipe`), with the preview beside the graph, and each node's cost against the live budget.

**Try it** — Open a stock recipe, raise a filter's strength, and set it live: the preview changes. Save it under a new name; it is there after a reload.

**Tests**
- End to end: a parameter changed reaches the live preview; save, reload and open give the same recipe.
- Unit: what counts as changed; the costs' display.

<a id="w7-4"></a>
### W7.4: Analyses and results

**Goal.** Run a recipe on a project's records, and keep what it made with how it was made (P48).
**Depends on** W7.3, W6.2 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/features/workspace/`)
- **Running** (`workspace/run`): the recipe, its inputs chosen from a project, and a region, the whole or a part. A job.
- **Results** (`workspace/results/{id}`): each with its provenance.
- **Made again** (`rerun`), and **compared** (`difference/{id}/{a}/{b}`): what differs, and by how much, as a table.
- **Deleting** a result (`delete_result`).

**Try it** — Run the counting recipe on a region of the mosaic. Make it again: the table is the same. Change the least area and run it: the difference names the parameter and the counts.

**Tests**
- End to end: a run makes a result with its provenance; a rerun's difference is empty; a changed parameter shows in it.
- Unit: the difference's table, from recorded answers.

<a id="w7-5"></a>
### W7.5: Annotations and measurements

**Goal.** Mark an image and measure on it, in the sample's units.
**Depends on** W6.4 · **Estimate** 1,800 · **ADRs** — · **Spec-first**

**Scope** (`web/src/features/workspace/annotate/`)
- **On the TileViewer:** points, lines, rectangles, ellipses and polygons. Each is selected, moved, reshaped by its vertices, labelled, and deleted.
- **Measurements,** shown as a shape is drawn: lengths in µm and areas in µm², from the record's calibration. Polygons by the shoelace formula.
- **A table** of them, exported as CSV.
- **Kept with the record** (`workspace/annotate`, `remove_annotation`, `annotations/…`, `measurements/…`).
- **From the keyboard:** a shape is placed and adjusted with the arrows.

**Try it** — Outline an island of tissue with a polygon: its area shows as it closes. Measure a nucleus across. Reload: both are still there. Export the table.

**Tests**
- Unit: lengths and areas against known shapes, at several zooms; the shapes' editing.
- End to end: shapes survive a reload; the CSV's numbers are the table's.
- Visual: each shape, selected and not.

<a id="w7-6"></a>
### W7.6: Cells and stains

**Goal.** The two stock analyses, seen on the image they were made from (D25).
**Depends on** W7.4, W7.5 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/workspace/analyses/`)
- **Cell counting:**
  - the outlines over the image, at every zoom, drawn only where the view is;
  - the table: each cell's area, shape and place; a row selected shows its cell;
  - histograms of them; the count, and the count per mm².
- **Stains:**
  - the separated channels, for H&E and H-DAB, with the stain vectors used;
  - DAB's positive fraction, and the H-score, each with what it measures.

**Try it** — Count the cells of a region: outlines follow the nuclei, and touching ones are split. Click a row: the view goes to that cell. On the H-DAB slide, view the DAB channel alone, and read its positive fraction.

**Tests**
- End to end: the outlines' count is the table's; a row leads to its cell; the stain's numbers are the server's.
- Unit: which outlines a view needs; the histograms' bins.
- Visual: outlines over a fixed region.

<a id="w7-7"></a>
### W7.7: 3D: surfaces

**Goal.** A depth-from-focus result as a surface to turn and measure (P42).
**Depends on** W7.4 · **Estimate** 1,600 · **ADRs** 0054

**Scope** (`web/src/features/workspace/three-d/`)
- **VTK.js,** in a chunk of its own, loaded when first needed (ADR-0054).
- **The surface:** the height map, with the all-in-focus image as its texture; lit; or coloured by height.
- **Its controls:** turn, zoom, the vertical exaggeration, the colour map, and a reset.
- **A profile** along a line drawn on it, charted in µm.
- **The data** comes from the media plane's volume routes, coarser from afar.
- **Without WebGL2,** the tab says so, and shows the height map as an image.

**Try it** — Open the reflective surface's depth-from-focus result: turn it, raise the exaggeration, and draw a profile across a step. Its height is the simulator's.

**Tests**
- End to end: the surface draws; a profile across a known step gives its height within the method's error.
- Unit: the mesh from a small height map; the profile's sampling.
- Visual: a fixed surface from a fixed camera.

<a id="w7-8"></a>
### W7.8: 3D: volumes and slices

**Goal.** A z-stack as a volume, and as slices through it (D8).
**Depends on** W7.7 · **Estimate** 1,700 · **ADRs** —

**Scope** (`web/src/features/workspace/three-d/`)
- **The volume:** bricks fetched by level, rendered by ray casting, a channel each; a transfer function drawn on a chart; clipping.
- **Slices:** the three orthogonal planes, linked to the volume, with a slider through each.
- **Deconvolution** (P43): before and after, side by side, their views linked.
- **Memory is bounded:** bricks are kept to a budget, and the least recently used go.

**Try it** — Open the cell sample's stack: turn the volume, clip into it, and move the slices. Open its deconvolved result beside it: the structures sharpen.

**Tests**
- End to end: the volume draws; a slice's pixels are the server's slice.
- Unit: the bricks' cache at its budget; the transfer function's mapping.
- Visual: a fixed volume from a fixed camera.

<a id="w7-9"></a>
### W7.9: Reports

**Goal.** A project's results as a report, made, read and restyled in the app (P47).
**Depends on** W7.4 · **Estimate** 1,300 · **ADRs** —

**Scope** (`web/src/features/workspace/reports/`)
- **Making one:** the results to include, and a template; a job.
- **Reading one:** the PDF and the HTML, each in a sandboxed frame, and downloaded.
- **Templates** (`projects/templates`, `template/{name}`, `save_template`, `delete_template`): the source edited beside a preview made from a sample result: P47's follow-up.

**Try it** — Report on the slide's cell count and DAB measure: the PDF opens in the app. Change the template's heading, and make it again.

**Tests**
- End to end: a report is made and shown; a changed template changes the next report; the HTML's frame is sandboxed.
- Unit: the choice of results.

**Checkpoint K** — a demo script and a checklist in W7.9's notes: draw a recipe and run it live; count cells and measure DAB on the stitched slide; annotate and measure; turn a surface and a volume; and make the PDF. It is Milestone 1's Checkpoints E and F, in the app.

---

## Stage W8: Settings

The instrument's settings, its calibration, the simulator's samples, extensions and accounts. It ends with Checkpoint L.

<a id="w8-1"></a>
### W8.1: System, display and devices

**Goal.** The settings that P52 keeps, shown and changed.
**Depends on** W2.2 · **Estimate** 1,800 · **ADRs** —

**Scope** (`web/src/features/settings/`)
- **System** (`settings/system`): the address, the port, the data and scratch roots. When a change waits for a restart (`restart_needed`), the page says so, and offers it.
- **Display** (`settings/display`, `display_defaults`): the theme, the units, the scale bar and the colour map. The app's preferences now live with the account, and W0.2's theme store follows them. No lease is needed for one's own.
- **The whole document** (`settings/document`, `import`): exported to a file, and imported from one.
- **The devices' saved settings:** the camera's, the stage's speed, the lights', the objective fitted: what is saved, and each put back to its default.

**Try it** — Open `#/settings`. Change the units to mm: the stage's readout follows, in this browser and in another logged in as the same account. Change the port: the page asks for a restart.

**Tests**
- End to end: a display preference follows the account to a second browser; a system change shows the restart; export then import gives the same document.
- Unit: the theme store's two sources.

<a id="w8-2"></a>
### W8.2: Calibration sets

**Goal.** Each objective's calibration, seen, run and chosen (D18, ADR-0023).
**Depends on** W8.1, W4.1 · **Estimate** 1,600 · **ADRs** —

**Scope** (`web/src/features/settings/calibration/`, `oscope-api`)
- **The sets** (`calibration/sets`, `active`): listed by objective, activated, exported and imported.
- **The routines,** one at a time, each a job: dark, flat, colour, geometry and stage. Each one's result is shown: the dark frame's level, the flat field as a map, the colour matrix, the geometry's residuals.
- **The objective's check** (`check_objective`, `last_check`).
- **Server:** the preview's look takes the active set's dark frame, flat field and colour matrix when the look asks for them: P33 kept them, and nothing applied them.

**Try it** — Run the flat field for the 10× objective: its map shows the vignetting. Turn *flat field* on in the look: the live image evens out.

**Tests**
- Rust: the look with each correction on and off, against the calibration's own arithmetic.
- End to end: a routine runs and its result shows; activating a set changes the scale.
- Unit: the maps' and residuals' charts.

<a id="w8-3"></a>
### W8.3: The calibration wizard

**Goal.** A new objective calibrated by following steps (P33).
**Depends on** W8.2, W3.3 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/src/features/settings/calibration/wizard/`)
- **The wizard's steps** (`calibration/wizard`, `wizard_start`, `wizard_confirm`, `wizard_retry`, `wizard_skip`, `wizard_cancel`): each with what to do, the live view beside it, its progress, and what it found.
- **It resumes** where it was after a reload, or in another browser with the lease.
- **At its end:** the set, with each step's result, activated or not.

**Try it** — Start the wizard for the 20× objective, and follow it: confirm the objective, let it measure the dark frame and the flat field, and watch the stage step for the geometry. Reload halfway: it is where it was.

**Tests**
- End to end: the wizard run through on the simulator gives a set within the simulator's truth; retry and skip; the reload.
- Unit: the steps' display from recorded states.

<a id="w8-4"></a>
### W8.4: Simulated samples

**Goal.** Choose what the simulator shows, and bring one's own images to it (D23).
**Depends on** W8.1, W6.3 · **Estimate** 1,400 · **ADRs** —

**Scope** (`web/src/features/settings/simulation/`)
- **Samples** (`simulation/samples`, `loaded`): each with its picture and what it is for; load and unload.
- **One's own:** an image or a stack uploaded (W6.3) and imported as a sample (`simulation/import`).
- **The simulator's settings** (`settings/simulator`, `rate`, `confocal`): the clock's rate, the modality, and its imperfections: backlash, tilt, noise.
- **Finding focus** (`in_focus`), and the tile cache's state (`tile_cache`).
- **Faults** to inject, for trying the status.

**Try it** — Load the cell sample: the live view changes to it. Upload a photograph and load it as a sample. Set the clock to 10×: moves finish sooner.

**Tests**
- End to end: loading a sample changes the frames; an uploaded image becomes a sample; the rate changes a move's time.
- Unit: the settings' form.

<a id="w8-5"></a>
### W8.5: Extensions and their panels

**Goal.** Install what third parties wrote, and use the panels it describes (D11, ADR-0036).
**Depends on** W8.1, W0.6 · **Estimate** 1,700 · **ADRs** —

**Scope** (`web/src/features/settings/extensions/`)
- **Installed** (`extensions/installed`, `provided`, `host`): each package, with the nodes, drivers and workflows it brings, and whether it loaded.
- **Changing them,** with the lease: install a `.omext` by uploading it (`extensions/install`), enable, disable, uninstall. What waits for a restart is said (`restart_needed`).
- **Panels** (`extensions/panels`, `panel/{name}`): each drawn from its description: a form (SchemaForm), what it submits to, a node or a Thing's action, and its view of the answer. Each has a route of its own under its extension.
- **The app runs no code from an extension:** a panel is data.

**Try it** — Install the sample Rust extension: after a restart its node is in the catalogue, and its panel opens, takes values, and shows the node's answer.

**Tests**
- End to end: install, restart, and the node is there; a panel submits and shows its answer; uninstall.
- Unit: a panel drawn from each of the sample extensions' descriptions; a description with a construct the app doesn't know says so.

<a id="w8-6"></a>
### W8.6: Accounts

**Goal.** Accounts kept from the app, where M1 had only the command line (D22).
**Depends on** W8.1, W1.3 · **Estimate** 1,400 · **ADRs** 0055

**Scope** (`web/src/features/settings/accounts/`, `oscope-api`)
- **Server:** accounts over the API (ADR-0055): listed, added, removed, and a password changed, with the lease.
  - The first account, on a server that has none, turns security on at the next start; the app says so, and offers the restart.
  - The last account can't be removed while the server listens on the network.
  - One's own password is changed with the old one.
- **The page:** the accounts, the sessions open, and the idle times of sessions and the lease.

**Try it** — On an open server, add an account and restart from the app: it now asks for the login. Add a second account, and change the first's password.

**Tests**
- Rust: each call's rules; the last account; a password changed without the old one refused.
- End to end: the first account, the restart, the login; an account added and removed.

**Checkpoint L** — a demo script and a checklist in W8.6's notes: calibrate an objective with the wizard, load one's own sample, install an extension and use its panel, change the display's units, and add an account.

---

## Stage W9: Desktop (dropped)

Dropped after W7, at the owner's choice: OScope has no desktop app ([ADR-0063](../adr/0063-no-desktop-app.md)). The stage's five phases, W9.1 to W9.5, and Checkpoint M were specified here until then, and are in this file's history.

---

## Stage W10: Quality

The app tested as a whole, measured, packaged, and looked back on.

<a id="w10-1"></a>
### W10.1: Workflows end to end

**Goal.** The checkpoints, as tests.
**Depends on** W8.6 · **Estimate** 1,500 · **ADRs** —

**Scope** (`web/tests/workflows/`)
- **Checkpoints G to L** as Playwright suites, each from a clean data root, on the simulator's default samples and over real distances.
- **Chromium** runs them all; Firefox and WebKit the main path.
- **They wait** for the instrument's own conditions, never for a time.

**Try it** — `npm --prefix web run test:workflows`: the app drives itself through a scan, a stitch, a count and a report.

**Tests**
- Each suite, five times on two cores.
- Broken on purpose: one fault a suite, in what it is there to catch.

<a id="w10-2"></a>
### W10.2: Visual baselines and accessibility

**Goal.** Every tab looks as it should in both themes, and works without a mouse.
**Depends on** W10.1 · **Estimate** 1,200 · **ADRs** —

**Scope** (`web/tests/`)
- **Baselines** of every tab and dialog, in both themes, on fixed data and a fixed seed.
- **axe** on every route.
- **Keyboard only:** the main workflows walked through.
- **Names** for a screen reader on every control.

**Try it** — `npm --prefix web run test:visual` and `test:a11y`. Change a token's colour: the baselines that use it fail.

**Tests**
- The baselines' stability: the same run five times makes no difference.
- A control without a name fails the scan: checked by removing one.

<a id="w10-3"></a>
### W10.3: Front-end budgets

**Goal.** The app's speed and size, measured each night beside the server's (P53).
**Depends on** W10.1 · **Estimate** 1,100 · **ADRs** 0059

**Scope** (`web/tests/perf/`, `xtask`)
- **Budgets** (ADR-0059), set from what W1.5 and W6.4 measured:
  - the live view's rate and latency in the browser;
  - the time to the first frame after opening the app;
  - a cold deep-zoom view;
  - the bundle's size, a chunk each;
  - memory after an hour of live view.
- **`cargo xtask perf`** runs them with the server's, and the nightly soak keeps the app open.

**Try it** — `cargo xtask perf --web`: a report of each budget and its measure.

**Tests**
- A budget broken on purpose fails the run.
- The memory's slope by P53's detector.

<a id="w10-4"></a>
### W10.4: The app in the packages, and the look back

**Goal.** The service serves the app, and Milestone 2 is released.
**Depends on** W10.3 · **Estimate** 1,000 · **ADRs** —

**Scope** (`packaging/`, `docs/`)
- **M1's packages carry the app,** which the service serves at `/` (W0.1's `[web] root`). The deployment guide and its smoke test follow.
- **A user's guide:** a page a tab.
- **0.2.0,** by the release checklist.
- **The look back** on Milestone 2, and what Milestone 3 inherits.

**Try it** — Install the .deb on Ubuntu, and open `http://127.0.0.1:8210/`: the app.

**Tests**
- `test-deb.sh` and the smoke test check that `/` serves the app.
- The guide's links.
