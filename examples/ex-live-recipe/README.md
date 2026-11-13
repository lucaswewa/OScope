# ex-live-recipe

Live recipes (P22, [ADR-0017](../../docs/adr/0017-the-pipeline-model.md)): a recipe of stock nodes run on every preview, after demosaicing, binning and the black level, in place of the preview's own look.

A recipe is checked before it runs, by the `preview` Thing's `set_live_recipe`:

- every step can run live;
- it takes the image as `image`, and, if it asks, the part of the frame the image shows as `region`;
- it gives `image`, the same size, in display values;
- it takes at most `recipe_budget_ms` (20 ms unless set) on an image the default viewport's size.

Otherwise it is refused, saying why.

## How to try it

```sh
cargo run --release -p ex-live-recipe
```

It starts the simulated microscope on the scaled-down sensor, homes the stage and places it over the H&E slide, then prints `ex-live-recipe: open http://127.0.0.1:8218/`.

Port 8218 is `ex-live-view`'s too: the range for manual checks is used up. Run one at a time.

**Use `--release`.** A debug build is several times slower, and most recipes then go over the default 20 ms budget. Raise the budget on the page to try them anyway.

**The page:**

- **The live view,** each preview acknowledged once drawn. Its readout shows the rate, the latency, and the recipe's time on each preview.
- **Recipe:** the recipes in [`page/recipes/`](page/recipes/). Choosing one sets it, and shows the time it was checked at, or why it was refused. *None* gives the preview's own look back.
- **Budget:** `recipe_budget_ms`, for the next recipe set.
- **Capture a flat field:** takes the slide out of the simulated stage (`simulation/unload`), takes the mean of 8 frames of the empty field as the reference `flat`, then puts the slide back. On the instrument, you would move to empty glass instead.
- **Capture a dark frame:** turns the light off, takes the reference `dark`, and turns it back on.

**The recipes:**

| Recipe | Its steps |
|---|---|
| Levels and gamma | `display.levels`: 0.05 to 0.85, gamma 2.2 |
| False colour | `display.false_colour`: the fire map, from 0.3 to 0.95 |
| Inverted | `display.levels`, then `display.lut` with the table `[1, 0]` |
| The green channel | `convert.format` to green alone, then `display.levels` |
| Flat field | `io.reference` `flat`, then `correction.flat_field` over the region shown, then `display.levels` |
| Flat field, then false colour | The same, then `display.false_colour` with the spectrum map |
| Dark frame and flat field | `correction.dark_frame` with `dark`, then `correction.flat_field` with `flat`, then `display.levels` |

The last three need their references: capture them first, or they are refused, naming the one missing.

**Things to try:**

- **Zoom in** with a flat-field recipe on: the flat is sampled over the region shown, so the level holds.
- **The simulator lights the field evenly,** so a flat field changes little here. On the instrument it evens out the illumination's fall-off towards the corners.
- **A budget of 1 ms:** each recipe is refused, with its time.

**On the owner's PC** (release, 24 threads, the live pool's 6), the recipes were checked at a 1920 × 1080 image in:

| Recipe | Time |
|---|---|
| Levels and gamma | 4.0 ms |
| False colour | 5.0 ms |
| Inverted | 8.5 ms |
| The green channel | 2.8 ms |
| Flat field | 7.5 ms |
| Flat field, then false colour | 8.7 ms |
| Dark frame and flat field | 11.7 ms |

**With `curl`:**

```sh
curl -X POST -H "Content-Type: application/json" \
     -d "{\"recipe\": $(cat examples/ex-live-recipe/page/recipes/false-colour.json)}" \
     http://127.0.0.1:8218/api/v1/preview/set_live_recipe
# The invocation's output: the steps, and the time it was checked at.
curl http://127.0.0.1:8218/api/v1/preview/live_recipe
curl http://127.0.0.1:8218/api/v1/preview/nodes        # the stock nodes' descriptors
curl http://127.0.0.1:8218/api/v1/preview/references   # the references, by name
```

The references are in `.oscope/ex-live-recipe/data/references/`, as 16-bit TIFFs.

## `--smoke`

```sh
cargo run -p ex-live-recipe -- --smoke
```

With a temporary data root, a budget of 200 ms (as the smoke runs are built without optimising) and a 320 × 240 default viewport, it checks:

- that the page and its recipes are served;
- that a flat field and a dark frame are captured, and listed;
- that each recipe is checked, set, and runs on the previews;
- that one over its budget is refused;
- that *none* clears it.

It ends with `smoke: ok`, or says what failed and exits with code 1.
