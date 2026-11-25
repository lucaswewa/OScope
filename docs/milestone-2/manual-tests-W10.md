# Manual tests: W10

A checklist for trying stage W10 by hand: the workflows, the baselines and accessibility, the app's budgets, and the app in the packages, with 0.2.0's release checklist. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** the repository's root, in Git Bash, on an up-to-date `main`, with Node 24 and the app's packages: `npm --prefix web ci`.
- **The server the tests start** is the debug build, `cargo build -p oscope-server`, and the app they open the built one, `npm --prefix web run build`.
- **A release build of your own,** beside the release server you run from `target/release`: `CARGO_TARGET_DIR=target-perf`. Windows won't replace a program that runs.

## W10.1: workflows end to end

- [ ] **All of them:** `npm --prefix web run test:workflows`: 20 passed, in some three minutes.
- [ ] **One, watched:** `npm --prefix web run test:workflows -- i-histo-scan --headed --project chromium`: the app drives itself through an overview, its regions, a focus map, a run and its stitch.
- [ ] **What they found, by hand:**
  - Scan, then Control, then Scan again: the plan is still drawn;
  - after a histo scan ends, its tiles keep their green for more than ten seconds;
  - a project made in another browser is offered in the Workspace's lists, opened again.

## W10.2: visual baselines and accessibility

- [ ] **The baselines:** `npm --prefix web run test:visual`: all passed.
- [ ] **A token changed:** the accent's `--accent` in `web/src/ui/tokens/tokens.css` to another colour, then `test:visual`: the baselines that use it fail. Put it back.
- [ ] **Accessibility:** `npm --prefix web run test:a11y`: all passed.
- [ ] **Keyboard only, by hand:** with the mouse put aside, Tab to *Home* on Control, home, type a place and move, start the camera, make a project and capture a frame. In the Gallery, Tab to a project's card and press Enter: it opens.
- [ ] **A screen reader,** NVDA or Narrator on Windows, Orca on Ubuntu: each control on Control and in a dialog is said by its name.

## W10.3: front-end budgets

- [ ] **The report:** `CARGO_TARGET_DIR=target-perf cargo xtask perf --quick --cuda --web`: the app's budgets met, and each with its measure in `target-perf/perf/report.html`.
- [ ] **A budget broken:** `web.live.fps`'s bound in `xtask/src/perf.rs` raised to 20, the run again: it fails, saying which. Put it back.
- [ ] **The soak with the app:** `CARGO_TARGET_DIR=target-perf cargo xtask soak --minutes 30 --web`: the server's soak passes, and the app's memory is within its budget.

## W10.4: the app in the packages

- [ ] **The .deb, in clean containers,** with Docker: `bash packaging/linux/test-deb.sh "$(bash packaging/linux/build-deb.sh target-perf/release/oscope-server)" ubuntu:24.04`, from a build on Ubuntu: `test-deb: ok`.
- [ ] **The zip:** `packaging\windows\package.ps1`, then `test-zip.ps1`: `test-zip: ok`.
- [ ] **The zip installed,** from an administrator's PowerShell, unzipped: `install.ps1`, then `http://127.0.0.1:8210/` in a browser: the app. `uninstall.ps1` removes it, the configuration and the data kept.
- [ ] **On Ubuntu 24.04,** by [the deployment guide](../deploying-on-ubuntu.md): Node through nvm, `build-from-source.sh`, install, `smoke-test.sh`'s `smoke-test: ok`, *it serves the web app*, and `http://127.0.0.1:8210/` in a browser.
- [ ] **Over 0.1.1, upgraded,** with `/etc/oscope/oscope.toml` changed before: the app is served all the same.
- [ ] **The user's guide:** [`docs/user-guide/`](../user-guide/README.md), a page a tab, read beside the app.

## 0.2.0, by the release checklist

[`docs/releasing.md`](../releasing.md), on `main` with W10.4 merged:

- [ ] **1, 2:** the version is 0.2.0, every phase *Done*, and both local replays pass on `main`.
- [ ] **3:** `cargo xtask perf --cuda --web`, every budget met; the soak, 240 minutes, `--web`, passes.
- [ ] **4:** the notices; the packages, tried as above.
- [ ] **5, the tag,** by you: `git tag -a v0.2.0 -m "OScope 0.2.0"` and `git push origin v0.2.0`. The release workflow needs GitHub Actions back.
