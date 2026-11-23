# Manual tests: W7

A checklist for trying stage W7 of the app by hand, phase by phase, in the app itself, with Checkpoint K. Tick each box as it passes. Each phase's notes say more, under *How to try it*.

## Before you start

- **Where:** run everything from the repository's root, in Git Bash, on an up-to-date `main`, with Node 24. In PowerShell, use `curl.exe` rather than `curl`.
- **One server for all of it:** the simulated instrument, built with CUDA as W7's try-its were, serving the built app, on a manual-check port:

  ```sh
  cargo build --release -p oscope-server --features cuda
  npm --prefix web ci
  npm --prefix web run build
  target/release/oscope-server --config configs/oscope.toml --port 8211 --web web/dist
  ```

  Then open `http://127.0.0.1:8211/`. Without a GPU, leave out `--features cuda`: the simulator then renders on the CPU, more slowly.
- **A stitched slide:** W7.4 to W7.6 and W7.9 need one. Checkpoint K's first step makes it: in [W7.9's notes](notes/W7.9-reports.md#the-demo). W5.2's try-it makes one too.
- **The surface and the volume** come from the tests' own helpers, which drive the server through its API as the app would. Each loads its own simulated sample and changes the camera's region, so stop the camera on Control first, and load the H-DAB slide again afterwards for the other steps.
- **Both themes:** Settings has the theme. Go round each tab in *Light* and in *Dark*.
- **Keyboard first:** each control works from the keyboard. Try a few with the mouse put aside.
- **Stopping:** stop the server with Ctrl-C.

## W7.1: recipes over the API

With `curl`, as [W7.1's notes](notes/W7.1-recipes-api.md#how-to-try-it) give each command:

- [ ] **The catalogue:** `curl http://127.0.0.1:8211/api/v1/pipeline/nodes`: each node, its ports and its parameters' schema.
- [ ] **A recipe saved, listed and read:** `blur-3`, after the stock recipes.
- [ ] **A broken one checked:** both faults, each with its node and port.
- [ ] **A name that would leave the library,** `../out`: refused.

## W7.2: the graph

`#/workspace`, *Recipe*:

- [ ] **Empty:** *One fault*: it gives nothing.
- [ ] **Nodes added** from *Find a node*: `filter.gaussian`, `threshold.otsu`.
- [ ] **Joined by dragging,** each line its port's colour; two unlike ports turn red, and don't join.
- [ ] **Faults where they are:** the gaussian's `sigma` not set; an edge deleted, the port it fed red.
- [ ] **From the keyboard:** Tab to an output, Enter, Tab to an input, Enter: joined; Delete, Ctrl+Z, Ctrl+Y; Ctrl+C, Ctrl+V and *Duplicate*.
- [ ] **Laid out, exported and imported:** the same graph, where it was.

## W7.3: parameters, the library, and live

- [ ] **A node's form:** *Open…* `levels`; press its node: *Black*, *White*, *Gamma*, *As it was saved*.
- [ ] **Live:** with the camera started on Control, *Live*: the live image beside the graph, and each node's cost. *White* 0.3, Enter: the image brightens, *Changed since it was saved: white*.
- [ ] **Saved under a name of yours,** `my-levels`: OScope's own can't be changed. Reloaded and opened: *White* 0.3.
- [ ] **Leaving with changes:** asked first; *Cancel* keeps them.

## W7.4: analyses and results

*Analyses*, the stitched slide's project:

- [ ] **A run:** *Nuclei counted* on a region of 500 × 500 µm of the mosaic: its progress, then the result listed, its rows and its files.
- [ ] **Made again and compared:** *They were made alike, and made the same.*
- [ ] **A parameter changed:** *Open its recipe*, the `measure` node's *Min area* 30, run from the editor: the comparison names `measure: min_area` and the rows.
- [ ] **Deleted:** asked first, then gone.

## W7.5: annotations

*Annotations*, the mosaic:

- [ ] **A polygon round an island,** its area shown as it would close; closed, listed in µm².
- [ ] **A line across a nucleus:** its length, in µm.
- [ ] **Named and reshaped:** a handle, the shape; the table follows.
- [ ] **From the keyboard:** *Rectangle*, Enter, the arrows, Enter; moved with the arrows, and V its corners.
- [ ] **Kept:** reloaded, still there; *Export as CSV*, its numbers the table's.

## W7.6: cells and stains

- [ ] **A count shown:** *Show it* on *Nuclei counted*: the outlines on the nuclei, touching ones split, the count, the count per mm², the histograms.
- [ ] **A row leads to its cell,** its outline marked.
- [ ] **Stains:** *Stains separated: H-DAB, and DAB scored* on the H-DAB image, *Show it*, *DAB* as the channel shown: DAB alone, in grey.
- [ ] **The score:** its positive fraction and H-score, each with what it measures.

## W7.7: surfaces

With the camera stopped on Control, a depth-from-focus result of the simulated reflective chip:

```sh
node --experimental-transform-types -e 'import("./web/tests/e2e/surfaces.ts").then((m) => m.surfaced("http://127.0.0.1:8211"))'
```

*Surfaces*, the project *Chip*:

- [ ] **The surface:** drawn, lit, its texture the all-in-focus image; turned and zoomed with the pointer.
- [ ] **Its controls:** *Its heights, stretched*; coloured by its height, through a colour map; *Reset the view*.
- [ ] **A profile:** a line dragged across the height map, over the dome: charted, its ends' heights and rise said.
- [ ] **Without WebGL2,** in a browser with it turned off: the tab says so, and the height map's profile still works.

## W7.8: volumes and slices

With the camera stopped on Control, the simulated cells' stack and its deconvolution:

```sh
node --experimental-transform-types -e 'import("./web/tests/e2e/volumes.ts").then((m) => m.deconvolved("http://127.0.0.1:8211"))'
```

*Volumes*, the project *Cells*:

- [ ] **The volume:** *cells in widefield*, drawn; turned and zoomed with the pointer.
- [ ] **Its look:** the transfer function's points dragged; a colour map; a cut from x, y and z; *Reset the view*.
- [ ] **The slices:** each slider moves its slice, and its outline in the volume; the y and x slices are as tall as the stack is deep.
- [ ] **Beside it:** *cells in widefield deconvolved*: the two turn together, either one turned; the deconvolution's membranes tighter.
- [ ] **What is kept** is said under the slices, and grows only to 256 MB.

## W7.9: reports

*Reports*, the stitched slide's project:

- [ ] **Made:** its results ticked; *Make it*: the report shown as HTML. None can be made with no result ticked.
- [ ] **As PDF:** *PDF*: the browser's viewer, or, where it has none, a note to download it. Each file downloads.
- [ ] **A template:** *Templates*, a name `our-lab`, the HTML's heading changed; *Preview* on a result: the report as it would be, as HTML and PDF, kept nowhere. *Save*.
- [ ] **Made again,** through `our-lab`: its heading the new one, shown in place of the one before.
- [ ] **A wrong template,** `{{ report.title` unclosed: *Save* refused, saying the line. A name with a space: refused, as its field says.
- [ ] **Deleted,** after asking.

## Checkpoint K

- [ ] **Its demo and checklist,** in [W7.9's notes](notes/W7.9-reports.md#checkpoint-k), each box.
