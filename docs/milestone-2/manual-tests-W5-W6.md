# Manual tests: W5 and W6

A checklist for trying stages W5 and W6 of the app by hand, phase by phase, in the app itself, with Checkpoints I and J. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** run everything from the repository's root, in Git Bash, on an up-to-date `main`, with Node 24. In PowerShell, use `curl.exe` rather than `curl`.
- **One server for all of it:** the simulated instrument, built with CUDA as W5's and W6's try-its were, serving the built app, on a manual-check port:

  ```sh
  cargo build --release -p oscope-server --features cuda
  npm --prefix web ci
  npm --prefix web run build
  target/release/oscope-server --config configs/oscope.toml --port 8211 --web web/dist
  ```

  Then open `http://127.0.0.1:8211/`. Without a GPU, leave out `--features cuda`: the simulator then renders on the CPU, more slowly.
- **Over the tissue:** most of W5 starts there. On Control, take control and press *Home*; then under *Move to* type x 40, y 12 and z 5 mm, press *Move*, then *Start* and *Focus*.
- **A camera region** makes scans and autofocus quicker: under the camera's controls, a region of 960 × 648 about the sensor's middle (W3.2).
- **Both themes:** Settings has the theme. Go round each tab in *Light* and in *Dark*.
- **Keyboard first:** each control works from the keyboard. Try a few with the mouse put aside.
- **A TIFF or two** for W6.3 and W6.4: any 8- or 16-bit TIFF, a multi-page stack among them, or a PNG or a JPEG.
- **Stopping:** stop the server with Ctrl-C.

## W5.1: scan plans

On `#/scan`, its tab *Plan*, the 5× fitted under *The objective*:

- [ ] **The overview:** its region is a standard coverslip at first. Type x 35, y 8.5, width 10.5 and height 7 mm: 16 frames. *Make the overview*: its progress, then the tissue's edge on the glass, in about a minute.
- [ ] **A plan:** *Snake*, then *Draw it*, and drag a rectangle over the tissue: the tiles appear in order, with the time they will take.
- [ ] **The overlap:** changed, the plan follows.
- [ ] **The other patterns:** *Raster*; *Tiled z-stack* with its stack; *Z-scan* at *Here*; *Positions*; and *The tissue found*, which plans over the overview's tissue.
- [ ] **The warnings:** a region past the stage's travel, at x 95 mm and 10 mm wide: its tiles past the limits are counted.

## W5.2: running a scan

With the 10× fitted:

- [ ] **The run:** a *Snake* of x 38.5, y 11, width 3 and height 2 mm: 9 tiles. Choose a project, or make one, and *Run the scan*: the live mosaic fills in under the plan, each tile turning green as it is taken, the next marked, and the pane says how many are done and when it should end.
- [ ] **Pause and resume:** *Pause*: the tile under way ends, and the stage holds. *Resume*: it goes on from the next tile.
- [ ] **Its end:** *Completed: 9 of 9 tiles*, *Open it in the Gallery*, and *Stitch it*: the stitch job's state, then *Open the stitched image*.
- [ ] **A cancel:** run it again, *Cancel…*, then *Cancel the scan*: the stage goes back, and the pane says where the frames so far were kept, in scratch.
- [ ] **Focus:** *By autofocus, every so many tiles*, or *By a focus map, measured first*: the plan's estimate counts the autofocus runs.

## W5.3: the histo scan, and Checkpoint I

The tab *Histo scan*. Checkpoint I's demo and checklist are in [W5.4's notes](notes/W5.4-zscans-stacks-positions.md#checkpoint-i), and repeat this section's steps from the start.

- [ ] **The overview:** x 35, y 8.5, width 10.5 and height 7 mm, *Make the overview*, then *Next*.
- [ ] **The tissue:** its islands, its area and its threshold are said. *Next*.
- [ ] **The regions:** one about each island. Untick *Keep it* on the small one: drawn fainter, and left out. *Draw it again* on the large one, a rectangle of some 2 × 1.5 mm inside it. *Next*.
- [ ] **The focus map:** 2 columns and 2 rows: its points on the view. Fit the 10× on Control: back here, the flow is where it was. *Next*.
- [ ] **The run:** into a project. It measures the map first, then takes the tiles. *The focus map* then says its surface, how well it fits, and each point's focus.

## W5.4: z-stacks and positions

- [ ] **A z-stack:** *Tiled z-stack*, a region of some 0.3 × 0.2 mm about the stage, *Below* 10, *Above* 10, *Step* 1: the estimate says 21 planes. Run it: each tile's 21 planes, from the lowest.
- [ ] **Positions:** *Positions*; jog to a cell, *Add where the stage is*: the line beside the button says where the stage is, and the list gains *Position 1*, at its x, y and z. Add two more; rename one; move the last up, first. Run them: visited in the list's order, each at its own z.
- [ ] **Checkpoint I:** its checklist, in [W5.4's notes](notes/W5.4-zscans-stacks-positions.md#the-checklist), each box.

## W6.1: projects

`#/gallery`:

- [ ] **The cards:** the projects so far, the most recently changed first; with none, it says so, and offers *New project*.
- [ ] **Make three,** tag two, `he, human` and `he`. Under *Tag*, `human`: one card.
- [ ] **Rename one,** then *By name* under *Sort*: in any case, by name.
- [ ] **The grid from the keys:** Tab onto the first card, then the arrows, Home and End.
- [ ] **Kept live:** capture a frame into a project from another window: its card updates, its picture and its count.
- [ ] **Delete one:** the question says what goes with it.

## W6.2: a project's records

- [ ] **The page:** a card's name opens it: its acquisitions, results and reports, each kind's newest first.
- [ ] **A record:** the acquisition's picture, size, levels, frames, camera, objective, calibration and the stage's place; a stitched result's *How it was made*, its steps and what it was made from; a report's files.
- [ ] **A download:** *Download thumbnail.jpg*, saved whole. With the network slowed to a few kB/s in the developer tools, *Stop*, then *Resume*: the file saved is the server's.
- [ ] **Deleting:** an acquisition a result was made from is refused; the result, then the acquisition, go.
- [ ] **Rebuild the index:** it says how many projects.

## W6.3: imports, exports and uploads

- [ ] **On a card:** drag a TIFF onto a project's card: *Importing one file…*, then one acquisition more.
- [ ] **On the page:** drop a file on a project's page: under *Imports*, its upload's progress, then *imported*; *Open* chooses what it made. A `.txt` is refused at once.
- [ ] **Chosen:** *Import files…*; a large file stopped with *Stop* while it uploads.
- [ ] **An export:** a record's *Export*, *OME-TIFF*: when its job ends, *Download*. Open the file in Fiji or QuPath.
- [ ] **A recording waiting:** a scan cancelled part way. In the Gallery, *One recording waiting*: choose a project, *Ingest into …*. The section goes, and the project holds one acquisition more.

## W6.4: the deep-zoom viewer, and Checkpoint J

- [ ] **Open it:** on a stitched result, *View it, deep-zoomed*.
- [ ] **Every zoom:** *Zoom in*, or the wheel, from the whole mosaic to single nuclei; the navigator, the scale bar and the pointer's place in µm follow. *All of it* shows the whole again.
- [ ] **Full screen:** *Full screen*; a region never seen comes coarse, then sharp. Escape leaves.
- [ ] **Its look:** *White point*, *Gamma*, let go: the tiles in the new look. On a one-channel image, a *Colour map*.
- [ ] **Beside it:** the scan's acquisition. Pan either: the other follows. *Plane* walks its tiles.
- [ ] **A stack:** an imported multi-page TIFF's *Plane* walks its pages.
- [ ] **Save the view:** a PNG of the region in view.
- [ ] **Checkpoint J:** its demo and checklist, in [W6.4's notes](notes/W6.4-deep-zoom.md#checkpoint-j), each box.

## Afterwards

- **Projects made here** stay in the data root: delete them from the Gallery, or leave them.
- **Recordings waiting** stay in the scratch space until ingested: a cancelled scan's, ingested or not.
- **Anything that fails:** the phase's notes, under *Follow-ups and known gaps*, may already say it. Else, a note of what was done, what was expected, and what happened.
