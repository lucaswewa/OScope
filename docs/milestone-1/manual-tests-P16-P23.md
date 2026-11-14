# Manual tests: P16–P23

A checklist for trying each phase by hand, from its example. Tick each box as it passes.

## Before you start

- **Where:** run everything from the repository root, in Git Bash, on an up-to-date `main`. In PowerShell, use `curl.exe` rather than `curl`.
- **Release builds:** use `--release` where shown. Debug builds are several times slower, and P22's 20 ms recipe budget assumes release.
- **Starting clean:** each example keeps its data and saved settings in `.oscope/<example>/`. Delete that folder first for a clean start; P16 and P22 also test that settings survive a restart.
- **Shared ports:** run one at a time where two examples share a port: `ex-live-view` and `ex-live-recipe` (8218), `ex-jobs` and `ex-server-hello` (8211), `ex-filters` and `ex-demosaic` (8214).
- **Stopping:** stop a server with Ctrl-C.

## P16: device Things over HTTP

Run `cargo run -p ex-http-walkthrough`, then open `http://127.0.0.1:8212/`.

- [ ] Press **Home**. The position updates about four times a second and ends at home.
- [ ] Move to x = 45000, y = 12500, z = 5000. The position moves at about 10 mm/s and stops at the target.
- [ ] Hold a jog button. The stage moves while you hold it and halts when you let go. **Halt** stops a move partway.
- [ ] Fit the 50× objective. Turn the transmitted light on, then change its intensity. Load "test target", then unload it.
- [ ] In another terminal, `curl -i -X PUT -H "Content-Type: application/json" -d 999 http://127.0.0.1:8212/api/v1/camera/gain` answers 400, saying what was wrong.
- [ ] `curl -N http://127.0.0.1:8212/api/v1/stage/moved` prints an event each time you move the stage from the page.
- [ ] `http://127.0.0.1:8212/docs` lists every route, and you can try them there.
- [ ] **Persistence:** stop the example and start it again. The objective you fitted, the lights and the stage speed come back as you left them.

## P17: live preview

Run `cargo run --release -p ex-live-view --features cuda`, then open `http://127.0.0.1:8218/`.

- [ ] The live H&E image is there. The readout shows the frame rate (about 7/s), the size, the sensor region, the quality and the latency.
- [ ] Scroll on the image to zoom. It gets sharper, up to full resolution. **Whole frame** zooms back out.
- [ ] Hold **x +**. The image slides left at about the pixels per second the page says to expect.
- [ ] Hold **y +**. The image slides up. **z ±** goes in and out of focus.
- [ ] Move the quality slider. The JPEG quality in the readout changes, and so does the image.
- [ ] `curl http://127.0.0.1:8218/api/v1/preview/gpu` says `true`. Run without `--features cuda` and it says `false`, with the page still working.
- [ ] Change the look: `curl -X PUT -H "Content-Type: application/json" -d '{"black": 0.0, "white": 0.5, "gamma": null, "gains": [1.0, 1.0, 1.0]}' http://127.0.0.1:8218/api/v1/preview/look`. The image gets brighter at once.

## P18: live statistics and overlays

Run `cargo run --release -p ex-live-overlays`, then open `http://127.0.0.1:8219/`.

- [ ] Histograms of the raw mosaic (0–4095) and their percentiles update about 10 times a second.
- [ ] **Drag** on the image to draw an ROI. Its statistics appear on the right, and the focus value is measured over it.
- [ ] **Shift-drag** to draw a line. Its profile appears, cell by cell along the line.
- [ ] Hold **z −** or **z +**. The focus value falls and the profile flattens. Coming back to focus, it peaks again.
- [ ] Turn the light to full and set the exposure to 25 ms. Pink saturation marks appear where the slide is brightest, and the histograms pile up at 4095.
- [ ] Toggle **Crosshair** and **False colour**. They change only the overlay, not the statistics.

## P19: sidecar, shared-memory ring, service mode

**The sidecar:** run `cargo run -p ex-sidecar`.

- [ ] It prints the ready line: the port, the process ID, the token's path and the ring's name.
- [ ] Run the `curl ... /system/restart` it prints. It reports "exited with 75", then a new process on a new port.
- [ ] Ctrl-C. It reports the server exited with 0.

**The ring:** run `cargo run -p ex-shm-reader -- --seconds 20`.

- [ ] It prints about 16 previews a second, ending with 0 torn reads and 0 overtaken.

**Windows service mode,** in an administrator's PowerShell:

```powershell
cargo build -p oscope-server
powershell -ExecutionPolicy Bypass -File packaging\windows\service-test.ps1
```

- [ ] It ends with `service-test: ok`, having started, restarted and stopped the service, then removed it. A transcript is in `%TEMP%\oscope-service-test.log`.

**Linux service mode,** under WSL with systemd on:

```sh
cargo build -p oscope-server
packaging/linux/test-user-unit.sh target/debug/oscope-server
```

- [ ] It ends with `test-user-unit: ok`.

## P20: job service

Run `cargo run -p ex-jobs`, then open `http://127.0.0.1:8211/`.

- [ ] Start three **Tours**. They run one after another, and the hardware lock names the running one.
- [ ] **First** moves the last queued tour to the front.
- [ ] While a tour runs, hold **Jog the stage by hand**. It's refused as busy, naming the tour.
- [ ] **Pause** the tour, and jogging is still refused. **Resume** it, then **Cancel** it, and the stage jogs again.
- [ ] Start two **Crunches** of 1500 MiB. They run one after the other, because the budget is 2048. Two of 1000 MiB run together.
- [ ] The history shows each finished job and its results.
- [ ] Stop the example during a tour, then start it again. The history shows that tour as interrupted.
- [ ] `curl -N http://127.0.0.1:8211/api/v1/jobs/changed` streams job changes while you start jobs.

## P21: pipeline engine (a command-line example)

Run `cargo run --release -p ex-pipeline`. It prints, in order:

- [ ] The graph: five steps, a 12 px halo for tiles, and 0 copies.
- [ ] **Run 1**, with each step's time.
- [ ] **Run 2**, with every step `cached`.
- [ ] **Run 3**, after `mask.level=0.06`, with only `mask` run again.
- [ ] **Tile by tile:** 12 tiles, "the same as the whole run, to the bit".
- [ ] **The batch:** progress through three items, then completed, with the files it wrote.
- [ ] `.oscope/ex-pipeline/data/out/mask.png` is white on the nuclei.

Then the variations:

- [ ] `cargo run --release -p ex-pipeline -- --set coarse.radius=20`: only `coarse`, `contrast` and `mask` run again, and the halo becomes 20 px.
- [ ] `cargo run --release -p ex-pipeline -- --set coarse.radius=100`: refused with `nodes[2] (coarse) params/radius: 100 is above the maximum, 64`.
- [ ] `cargo run --release -p ex-pipeline -- --recipe examples/ex-pipeline/recipes/bin-and-measure.json`: a statistics table, and "not this recipe" for tiles.
- [ ] The same with `--features cuda` (before the `--`): `half` and `quarter` run on the GPU, with 2 copies.

## P22: stock nodes I and live recipes

Run `cargo run --release -p ex-live-recipe --features cuda`, then open `http://127.0.0.1:8218/`.

Its README mentions zooming in, but its page has no zoom yet, so that step isn't here.

- [ ] Choose each of the first four recipes. The image changes, and the line under the recipe list says "checked: N ms … within 20 ms".
- [ ] The readout shows the recipe's time on each preview.
- [ ] **Before capturing anything,** choose *Flat field*. It's refused: "there is no reference `flat`".
- [ ] **Capture a flat field.** The slide disappears briefly and comes back, and `flat` is listed. *Flat field, then false colour* is now accepted.
- [ ] **Capture a dark frame.** The light goes off briefly. *Dark frame and flat field* is now accepted.
- [ ] Set the budget to 1 and choose a recipe. It's refused, with its time. Set the budget back to 20.
- [ ] Choose *None*, and the preview's own look returns.
- [ ] **Persistence:** set a recipe, stop the example, and start it again. The recipe is still applied, and `curl http://127.0.0.1:8218/api/v1/preview/live_recipe` shows it.
- [ ] `curl http://127.0.0.1:8218/api/v1/preview/nodes` lists the stock nodes.

The simulator lights the field evenly, so a flat field changes little here.

## P23: stock nodes II

Run `cargo run --release -p ex-filters --features cuda`, then open `http://127.0.0.1:8214/`. The terminal prints each card's times as it renders.

- [ ] The field and the noisy copy appear, then ten cards.
- [ ] The four cards with GPU twins (Gaussian, median, bilateral, CLAHE) show both CPU and GPU times.
- [ ] The **median** removes the black and white specks.
- [ ] The **bilateral** filter smooths the stroma but keeps the nuclei's edges sharp.
- [ ] **CLAHE** brings out the stroma's texture.
- [ ] **Otsu** reports a level of about 0.6.
- [ ] **Opened** removes specks.
- [ ] **The distance transform** is brighter inside each nucleus.
- [ ] **Labelled** shows each nucleus in its own colour, with the count: 118 at 1024 × 768. Nuclei that touch share a colour.
- [ ] Without `--features cuda`, the page says it ran on the CPU only.
