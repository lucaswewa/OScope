# Manual tests: P44–P54

A checklist for trying each phase by hand, from its example. Tick each box as it passes.

## Before you start

- **Where:** run everything from the repository root, in Git Bash, on an up-to-date `main`. In PowerShell, use `curl.exe` rather than `curl`.
- **Release builds:** use `--release` where shown. The simulator renders far more slowly in debug builds, and the times in the READMEs are from release builds at real time.
- **Starting clean:** each example keeps its data in `.oscope/<example>/`. Delete that folder first for a clean start.
- **Ports:** each example here serves on its own: 8213 to 8219, and 8230 to 8233. None of them share one with another in this list.
- **Stopping:** stop a server with Ctrl-C.
- **Pages:** none of P44–P54's pages has been seen in a browser before this checklist. Their scripts ran, and their pages were fetched, in the smoke runs.
- **Some need more:** P49's TLS, P51's real power actions and P54's installs. Each says what.

## P44: annotations and measurement

Run `cargo run --release -p ex-annotate`, then open `http://127.0.0.1:8230/`.

- [ ] It prints a mosaic of about 2598 × 1730 pixels of 0.96 µm, with 5 annotations in 3 layers. The rectangle of 600 × 300 µm measures 180000.0 µm², 1800.0 µm round, in some 20 ms.
- [ ] **Draw** with *Draw* ticked: a polygon (clicks, then a double-click or Enter), a line, and a point. Esc drops a shape half drawn.
- [ ] Each new shape goes in the layer, and under the name, typed above. The table lists its length, area, pixels, and means and standard deviations by colour.
- [ ] *Remove* removes one from the table and the drawing. *Download CSV* gets the same table as a file.
- [ ] Untick *Draw*: a click zooms again, and dragging pans.

## P45: stain separation

Run `cargo run --release -p ex-stain`, then open `http://127.0.0.1:8231/`. It takes about 35 s.

- [ ] **H&E:** the white about 0.75 in each colour; hematoxylin about 0.16 and eosin 0.23 over the tissue; the residual's RMS about 0.03.
- [ ] **Macenko's vectors** within a few degrees of H&E's: about 1.5° and 6°.
- [ ] **H-DAB, the same place:** the tissue's pixels binned negative, weak, moderate and strong; DAB positive about 8 % of the tissue, an H-score about 13.
- [ ] **The page** shows both mosaics and their stains' amounts.

## P46: cell detection and counting

Run `cargo run --release -p ex-count-cells`, then open `http://127.0.0.1:8232/`. It takes about 26 s.

- [ ] About 4,400 nuclei counted, some 4,600 per mm² of tissue.
- [ ] Against the simulator's truth: recall and precision about 0.99, the median miss under a micrometre.
- [ ] **The page** outlines the nuclei on the mosaic. Zoom in: each outline follows its nucleus, and touching nuclei are split.
- [ ] **The table:** `/api/v1/data/<project>/results/<count>/cells.csv`, a row a nucleus, from the IDs the page shows.

## P47: reports

Run `cargo run --release -p ex-report`, then open `http://127.0.0.1:8233/`. It takes about 30 s.

- [ ] It prints a report on 2 results, as `report.pdf` (about 150 kB) and `report.html` (about 160 kB), through OScope's template; and another through the example's own, "blue".
- [ ] **The PDF** opens in the browser: the project, each result with its thumbnail, a region in colour, its table and its chart, in Roboto.
- [ ] **The HTML** shows the same. It is sandboxed: the browser's developer tools show `Content-Security-Policy: sandbox` on it.
- [ ] **The "blue" report** looks different, through the example's template.
- [ ] **A raw mosaic's thumbnail is grey,** as its README says: a known gap, not a fault.
- [ ] **Python's report generator** (optional, needs uv): `OSCOPE_PYTHON=1 cargo test -p oscope-api --test reports` passes.

## P48: the workspace, and Checkpoint F

Run `cargo run --release -p ex-workspace`, then open `http://127.0.0.1:8216/`. It takes about 27 s.

- [ ] The counting recipe on 200 × 150 µm of the mosaic: some 150 nuclei, about 5,100 per mm².
- [ ] **Made again,** the same table. With nuclei of 25 µm² at least: fewer, about 96, and the difference names `min_area`, from 10 to 25.
- [ ] **The page** lists the results with their provenance: the recipe, the input, and its hash.

Then run `cargo run --release -p ex-checkpoint-f`, and open `http://127.0.0.1:8217/`. It takes about 28 s.

- [ ] An H-DAB slide: about 4,450 nuclei, binned by their DAB, about 40 % positive.
- [ ] Across the tissue: DAB positive about 7 %, an H-score about 12. The README says why the two measures differ.
- [ ] **The report** of 5 sections, as PDF and HTML, opens from the page.

## P49: accounts, sessions, the control lease and TLS

Run `cargo run -p ex-two-clients`, then open `http://127.0.0.1:8213/` in two browser windows.

- [ ] It prints: alice took control, homed and moved (200); bob was refused (403); then alice handed over to bob, and bob moved.
- [ ] Log in as `alice` (password `alice's password`) in one window and `bob` (`bob's password`) in the other.
- [ ] Whoever hasn't control sees "take control first" when they move the stage.
- [ ] *Ask for control* shows the request in the other window; *Hand over* passes control.
- [ ] Left for 10 minutes without a change, control goes.
- [ ] **The server's accounts:**
  - `cargo run -p oscope-server -- --data-root .oscope/manual-p49 account add carol` asks for a password twice;
  - `account list` lists her;
  - `account passwd carol` changes it;
  - `account remove carol` removes her.
- [ ] **TLS, on the network,** with a scratch configuration of your own:
  - `[network]` with `address = "0.0.0.0"` and `port = 8211`;
  - its `[storage]` folders under `.oscope/manual-p49`, where carol was added again.

  The server serves over HTTPS with a self-signed certificate, made once in the data root's `tls`. A browser on another computer warns of the certificate, then shows the login. Plain HTTP is refused. Windows may ask to let it through its firewall.

## P50: instrument status and health

Run `cargo run --release -p ex-status`, then open `http://127.0.0.1:8215/`. It takes about 23 s.

- [ ] At the start, "fault": the stage in alarm, homing required. Homed, "all well".
- [ ] Each fault, injected and taken away, shows and clears: the camera unplugged; the stage's alarm; the scratch disk full.
- [ ] The GPU's line names it, with its temperature and memory, or says it's offline without NVIDIA's driver.
- [ ] **The page's indicator** follows the boxes that inject the faults, within a second or two.

## P51: power actions

Run `cargo run -p ex-power`, then open `http://127.0.0.1:8214/`. It's always a dry run.

- [ ] It prints: rebooting unconfirmed is refused (400); shutting down while a job holds the instrument is refused (409), naming it; forced, it says what it would run; then each action's command for this computer.
- [ ] **The page** asks to confirm each action, and says what it would run. The box forces past a job.
- [ ] **A real restart** of the server:
  - start `cargo run -p oscope-server -- --managed --config configs/oscope.toml`, as the desktop app runs it. Its port is in the JSON line it prints;
  - confirm `power/restart` at that port's `/docs`;
  - it exits with code 75, which a supervisor answers by starting it again: `echo $?` in Git Bash.
- [ ] **Real reboots and shutdowns,** only on a computer that may be rebooted, with its configuration's `[power] dry_run = false`, the default:
  - `power/reboot` reboots it;
  - `power/shutdown` shuts it down.

  On Ubuntu, this is the .deb's service with its polkit rule: see P54.

## P52: settings and configuration migration

Run `cargo run -p ex-settings`, then open `http://127.0.0.1:8219/`.

- [ ] It migrates a version-1 file: `[server]` is now `[network]`, and `[devices.simulator]` is now `[simulator]`. Its comments are kept, and `oscope.toml.v1.bak` beside it is the old file: `.oscope/ex-settings/data/config/`.
- [ ] It writes your display preferences, the next start's sample and port, refuses a clock rate of 0, imports the settings' own document, and prints the file.
- [ ] **The page:** change the theme, the units, the colour map or the scale bar. "saved" shows, and reloading the page keeps them.
- [ ] **Import** a document edited in the box, with a part made wrong, such as `"rate": 0`: it is refused, and nothing changes.
- [ ] **By hand:** copy `crates/oscope-server/tests/fixtures/v1/full.toml` to a folder of your own, and run `cargo run -p oscope-server -- --config <the copy> config migrate`. It names what changed, and leaves the old file as `.v1.bak`. Run it again: "already version 2".

## P53: performance budgets and the soak

- [ ] **The report:** `cargo xtask perf --quick --cuda`. It takes about half an hour with its builds cold.
  - Every budget it measures is met, and the recording's length shows "not measured".
  - `target/perf/report.html` shows the same, with this computer named.
- [ ] **The whole report:** `cargo xtask perf --cuda`, with the recording's 10 minutes. It needs about 20 GB of free scratch on the disk, and writes some 350 GB over it.
- [ ] **A soak:** `cargo xtask soak --minutes 60`. It passes, with no preview dropped. `target/soak/soak.json` has its samples.
- [ ] **The nightly, by hand, before turning it on:**
  - start the Soak workflow from GitHub's Actions tab, for its 240 minutes;
  - it should pass. P53's notes say why it might not: memory's slow growth once warm.

  This needs GitHub Actions to start jobs again.

## P54: packaging

- [ ] **The zip, installed,** on Windows, as an administrator:
  - make it, as `packaging/README.md` says;
  - unzip it, and run `powershell -ExecutionPolicy Bypass -File install.ps1`;
  - the service `oscope` runs, and `http://127.0.0.1:8210/health` answers. Mind your own server on 8210: stop it first;
  - `C:\Program Files\OScope` has the program and notices, and `C:\ProgramData\OScope` the configuration and data.
- [ ] **The zip, removed:** `uninstall.ps1` removes the service and the program. `C:\ProgramData\OScope` stays.
- [ ] **The .deb in containers,** with Docker: `bash packaging/linux/test-deb.sh <the .deb> ubuntu:24.04` ends with `test-deb: ok`. A .deb built on 22.04 also passes `ubuntu:22.04`.
- [ ] **The .deb on a real Ubuntu,** with systemd, on a computer that may be rebooted:
  - `sudo apt install ./oscope_0.1.0_amd64.deb`;
  - `systemctl status oscope` shows it running;
  - `curl http://127.0.0.1:8210/health` answers.
- [ ] **Its polkit rule:** confirm `power/reboot` at `http://127.0.0.1:8210/docs`, and the computer reboots: the rule lets the account `oscope` do so. The service is back after the reboot.
- [ ] **Its settings:** change `settings/system`'s port at `/docs`. `/etc/oscope/oscope.toml` changes, with `oscope.toml.bak` beside it.
- [ ] **Removed and purged:**
  - `sudo apt remove oscope` stops the service;
  - `sudo apt purge oscope` removes `/etc/oscope`;
  - `/var/lib/oscope` and the account `oscope` stay.
- [ ] **The release:** see `docs/releasing.md`. The tag is yours to push.
