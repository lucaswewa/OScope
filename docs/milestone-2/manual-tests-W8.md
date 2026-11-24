# Manual tests: W8

A checklist for trying stage W8 of the app by hand, phase by phase, in the app itself, with Checkpoint L. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** run everything from the repository's root, in Git Bash, on an up-to-date `main`, with Node 24. In PowerShell, use `curl.exe` rather than `curl`.
- **One server for most of it:** the simulated instrument, built with CUDA as W8's try-its were, serving the built app, on a manual-check port, with its own configuration and data root, so that its settings and accounts are its own:

  ```sh
  cargo build --release -p oscope-server --features cuda
  npm --prefix web ci
  npm --prefix web run build
  mkdir -p .oscope/w8 && cp configs/oscope.toml .oscope/w8/oscope.toml
  target/release/oscope-server --config .oscope/w8/oscope.toml --port 8212 --web web/dist
  ```

  Then open `http://127.0.0.1:8212/`. Without a GPU, leave out `--features cuda`: the simulator then renders on the CPU, more slowly.
- **Restarts:** W8.1, W8.5 and W8.6 change what the next start reads. Restart from Power where the server runs under a supervisor (ADR-0060); otherwise stop it with Ctrl-C and start it again with the same command.
- **The simulator's imperfections** for W8.2 and W8.3: in `.oscope/w8/oscope.toml`, under `[simulator]`, `vignetting = 8.0`, `rotation = -20.0` and `scale = 0.98`, then a restart. W8.4's *At the next start* writes them too.
- **Both themes:** Settings has the theme. Go round each view in *Light* and in *Dark*.
- **Keyboard first:** each control works from the keyboard. Try a few with the mouse put aside.
- **Accounts last:** W8.6 turns security on. Do it after the rest, or remove the account again before the next restart.

## W8.1: system, display and devices

`#/settings`:

- [ ] **Display:** *Lengths in* mm: the stage's readout on Control follows, and in a second browser within 5 s. *Dark*: the second browser turns dark too. *Back to the defaults*.
- [ ] **The scale bar and the colour map:** *A scale bar on images* off, and a colour map for one-channel images: the Gallery's viewer follows.
- [ ] **System:** the port changed and saved: *Saved: port.*, and the restart said and offered on Power. `0.0.0.0` without accounts: refused, saying why. Put the port back.
- [ ] **Devices:** a light on and an objective fitted on Control show here; *Forget them*, and what the devices have now stays.
- [ ] **The whole document:** exported, something changed, imported back: as it was.

## W8.2: calibration sets

*Calibration*, with the camera streaming on Control:

- [ ] **The flat field,** over clear glass beside the coverslip (x 10,000 µm, y 12,500 µm): its map shows the vignetting, and *Every set* lists it under the 10×.
- [ ] **The look:** on Control, *Histogram*, *Divide by the flat field*: the live image evens out; off again, the vignetting is back.
- [ ] **The geometry,** over the tissue (x 40,000 µm, y 12,000 µm): its pixel, its turn, and each move's residual charted.
- [ ] **A set exported and imported:** the geometry's file, imported back as version 2, not in use until *Use this one*. A dark frame or a flat field has no *Export*.
- [ ] **The scale follows:** an imported geometry, its pixel twice as large in the file, chosen: the live view's scale bar spans twice the µm.

## W8.3: the calibration wizard

*Calibration*, *Start the wizard*, streaming over the tissue:

- [ ] **The objective step:** its pane beside the prompt; *Done: measure it*: *plausible*.
- [ ] **The geometry:** the stage steps in the live image beside it; it says the pixel it found.
- [ ] **A reload:** the page opened again finds the wizard at its third step.
- [ ] **A failed step:** the flat field over the tissue fails, saying why; over glass, *Try it again*, and it passes.
- [ ] **Skipped, cancelled, and in another browser:** a step skipped there is followed here; *Cancel the wizard* ends it in both.
- [ ] **Its end:** each step's set, in use, or *Use this one*.

## W8.4: simulated samples

*Simulation*:

- [ ] **The samples:** each with its picture and what it is for; *Load it* on the colour target: the live view changes to it; *Take the slide off*.
- [ ] **One's own:** *Choose a file…*, a photograph, 10 µm a pixel, *Import it*, *Load it*: at the coverslip's middle, the live view shows it.
- [ ] **The clock:** *Its clock* 10: a stage move finishes ten times sooner. Put it back to 1.
- [ ] **At the next start:** the backlash changed and saved, the restart said; a magnification of 0 said at its field, and *Save* refused.
- [ ] **For trying things:** *Where is it in focus here?*; *The camera unplugged*: the rail's indicator says *Fault*, then *OK* again.

## W8.5: extensions and their panels

`cargo run -p ex-rust-ext -- --pack emboss.omext` first. *Extensions*:

- [ ] **Installed:** *Install a package…*, `emboss.omext`: *Emboss 0.1.0 is installed*, the restart said.
- [ ] **Loaded at the restart:** *Loaded: 0.1.0*, with what it brings.
- [ ] **Its panel:** *Emboss the live view*, at its own address; with the camera streaming, *Run it*: the newest frame in relief. Strength 0: a flat grey.
- [ ] **A package that isn't one:** refused, saying why.
- [ ] **Disabled, enabled and uninstalled:** each said, and the next start's change.

## W8.6: accounts

*Accounts*, last:

- [ ] **The first account:** `ana`, its password typed twice: *Security turns on at the next start.* Restart, and open the app again: it asks for the login.
- [ ] **With the lease:** *Take control*, add `ben`, then remove him, asked first: he can't log in.
- [ ] **A name refused at its field:** `Ben Smith`; *Add it* stays off.
- [ ] **One's own password:** a wrong old one refused; the right one, changed.
- [ ] **Sessions open:** `ana`'s, idle for a while, and the idle times of sessions and the lease.
- [ ] **The last account on the network:** with *System*'s address `0.0.0.0` saved, removing `ana` is refused, saying why. Put the address back.
- [ ] **Removed, at the end:** removing `ana` asks first, says it is yours, and logs you out; security turns off at the next start.

## Checkpoint L

[W8.6's notes](notes/W8.6-accounts.md#checkpoint-l) have its demo and its checklist: an objective calibrated by the wizard, one's own sample, an extension's panel, the display's units, and an account.

- [ ] **The demo, through,** as it gives it.
- [ ] **Its checklist,** each box.
