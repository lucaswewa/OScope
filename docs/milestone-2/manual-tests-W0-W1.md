# Manual tests: W0 and W1

A checklist for trying stages W0 and W1 of the app by hand, phase by phase, in the app itself. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** run everything from the repository's root, in Git Bash, on an up-to-date `main`, with Node 24. In PowerShell, use `curl.exe` rather than `curl`.
- **One server for most of it:** the simulated instrument, serving the built app, on a manual-check port:

  ```sh
  cargo build --release -p oscope-server
  npm --prefix web ci
  npm --prefix web run build
  target/release/oscope-server --config configs/oscope.toml --port 8211 --web web/dist
  ```

  Then open `http://127.0.0.1:8211/`. A gallery page is `http://127.0.0.1:8211/#/dev/gallery/…`, and each page's header leads to the others.
- **The dev server instead,** for W0.1: `npm --prefix web run dev`, on `http://127.0.0.1:8280/`, passes `/api` and `/health` to the server on 8210.
- **Release builds:** the simulator renders far more slowly in a debug build, and the live preview with it.
- **Both themes:** each gallery page's header has *Theme*. Go round each page in *Light* and in *Dark*.
- **Keyboard first:** most phases are tried with the mouse put aside, with Tab and Shift+Tab.
- **Stopping:** stop a server with Ctrl-C.
- **Pages seen before:** the pictures in each phase's notes, and its visual tests, are the only times these pages have been looked at. The rest was checked by tests.

## W0.1: the workspace and the first page

With the server on 8210 and `npm --prefix web run dev`, open `http://127.0.0.1:8280/`.

- [ ] It shows **OScope** and **Server 0.1.1**.
- [ ] Stop the server: within a few seconds it says **The server can't be reached.** Start it again: the version returns.
- [ ] Served by the server itself, on 8211, the same page works.

## W0.2: tokens, themes and icons

Open `#/dev/gallery`.

- [ ] It shows the surfaces, the colours, each allowed pair, the type, the spacing, the corners and the icons.
- [ ] *Light* and *Dark* change the page at once. An accent changes the swatches named `accent`, and the titles in light.
- [ ] With *As the computer is*, the page follows the computer's own theme without a reload, and says which scheme is in use.
- [ ] Reload: the theme and the accent are still chosen.

## W0.3: the inputs

Open `#/dev/gallery/inputs`, the mouse put aside.

- [ ] **Buttons:** Enter and Space press *Move*. With *Autofocus is under way* on, *Autofocus* shows a ring, keeps the focus, and takes no press.
- [ ] **Buttons that stay pressed:** on *Objective*, the arrows move, Space presses, and Tab leaves the group in one step.
- [ ] **Choices:** Space ticks *Invert x*. On *Scan order*, the arrows pass *Spiral*, which is switched off. *Set by the instrument* takes the focus and no change.
- [ ] **Numbers:** in *x*, `12,5` and Enter says *This isn't a number.* and keeps 1250; Escape. `300` and Enter; then Arrow Up, Page Down, End, Home. In *Image overlap*, `0.9` and Tab says *At most 0.7.*
- [ ] **The wheel** changes *Step* only while it has the focus.
- [ ] **Sliders** and **lists:** on *Exposure*, the arrows, Home and End; on *Workflow*, Enter opens it, the arrows or a letter move, Enter chooses, Escape closes; in *Node*, `den` narrows it to two.
- [ ] **A tooltip:** Tab to *Fill in*: its tooltip shows, and Escape sends it away.

## W0.4: the overlays

Open `#/dev/gallery/overlays`, with the keyboard alone.

- [ ] **A dialog:** *New scan…* puts the focus in *Name*; Tab never leaves the dialog; the page behind doesn't scroll. In *Project*, Escape closes the list first, then the dialog; the focus is back on *New scan…*.
- [ ] **Confirmations:** *Stop the scan…* opens on *Keep scanning*; Escape is *no*. *Delete the project…* works once `slide-4` is typed.
- [ ] **A popover and the menus:** *Options* takes the focus in and gives it back on Escape. *Actions* passes *Rename*, switched off; Shift+F10 in the dashed box opens the same menu.
- [ ] **Notices:** *Tell*, *Succeed*, *Warn* and *Fail*: three show, the fourth waits; they stay while the pointer rests on them; what failed stays. F8, Tab and Escape send them away.

## W0.5: tables, trees and panes

Open `#/dev/gallery/data`, with the keyboard alone.

- [ ] **The table:** the arrows move; Ctrl+End reaches line 100,000; Enter on *Level* sorts, and again the other way; Shift+→ widens; Space picks a line, shown beside it.
- [ ] **Scrolling** with the wheel, as fast as you like: the rows are there.
- [ ] **The panes:** the handle moves with ← and →, Home and End. Reload: the panes and the columns are as you left them.
- [ ] **The tree,** in *Files*: ↑ ↓ ← →, a letter, and Enter.
- [ ] **The pieces:** the pages' buttons, the sections, and the release notes scrolled with Page Down.

## W0.6: forms from JSON Schema

Open `#/dev/gallery/forms`.

- [ ] *Exposure (ms)* refuses 5000 with "At most 1000.", and Escape puts 20 back.
- [ ] *Region*: choosing *rect* shows its four fields. *White balance*, a height, and *Labels* with an entry renamed.
- [ ] *Slide* is JSON, and says why; *Extra* is JSON by right.
- [ ] *Jobs · reorder · input*: 1.5 in *Position*, then *Let the server check it*: the server's refusal joins the form's own.
- [ ] The second tab draws every schema, and says that none is JSON.

## W0.7: charts

Open `#/dev/gallery/charts`.

- [ ] The cursor follows the nearest point, with each series' value below. The wheel zooms about the pointer; dragging moves; *Show all* shows everything.
- [ ] From the keyboard: ← → Home End, + and −, Page Up and Page Down, and 0.
- [ ] The histogram: *Pause* holds it; *Log scale* marks decades. *The numbers* below a chart opens its table.

## W1.1: the API's types and the typed client

Open `#/dev/gallery/api`.

- [ ] *Move the stage to its middle*, before homing: *Refused by a device*, `alarm`, 409.
- [ ] *Home the stage*: `pending`, `running`, `completed`, then the stage's position.
- [ ] *Move the stage slowly to its far corner*, then *Cancel*: it ends `cancelled`.
- [ ] Each button on the right shows one failure, read as one model.

## W1.2: live Things

Open `#/dev/gallery/things`.

- [ ] Move the stage with `curl`, as W1.2's notes show: the position follows, and each move is listed.
- [ ] Stop the server: within a few seconds the values are marked stale. Start it again on 8211: they are live again.
- [ ] A gain of 300 shows at once, then the gain comes back, with the camera's reason. A gain of 100 is taken.

## W1.3: the connection, the login and the lease

```sh
target/release/oscope-server account add alice     # asks for a password, twice
target/release/oscope-server account add bob
target/release/oscope-server --config configs/oscope.toml --port 8211 --web web/dist
```

Open `#/dev/gallery/connection`.

- [ ] It asks for a login. A wrong password is refused at the field; Alice's lets her in.
- [ ] In a second browser, or a private window, Bob logs in. Alice takes control: Bob's control is read-only. Bob asks; Alice sees it and hands over.
- [ ] Logged out in one of Alice's tabs, her other tab goes to the login, then back to its page.
- [ ] A reload keeps the tab logged in; a new tab asks for a login (W1.4 checks it too).
- [ ] Afterwards, remove the accounts (`oscope-server account remove alice`, and bob), or the next pages will ask for a login.

## W1.4: the host adapter

Open `#/dev/gallery/host`.

- [ ] **Save as a file:** the browser downloads the text. **Open a file:** a text file's name, size, type and first line are shown.
- [ ] **Copy the text,** then paste it anywhere. **Raise a notification,** allowed: it shows outside the browser. From another computer by plain HTTP, the copy still works, and the notification is refused, as the page says.
- [ ] The title changes the tab's. **Full screen** fills the screen; Escape leaves it, and the switch follows.
- [ ] *Something to keep*, and a theme, survive a reload.

## W1.5: the live preview

Put the default slide's tissue under the camera first:

```sh
curl -X POST http://127.0.0.1:8211/api/v1/stage/home -H "content-type: application/json" -d "{}"
curl -X POST http://127.0.0.1:8211/api/v1/stage/move_to -H "content-type: application/json" \
  -d '{"x": 40000, "y": 12000, "z": 5000}'
```

Open `#/dev/gallery/preview`.

- [ ] **Start:** the tissue moves live; the readout gives the frames drawn a second and the latency, and says WebGL2.
- [ ] **Zoom and pan:** the wheel or + and −, dragging or the arrows. Zoomed in, the sensor pixels asked for are fewer, and the picture sharper. 0, or *Whole frame*, goes back.
- [ ] **Resize the window:** the frames' size follows.
- [ ] **Statistics with the frames:** the focus value, the median, the share at full scale and a histogram of red, green and blue.
- [ ] **Hidden:** switch to another tab and back: the readout said it rested, and the frames go on.
- [ ] **The rate:** with the camera in its `design` mode and a 1920 × 1272 region, as W1.5's notes measure, the page draws some 14.5 frames a second, none replaced.

## Afterwards

- Note anything that fails, or looks wrong in either theme, beside its box: fixes go on one branch for this checklist.
