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

The controls are in `src/ui/`, one file each, on reka-ui's primitives ([ADR-0042](../docs/adr/0042-the-component-library-on-reka-ui.md)). Open `#/dev/gallery/inputs` to try every control, in each of its states, and `#/dev/gallery/overlays` for dialogs, menus, notices and the rest.

- **A tab imports the app's components,** never reka-ui: `import Button from "@/ui/Button.vue"`.
- **Every control takes** `id`, `label`, `disabled`, `readonly` and `invalid`, and its value as `v-model`. Read-only is reached by the keyboard and changed by nothing; switched off is passed by.
- **A `Field` names its control:** `<Field label="Sample ID" hint="…"><TextField v-model="name" /></Field>`. Nothing else is wired.
- **A control's own words** are in [`src/ui/words/en.ts`](src/ui/words/en.ts); a page's are the app's, in `src/app/locales/`.
- **A style for what is drawn at the page's root,** a list, a tooltip, a dialog, goes in `src/ui/styles/`, not in the component's scoped style, which doesn't reach it.
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

## Adding a package

```sh
npm install --before=<the date two weeks ago> <package>
npm run build && npm run licences && npm run advisories
```

- **Its licence** must be on its tier's list ([ADR-0040](../docs/adr/0040-testing-the-app.md)). The lists are in [`scripts/packages.mjs`](scripts/packages.mjs): a shorter one for what the app ships, a longer one for development tools. Another licence goes to the owner.
- **An advisory in a development tool** fails the check until it is listed in [`advisories.json`](advisories.json), with why it doesn't matter here. One in what the app ships can't be listed away.

## On Windows

`npm run test:unit` starts Vitest through [`scripts/vitest.mjs`](scripts/vitest.mjs), from this folder's path as the disk spells it. A terminal may hand Node the folder with a lower-case drive letter, as VS Code's do. Started directly there, Vitest fails every test before it starts, with *Cannot read properties of undefined (reading 'config')*.
