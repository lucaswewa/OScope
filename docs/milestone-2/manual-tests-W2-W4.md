# Manual tests: W2 to W4

A checklist for trying stages W2 to W4 of the app by hand, phase by phase, in the app itself, with Checkpoints G and H. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** run everything from the repository's root, in Git Bash, on an up-to-date `main`, with Node 24. In PowerShell, use `curl.exe` rather than `curl`.
- **One server for most of it:** the simulated instrument, serving the built app, on a manual-check port:

  ```sh
  cargo build --release -p oscope-server
  npm --prefix web ci
  npm --prefix web run build
  target/release/oscope-server --config configs/oscope.toml --port 8211 --web web/dist
  ```

  Then open `http://127.0.0.1:8211/`.
- **Over the tissue:** most of W3 starts there. On Control, press *Home*, then under *Move to* type x `40000`, y `12000` and z `5000` µm, and press *Move* (W3.3).
- **Release builds:** the simulator renders far more slowly in a debug build, and the live preview with it.
- **Both themes:** Settings has the theme. Go round each tab in *Light* and in *Dark*.
- **Keyboard first:** each control works from the keyboard. Try a few with the mouse put aside.
- **Two accounts,** for the lease in W2.1 and W2.2, with the server stopped:

  ```sh
  target/release/oscope-server --config configs/oscope.toml account add alice
  target/release/oscope-server --config configs/oscope.toml account add bob
  ```

  Then log in as each in two browsers, or one and a private window. Afterwards, remove them with `account remove alice` and `account remove bob`: without accounts, the server asks for no login.
- **W4.3's power actions** use a configuration of their own, and a loop that starts the server again after a restart, as its supervisor would. Its section says how.
- **Stopping:** stop a server with Ctrl-C.

## W2.1: the shell

- [ ] **The rail:** the app opens on Control. Tab shows *Skip to the content*; then the rail's arrow keys, Home and End go from tab to tab, and Enter opens one. The tab's heading takes the focus, and the window's title follows.
- [ ] **The keys:** away from a field, 1 to 9 open the tabs, and `?` lists the shortcuts.
- [ ] **A refusal:** before homing, *Centre* on Control is refused, and a notice says so in the stage's words.
- [ ] **The read-only mode:** with the two accounts, alice takes control. Bob's *Home* and *Centre* are off, with a line saying alice is in control, and *Ask for control*. His theme is still his to choose.

## W2.2: the status and the connection

- [ ] **The indicator:** at the rail's foot it says *Fault*, in red, before homing. Pressed, its panel says why, with each subsystem and its message.
- [ ] **Homing:** after *Home*, it turns to *OK*, in green.
- [ ] **A fault:** the camera unplugged and plugged in again, from W4.3's status page or with `curl` (W2.2's notes): *Fault*, the camera *Offline*, then *OK*.
- [ ] **The connection:** stop the server: within a few seconds the rail says *Lost*, the indicator *Unknown*, and a page takes the tab's place. Press another tab: the page stays. Start the server again: the tab pressed comes back.
- [ ] **The notices:** a refusal's notice, sent away, is counted as new on *Notices*, and listed there with when.
- [ ] **The lease:** alice takes control; bob asks for it; alice's notice has *Hand over*; once she presses it, the rail says *You* for bob.

## W3.1: the live viewer

Over the tissue:

- [ ] **Streaming:** *Start*: the state turns from *Waiting for a frame* to *Live*, and the rate and the latency are read out.
- [ ] **Zoom:** the wheel over a nucleus keeps it under the pointer. Past *100 %*, the sensor's pixels show as squares, and *Frame* is the small region asked for. The scale bar shortens. *Fit*, or `0` on the image, shows the whole frame again.
- [ ] **The place:** over the image, the pointer's pixel, and its distance from the sensor's centre in µm.
- [ ] **Waiting and stale:** with the camera on a software trigger (W3.1's notes), the image waits for its first frame; once one is triggered, it is live, then stale two seconds later. Then stop, and set the trigger back to free running: the camera keeps its trigger, after a restart too, and on a software trigger it waits for ever.
- [ ] **The PNG and full screen:** *Save PNG* downloads the view as shown. *Full screen*, or `F` on the image; Escape leaves.

## W3.2: the camera's controls

- [ ] **The exposure:** halved while streaming, the image darkens.
- [ ] **Auto exposure:** its job runs, and the pane says what it found. *Put the saved settings back* returns the exposure.
- [ ] **The mode** can't be applied while streaming, and the pane says so. Stopped, the other mode or format applies.
- [ ] **The region:** a region applied shows that part of the sensor; *The whole sensor* gives it all back; one off the sensor is refused, in the camera's words.
- [ ] **The trigger and white balance:** *Software* makes the camera wait for a trigger; set it back to *Free running* afterwards, as in W3.1. *White balance* says its gains.

## W3.3: the stage's controls

- [ ] **Homing:** *Homing*, then *Idle*, at the home corner.
- [ ] **A move:** to `40000`, `12000`, `5000`: the position follows, and the state is *Moving* until it arrives.
- [ ] **The jog pad:** steps of 100 µm slide the image; *While held* moves until the arrow is let go.
- [ ] **The keys:** on the image, the arrow keys, Page Up and Page Down move by the step shown.
- [ ] **Halt:** a move to x `90000`, then Escape, stops the stage. *Halt*, at the pane's foot, does the same.
- [ ] **A limit:** at x's least, a step towards it isn't sent, and a notice says why.
- [ ] **Pointing:** after the geometry's calibration (W3.3's notes), the readout under the image says where the pointer is on the stage; a double click centres a nucleus; with *A drag moves the stage*, a drag moves it.

## W3.4: the lights and the objectives

- [ ] **A light:** *transmitted* dimmed to 20 % darkens the image; switched off, the camera sees the dark; *reflected* lights it instead.
- [ ] **The objective:** *50x* under *Fitted*: the scale bar shortens, and the µm under the pointer are a fifth of what they were.
- [ ] **Its check:** *Check the objective* makes two small moves, and says whether the pixel measured is the 50×'s.

## W3.5: overlays I

- [ ] **The layers:** *Crosshair* and *Grid* hold their place as you zoom and pan; *Scale bar* hides it; a reload keeps them.
- [ ] **An ROI:** drawn over a nucleus, its statistics follow it, dragged or moved by the keys, Shift for ten pixels. A corner resizes it. *Read out only this region* makes it the camera's.
- [ ] **A profile:** a line across the tissue's edge shows the step. Its ends move by dragging or by the keys.

## W3.6: overlays II

- [ ] **The histogram** follows the frames, a channel each; *Logarithmic* shows the empty bins.
- [ ] **Saturation:** with the exposure raised until the histogram piles up at the right, the saturated pixels are red.
- [ ] **The look:** the white point dragged down brightens the image, and the frame's percentiles don't change. *Put the look back* returns it. *White balance*'s gains are offered.
- [ ] **False colour:** *Turbo* colours the image, and its bar shows; a reload keeps it.
- [ ] **Focus:** along z, the focus value peaks where the image is sharpest.

## W3.7: autofocus, capture and the live recipe

- [ ] **Autofocus:** a few tens of µm out of focus, *Focus* shows its progress, then where it found the focus, and the curve with its peak. The image is sharp again.
- [ ] **Capture:** a new project made, *Capture* shows the record's thumbnail, with a link to the gallery. *Frames* at 5 captures a burst.
- [ ] **The live recipe:** *The green channel alone* turns the preview grey, its cost said against the budget. *Flat field* is refused until its reference is captured, on empty glass. *None* goes back to the look.

## W3.8: the free-roam mosaic, and Checkpoint G

- [ ] **The mosaic:** *Mosaic* over the image opens it beside the view. *Start the mosaic*, then jogging: a frame is drawn where the stage stops, and the field of view's rectangle follows it.
- [ ] **The tile viewer:** zoomed out, the path shows; its scale bar and navigator follow; a double click sends the stage there.
- [ ] **Checkpoint G:** its demo and checklist, in [W3.8's notes](notes/W3.8-free-roam-mosaic.md#checkpoint-g), all ticked.

## W4.1: jobs

With a project made in the capture pane:

- [ ] **The queue:** a 500-frame burst, two single captures and an autofocus queue. The burst runs, the rest wait in line, and the rail's foot shows the running job's progress, leading to it.
- [ ] **Reorder:** dragging, or Alt and the arrows, moves a waiting job.
- [ ] **Cancel and pause:** a waiting capture cancels at once; the burst asks first. A pausable job pauses and resumes.
- [ ] **A job's end:** each end raises a notice, with *Show it*. With the page hidden, the system says so too.

## W4.2: logging

- [ ] **A record at the tail:** with *Error* under *At least* and `out of range` under *Text*, a move past a limit (W4.2's notes) appears at the list's foot.
- [ ] **Holding the tail:** scrolled up while records come, the list stays, and a button says how many came since. Pressed, the list follows again.
- [ ] **A record's fields** show beside the list when it is pressed.
- [ ] **Today's file** downloads under *Daily files*.

## W4.3: About, Power and the status in full

For the power actions, stop the server, and start it again with a configuration that keeps rebooting and shutting down dry runs, in a loop that starts it again after a restart:

```sh
mkdir -p .oscope && cp configs/oscope.toml .oscope/checkpoint-h.toml
printf '\n[power]\ndry_run = true\ndry_run_restart = false\n' >> .oscope/checkpoint-h.toml
while :; do
  target/release/oscope-server --config .oscope/checkpoint-h.toml --port 8211 --web web/dist
  [ $? -eq 75 ] || break
done
```

- [ ] **About:** the server's version and commit, the operating system, the processor, the memory, and how long the server has been up. The app's own licences, each package's text a press away. A server built from source has no notices of its own to list: the packages put them beside it.
- [ ] **The status in full:** the rail's indicator, then *The status in full*: each subsystem's state, message and measures, such as the scratch disk's room.
- [ ] **The simulator's faults:** *Inject* by *The camera unplugged*: the camera's card and the rail's indicator turn to a fault; *Clear*: well again.
- [ ] **Reboot and shutdown:** each waits for its name, typed; then says, in this dry run, what it would have run, and does nothing more.
- [ ] **A refusal:** with a burst running, *Shut down…* is refused, naming it; *Force it…*, its name typed again, goes on, as the dry run does.
- [ ] **A restart:** *Restart…*, `restart` typed: the tab says it is restarting; the server stops, and the loop starts it again; then the tab says the service restarted, and About's *Up for* starts again from seconds.
- [ ] **Checkpoint H:** its demo and checklist, in [W4.3's notes](notes/W4.3-about-power.md#checkpoint-h), all ticked.

## Afterwards

- Remove the two accounts, with the server stopped: `account remove alice` and `account remove bob`.
- Delete `.oscope/checkpoint-h.toml`.
