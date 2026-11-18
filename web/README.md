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

**The end-to-end tests** start a server of their own for each test, with its own data and a port the system picks. Before them:

```sh
cargo build -p oscope-server                       # the server they start, or name another with OSCOPE_SERVER_BIN
npm run build                                      # the app they open
npx playwright install chromium firefox webkit     # the browsers, once
```

On Linux, the browsers need system libraries too, installed once as root: `npx playwright install-deps chromium firefox webkit`.

**Screenshots for a phase's notes** are taken by its end-to-end test when `OSCOPE_SHOTS` is set, into `docs/milestone-2/notes/shots/`:

```sh
OSCOPE_SHOTS=1 npx playwright test --project chromium
```

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
