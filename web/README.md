# OScope's app

`web/` is OScope's app ([plan §8](../docs/implementation-plan.md#8-milestone-2-phase-overview), [ADR-0039](../docs/adr/0039-the-web-app-its-layers-state-and-routing.md)): one Vue 3 codebase, built for the browser and, from W9, for the desktop app. It talks to `oscope-server`, which also serves its built files.

## Try it

It needs Node 24. From the repository's root:

```sh
cargo run --release -p oscope-server -- --config configs/oscope.toml   # the simulated instrument, on 8210
npm --prefix web ci                                                    # the packages: once, and when the lock file changes
npm --prefix web run dev                                               # the app, on http://127.0.0.1:8280/
```

- **The dev server** passes `/api` and `/health` on to the server on port 8210. Set `OSCOPE_SERVER` to another address to use another.
- **The page** shows OScope and the server's version. Stop the server, and within a few seconds it says the server can't be reached; start it again, and the version returns.

The server serves the built app itself, at its own address:

```sh
npm --prefix web run build
cargo run -p oscope-server -- --web web/dist    # http://127.0.0.1:8210/
```

## Its checks

Each is an npm script, run from `web/`. The replays and CI run them all ([ADR-0040](../docs/adr/0040-testing-the-app.md)).

| Script | Checks |
|---|---|
| `npm run api:check` | The API's types, `src/api/generated/openapi.ts`, against the server's description of its API, after `cargo build -p oscope-server`. `npm run api:types` writes them again |
| `npm run typecheck` | The types: the app's, and the configurations', scripts' and tests' |
| `npm run lint` | ESLint, with the layers' rules, and Prettier's formatting. `lint:fix` applies both |
| `npm run test:unit` | Vitest: the app's logic, and the workspace's tools in `tests/tools/` |
| `npm run build` | The app, into `dist/`, and the names of the packages in the bundle, into `.bundle/` |
| `npm run licences` | Every package's licence, after a build. `-- --list` prints them by count |
| `npm run advisories` | npm's advisories, after a build |
| `npm run test:e2e` | Playwright, on the built app: Chromium for every test, Firefox and WebKit for those tagged `@main` |

**The end-to-end tests** start a server of their own for each test: its configuration, data and scratch space in a folder of the test's, on a port the system picks, with power actions that only say what they would do. Before them:

```sh
cargo build -p oscope-server                       # the server they start, or name another with OSCOPE_SERVER_BIN
npm run build                                      # the app they open
npx --no-install playwright install chromium firefox webkit   # the browsers, once
```

On Linux, the browsers need system libraries too, installed once as root: `npx playwright install-deps chromium firefox webkit`.

**A failure's trace** is in the run's own folder, `test-results/<date>-<time>-<process>/`: the failure says where, and how to open it. Each run has a folder of its own so that several can go at once, as `scripts/two-core.sh web:e2e` runs them. A run with no failure removes its folder; delete the others once read.

**Screenshots for a phase's notes** are taken by its end-to-end test when `OSCOPE_SHOTS` is set, into `docs/milestone-2/notes/shots/`:

```sh
OSCOPE_SHOTS=1 npx playwright test --project chromium
```

**The visual tests** compare a page with its baselines, in `tests/e2e/baselines/`: one picture for each system, since each draws text its own way ([ADR-0041](../docs/adr/0041-design-tokens-and-themes.md)).

- **They run only with `OSCOPE_VISUAL=1`,** on the computer the baselines were made on. The replays and `scripts/two-core.sh web:e2e` set it. Without it they are skipped.
- **After a change that is meant to be seen,** [`scripts/baselines.sh`](../scripts/baselines.sh) makes them again, on Windows and under WSL ([`scripts/README.md`](../scripts/README.md#the-visual-tests-baselines-baselinessh)).

## Its look

The look is in tokens, CSS custom properties in [`src/ui/tokens/tokens.css`](src/ui/tokens/tokens.css) ([ADR-0041](../docs/adr/0041-design-tokens-and-themes.md), [`look.md`](../docs/milestone-2/look.md)). Open `#/dev/gallery` to see them all, and to change the theme and the accent.

- **A colour is written only in the tokens.** A component names a role: `var(--surface-pane)`, `var(--text-muted)`. A test refuses a colour anywhere else in `src/`.
- **What may be written on what** is in [`src/ui/tokens/pairs.ts`](src/ui/tokens/pairs.ts). A test computes each pair's contrast, in both schemes and with every accent. Text on a background that isn't a pair there isn't promised to read: add the pair.
- **An icon** is `<Icon name="…" />`, from the registry in [`src/ui/icons.ts`](src/ui/icons.ts). To use one the registry lacks, import its file from Material Symbols there, under the name the app will know it by.

## Its components

The controls are in `src/ui/`, one file each, on reka-ui's primitives ([ADR-0042](../docs/adr/0042-the-component-library-on-reka-ui.md)). Open `#/dev/gallery/inputs` to try every control, in each of its states, `#/dev/gallery/overlays` for dialogs, menus, notices and the rest, `#/dev/gallery/data` for tabs, panes, a table of 100,000 rows, a tree and the rest of a page's pieces, `#/dev/gallery/forms` for forms drawn from JSON Schemas, every one the server publishes among them, and `#/dev/gallery/charts` for charts of data recorded from the simulated instrument.

- **A tab imports the app's components,** never reka-ui: `import Button from "@/ui/Button.vue"`.
- **Every control takes** `id`, `label`, `disabled`, `readonly` and `invalid`, and its value as `v-model`. Read-only is reached by the keyboard and changed by nothing; switched off is passed by.
- **A `Field` names its control:** `<Field label="Sample ID" hint="…"><TextField v-model="name" /></Field>`. Nothing else is wired.
- **A control's own words** are in [`src/ui/words/en.ts`](src/ui/words/en.ts); a page's are the app's, in `src/app/locales/`.
- **A style for what is drawn at the page's root,** a list, a tooltip, a dialog, goes in `src/ui/styles/`, not in the component's scoped style, which doesn't reach it.
- **A `Splitter` and a `DataTable` take the height their page gives them.** A table with none grows to hold every row, and draws only as many as the window can show.
- **What a page keeps between visits,** a pane's size, a column's width, is kept with `kept(key, fallback, valid)` from [`src/host/kept.ts`](src/host/kept.ts): what was kept is used only if `valid` takes it.
- **A card's or a section's title** is a heading at the third level unless told: a page whose own come straight under its title says `:level="2"`.
- **A form from a JSON Schema** is `<SchemaForm :schema="…" label="…" v-model="value" @faults="…" />`, from `src/ui/schema-form/`. It draws the part of JSON Schema the server's schemas use; anything else is JSON to edit, with the reason. A 422's body, passed as `refused`, puts the server's refusals on the fields they name until those change. The contract test in `tests/e2e/forms.spec.ts` fails when the server publishes a construct the forms don't draw.
- **A chart** is `<Chart title="…" :series="[…]" :x="{ title: 'Position (µm)' }" :y="{ title: '…' }" />`, from `src/ui/charts/`: lines, areas, histograms and points, on a canvas coloured from the `--chart-*` tokens, with a table of its numbers behind it. New numbers are new arrays: it draws again when `series` changes. Each drawing is timed on the page's timeline, as `oscope.chart.draw`.
- **The charts gallery's data** are recorded from the simulated instrument by `node scripts/record-charts.mjs`, after `npm run build`, with `oscope-server` built (`OSCOPE_SERVER_BIN` names a release build, which renders far faster).
- **A prop or a model that may be a boolean, and may be left out, says `default: undefined`:** Vue makes a missing one `false`.
- **A notice** is raised from anywhere with `notify({ level, text })` from [`src/ui/toasts.ts`](src/ui/toasts.ts). What failed stays until it is sent away.
- **A component's tests** are beside it, mounted with [`src/ui/test-kit.ts`](src/ui/test-kit.ts). Keys that need a real page (anything that opens, or is measured) are tested end to end, on the gallery.
- **A unit test fails if the console is told anything,** a Vue warning above all, and everything it mounted is unmounted after it ([`tests/setup/unit.ts`](tests/setup/unit.ts)). A test that means to make a warning takes it with `heard()`.
- **An end-to-end test fails if the page throws** an error it didn't catch.
- **A new page of the gallery** gets an axe check and baselines: see `tests/e2e/inputs.spec.ts`.

## Its layers

`src/` has five, and lint keeps what each may import ([ADR-0039](../docs/adr/0039-the-web-app-its-layers-state-and-routing.md)):

- **`ui/`:** the components. The only place that imports reka-ui.
- **`api/`:** the server. The only place that uses `fetch` or a WebSocket.
- **`host/`:** the platform: storage, dialogs, windows. The desktop app replaces it.
- **`features/<tab>/`:** a tab, made of the three above.
- **`app/`:** the shell: the routes, the navigation, the words.

## Calling the server

Calls go through `src/api/`, typed from the server's own description of its API ([ADR-0043](../docs/adr/0043-generated-api-types-and-the-typed-client.md)). Open `#/dev/gallery/api` to call an action and follow it, and to see how the server's failures read.

- **The types** are in `src/api/generated/openapi.ts`, made by `npm run api:types` from `oscope-server --print-openapi`. After a change to the server's API, run it: the type check then fails wherever the app used the old shape. `npm run api:check` fails while the file is stale, and the replays and CI run it.
- **A call** is `call((signal) => server.api.GET("/api/v1/system/version", { signal }), { signal, timeout })`: its value, or an `ApiError`. Its path, parameters and body are checked against the description. It has a time limit, 10 s unless told, and stops when its caller aborts.
- **An action** is `act("/api/v1/stage/move_to", { x, y, z }, { seen })`: started, followed through `action_invocations/{id}` to its end, and its output given. `start`, `follow` and `cancel` take its steps one at a time. Aborting cancels it on the server too.
- **A job** that an action hands back as a `JobRef` is followed to its results by `finished(ref, { seen })`.
- **An `ApiError`** says how a call failed: its `failure` (refused by a device or by the server's checks, missing, out of time…), a device's `refusal` and its `status` (ADR-0034), the `fields` a 422 refused, and the server's `body`, for a form's `refused`.
- **The media plane,** which the description lacks, has wrappers written by hand in `src/api/media.ts`: deep-zoom tiles, volumes' slices and bricks, a project's files, and the logs' live tail. `tests/e2e/contract.spec.ts` checks each, and the client, against a real server.
- **The error model's tests** read the server's own answers, recorded into `src/api/recorded-answers.json` by `node scripts/record-answers.mjs`, after `npm run build`, with `oscope-server` built.
- **A Thing's state, live,** is `useProperty("/api/v1/stage/position")` in a component ([ADR-0044](../docs/adr/0044-things-in-the-app-live-properties-and-events.md)), from `src/api/things/`. Open `#/dev/gallery/things` to see the stage followed.
  - **Its value** is kept by its Thing's WebSocket if the property can be observed. Otherwise it is read every second, or as often as asked (`{ every: 250 }`), and again at each of its Thing's events.
  - **`stale`** says the server hasn't confirmed it for 5 s.
  - **`write`** shows the value at once. If the server refuses it, the last value comes back, with the reason in `refused`.
  - **`useEvent` and `useAction`** do the same for events and actions. Each Thing has one store, `thingStore`, and one WebSocket, opened again with back-off.

## Adding a package

```sh
npm install --before=<the date two weeks ago> <package>
npm run build && npm run licences && npm run advisories
```

- **Its licence** must be on its tier's list ([ADR-0040](../docs/adr/0040-testing-the-app.md)). The lists are in [`scripts/packages.mjs`](scripts/packages.mjs): a shorter one for what the app ships, a longer one for development tools. Another licence goes to the owner.
- **An advisory in a development tool** fails the check until it is listed in [`advisories.json`](advisories.json), with why it doesn't matter here. One in what the app ships can't be listed away.

## On Windows

`npm run test:unit` starts Vitest through [`scripts/vitest.mjs`](scripts/vitest.mjs), from this folder's path as the disk spells it. A terminal may hand Node the folder with a lower-case drive letter, as VS Code's do. Started directly there, Vitest fails every test before it starts, with *Cannot read properties of undefined (reading 'config')*.
