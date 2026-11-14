# P22: Stock nodes I: correction and display; live recipes

- Status: Done
- Branch / PR: `m1/p22-stock-nodes` / [#35](https://github.com/lucaswewa/OScope/pull/35)
- Size: M · estimated ~3,200 before it started (fifteen stock nodes over P08–P08c's functions, a reference reader and a resampler ~1,000; live recipes in the preview chain and the `preview` Thing, with references captured from the camera ~450; tests ~700; `ex-live-recipe` with its page ~650; glue ~50; and a tenth on top); one pull request, at the owner's standing choice for P12–P23 · measured **3,316** hand-written lines added (code 1,913; tests 801; examples 596; glue 6), and 25 removed. Docs, `Cargo.lock` and the generated snapshot aren't counted. **That is 2,716 over M's 600, and about 4% over the estimate.**

## Summary

The nodes the live view and every acquisition need, and a live recipe on the preview ([spec](../phases.md#p22), plan §5.7).

- **Fifteen stock nodes** in `oscope-pipeline`:
  - correction: demosaicing, the black level, dark frames, flat fields, white balance, the colour matrix;
  - display: levels and gamma, lookup tables, false colour;
  - geometry: binning, resizing, cropping;
  - measurement and formats: histograms with statistics, format conversion;
  - references: `io.reference`, which reads a reference image, such as a flat field.

  Each runs P08–P08c's function where there is one, and its CUDA twin on the GPU.
- **Live recipes:** the `preview` Thing runs a recipe on every preview, in place of its look. `set_live_recipe` checks it against its latency budget first. `capture_reference` makes the flat fields and dark frames that recipes read.
- **`ex-live-recipe`:** the live view with a choice of recipes, among them flat field plus false colour, and buttons that capture a flat field and a dark frame.

## What was built

| Where | What |
|---|---|
| `oscope-pipeline` `stock/` | `correction.demosaic`, `correction.black_level`, `correction.dark_frame`, `correction.flat_field`, `correction.white_balance`, `correction.colour_matrix`, `display.levels`, `display.lut`, `display.false_colour`, `resize.bin`, `resize.resize`, `geometry.crop`, `measure.histogram`, `convert.format`, `io.reference`; `stock::registry(references)`; `Region`; the resampler |
| `oscope-pipeline` | `try_node`, for parameters a schema can't check alone; `Step::key`, for what a step's outputs depend on beyond its parameters, such as a file's content; `Plan::outputs`; `Plan`'s `Debug` |
| `oscope-compute` | `Cuda::copy`: an image copied on the GPU, for the colour kernels that work in place |
| `oscope-acquire` `preview` | `LiveRecipe` (checked, run, timed); `Preview::set_recipe`, `recipe`, `compute` and `linear`; the recipe in the chain, on the CPU or after the GPU's steps; `Rendered::recipe`; `PreviewError::Recipe` |
| `oscope-api` `preview` | `live_recipe` (saved, read-only to clients), `recipe_budget_ms` (saved, 20 ms), `nodes`, `references`; `set_live_recipe` and `capture_reference`; `recipe_ms` in each session's statistics. `Roots` is now a server service |
| `examples/ex-live-recipe` | The server, the page with its seven recipes, `--smoke` |

## How to try it

```sh
cargo run --release -p ex-live-recipe
```

Open `http://127.0.0.1:8218/`:

- **Choose a recipe:** the live view changes, and its time shows beside the rate.
- **Capture a flat field,** then choose *Flat field, then false colour*. Zoom in: the flat is sampled over the region shown.
- **Set the budget to 1 ms,** and choose a recipe: refused, with its time.

The [README](../../../examples/ex-live-recipe/README.md) has the recipes, the same with `curl`, and the times.

## Performance

**The budget.** Plan §5.20 gives the preview 60 ms on the CPU, for a 20 MP frame shown at 1920 px. P17 measured the chain at 34.2 ms with the look. A live recipe replaces the look's last steps, so a default budget of 20 ms keeps the chain within §5.20's.

**The recipes, checked** (release, the owner's PC, 24 threads, the live pool's 6), each at a 1920 × 1080 RGB image: the middle of three runs, after one to warm up.

| Recipe | Time |
|---|---|
| Levels and gamma | 4.0 ms |
| False colour | 5.0 ms |
| Inverted (levels, then a table) | 8.5 ms |
| The green channel | 2.8 ms |
| Flat field | 7.5 ms |
| Flat field, then false colour | 8.7 ms |
| Dark frame and flat field | 11.7 ms |

The first measurement put *Dark frame and flat field* at 29.7 ms, over its budget. Two changes brought it down:

- **Copies:** each step's single-threaded copy of a 25 MB image became parallel (21.8 ms).
- **Sampling kept:** the dark and flat steps now keep their last sampled reference while the reference, the region and the size stay the same, as they do frame after frame (11.7 ms).

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P12–P23. It came in about 4% over the estimate, and more than five times M's budget.
- **A live recipe replaces the look:** it gives display values, from 0 to 1. While one is set, the `look` setting's levels, gamma and gains aren't applied. A recipe that wants them uses `display.levels` and `correction.white_balance`.
- **References are a stand-in for P33's calibration.** `capture_reference` takes the mean of the camera's next frames, in linear light, at most 1024 px wide. It keeps them as 16-bit TIFFs in `{data}/references/`. `io.reference` reads them by name, and refuses names that aren't plain names, so a recipe can't read other files. P33 will keep references for each objective and light, versioned.
- **The budget is checked once, when a recipe is set:** at the default viewport's size, on a synthetic image. Previews aren't refused afterwards; each session's statistics carry the recipe's time on its latest preview. In an unoptimised build the 20 ms default is too tight: the example's README says to use `--release`, and its smoke run sets 200 ms.
- **Where stock nodes run on the GPU:** only the five whose functions P07–P08c gave CUDA twins (demosaicing, the black level, white balance, the colour matrix, binning). P08c's display transform gives 8 bits on the GPU, not display values, so `display.levels` runs on the CPU. More twins come with P23's filters, or later.
- **Node IDs are `category.name`:** `correction.*`, `display.*`, `resize.*`, `geometry.*`, `measure.*`, `convert.*` and `io.*`.
- **P21's pull request, #34,** was squash-merged as `37e1d69`, but GitHub answered 502 during the merge and left it open. It was closed by hand, with a comment naming the commit. It shows as closed, not merged.

**Other choices:**

- **`display.levels` reads a table** of the 65,536 16-bit levels, as `to_display` does. So at 8 bits it gives `to_display`'s output exactly, and it needs no power function for each sample.
- **References are sampled to the image's size** over the region shown: the region's input `{x, y, width, height, frame_width, frame_height}`, in the frame's pixels. Shrinking averages by overlap; enlarging interpolates linearly, as references are smooth and usually smaller than the frame.
- **The flat field's mean is the whole flat's,** not the region's, so zooming in keeps the level.
- **A recipe that changes the image's size is refused** for live use: the preview's ROI, and what the client draws, assume the same size.
- **`Cuda::copy`** was added to `oscope-compute`, as P08c's colour kernels work in place, and a step must leave its input as it is.
- **The preview's engine never memoises:** live frames are never the same twice.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-pipeline` | `stock` (5) | **Against P08–P08c:** demosaicing by each method and Bayer pattern, on an odd-sized chart; the black level; white balance; the colour matrix; levels with sRGB's curve and with gamma, to the 8 bits of `to_display`; binning against `downsample`; cropping against `crop`; resizing by a whole factor against `downsample`. **New nodes:** enlarging between pixel centres; lookup tables; false colour at the map's stops; Rec. 709's luminance, grey to RGB, a channel, a bit depth; a histogram and its statistics and percentiles. **References:** a flat field evening out a fall-off; a quarter-size flat sampled over a zoomed region; one plan run over two regions in turn, as fresh plans give; a dark frame at full and half size; references read, memoised, and not mistaken when their file changes; bad names and missing files refused; levels refused when white isn't above black |
| `oscope-pipeline` | `gpu_steps_match_the_cpu_s` (with `OSCOPE_CUDA=1` and `cuda`) | The five GPU nodes give what their CPU steps do, within 1e-5 |
| `oscope-api` | `a_live_recipe_runs_on_each_preview_once_checked` | Over a real socket: an inverting recipe set, checked, and its previews inverted, with its time in the session's statistics. Refused, saying why: a recipe that changes the size, one reading a reference that isn't there, one over its budget. A flat field captured from the camera, then a recipe reading it over a zoomed region. None clears it |
| `oscope-api` | `devices` | The `preview` Thing's description, snapshotted again |
| `ex-live-recipe` | `--smoke` | References captured; each of the seven recipes set and run on the previews; one over its budget refused; none clears it |

**Where they ran:**

- the `preview` and `stock` suites: 5 times out of 5 with their binaries pinned to two cores; and on Linux, under WSL, with `oscope-acquire`'s;
- the CUDA test: on the owner's GPU, with `OSCOPE_CUDA=1`.

The first two-core runs failed the live recipe test. In an unoptimised build on two cores, the inverting recipe took 2.7 s at the default 1920 × 1080, over even the test's raised budget, and its load starved another test once. The test now times recipes at the stream's 320 × 240, as the example's smoke run does.

**Mutation checks** (each broke the code on purpose, then the test named was run): all nine caught.

| Broken on purpose | Caught by |
|---|---|
| Demosaicing ignores the pattern | `correction_nodes_give_what_p08_s_functions_do` |
| Levels ignore the gamma | `display_and_geometry_nodes_give_what_p08b_s_functions_do` |
| The flat field multiplies | `dark_frames_and_flat_fields_correct_the_part_of_the_frame_shown` |
| The region ignored | `dark_frames_and_flat_fields_correct_the_part_of_the_frame_shown` |
| The kept sampling reused for another region | `dark_frames_and_flat_fields_correct_the_part_of_the_frame_shown` |
| A reference's content left out of its memo key | `references_are_read_from_their_folder_and_keyed_by_content` |
| The preview doesn't run the recipe | `a_live_recipe_runs_on_each_preview_once_checked` |
| A recipe's size not checked | `a_live_recipe_runs_on_each_preview_once_checked` |
| The budget not checked | `a_live_recipe_runs_on_each_preview_once_checked` |

## Follow-ups and known gaps

- **CUDA twins** for the pixelwise stock nodes, and a display transform that gives display values on the GPU.
- **References by objective and light,** versioned: P33's calibration.
- **A flat field is taken as it was captured:** subtracting the dark frame from the flat first, as P33's calibration will, is left to it.
- **Budgets at run time:** a recipe that keeps going over its budget on real frames is shown, not dropped.
- **Recipes beyond live:** a job kind for batch recipes, and M2's editor.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The colour steps** are P08–P08c's own.
- **Grey from RGB** uses ITU-R BT.709's luminance weights, on linear light.
- **The flat-field correction** is the textbook one: each pixel divided by the flat, times the flat's mean.
- **Resampling** averages by overlap to shrink, and interpolates linearly to enlarge.
- **The colour maps' stops** are our own.
