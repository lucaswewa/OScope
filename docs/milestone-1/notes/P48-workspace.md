# P48: The `workspace` Thing: analyses on datasets, results, provenance

- Status: Done
- Branch / PR: `m1/p48-workspace` / [#64](https://github.com/lucaswewa/OScope/pull/64)
- Size: M · estimated ~2,200 before it started (in `oscope-store`, the inputs' hashes and deleting a result ~60; in `oscope-workflows`, the analysis job, its outputs, provenance, re-runs and comparison ~380; the `workspace` Thing with P44's annotation routes moved to it ~350; tests ~660; `ex-workspace` and `ex-checkpoint-f` ~550; and a tenth on top); one pull request, at the owner's standing choice · measured **2,842** hand-written lines added (code 1,030: `oscope-workflows` 611, `oscope-api` 364, `oscope-store` 55; tests 1,129; examples 636; glue 47), and 258 removed. The annotation routes and their test, moved from `projects`, count again where they are now, about 85 lines of code and 140 of tests. **That is 2,242 over M's 600, and 29 % over the estimate,** most of it tests: the mutants' reading added the raw mosaic, the stack, the mask's pixels and an extension's node

## Summary

The Workspace tab's backend: recipes run on a project's images as jobs, their results kept with complete provenance, made again, compared and deleted ([spec](../phases.md#p48); plan §5.4, D12, D25).

- **An analysis** is a recipe (ADR-0017) run on an acquisition's or a result's image, or a region of it in µm. It runs as a job, `workspace.run`: a batch run of one item, on an engine of the Workspace's own with the stock nodes and the extensions'.
- **Its result** keeps each output as a file, and the first image as its own.
- **Its provenance** holds:
  - the analysis;
  - each node's version, and its extension, `id@version`;
  - `sha256:` over what it read;
  - the software.
- **Made again,** a result's analysis runs again, its nodes' parameters changed or not. Two results' provenance are compared as JSON Pointers.
- **Results are deleted,** unless another was made from them.
- **Annotations,** kept and measured (P44), are the `workspace` Thing's now.
- **`ex-workspace`** runs the counting recipe on a stitched slide.
- **`ex-checkpoint-f`** is Checkpoint F: cells counted and DAB quantified on a stitched H-DAB slide, then a PDF report.

## What was built

- **`oscope-store`:**
  - `Provenance::hashes`, by input, left out of a document when empty;
  - `Store::delete_result`, its folder renamed out of the way first, refused while another result names it as an input;
  - `rebuild_index` deletes a record left half deleted, as it does a project.
- **`oscope-workflows::workspace`:**
  - `Analysis`, the recipe, its input, an optional `Roi` in µm, `tile` and `name`;
  - `analyse`, which runs one now;
  - `workspace_jobs`, the job, through the engine's `batch_job`;
  - `again`, a result's analysis with parameters changed;
  - `difference`, two provenances compared.

  P45's `Colour` reads grey as one channel for it.
- **`oscope-api`:**
  - the `workspace` Thing, served beside `projects`:
    - `GET results/{id}` and `GET difference/{id}/{a}/{b}`;
    - the actions `run`, `rerun` and `delete_result`;
    - `annotate`, `remove_annotation`, `GET annotations/…` and `GET measurements/…`, moved from `projects`;
  - its engine's nodes, and the extensions that provide them.
- **The examples:**
  - `ex-annotate` and `ex-count-cells` call the annotation routes where they now are;
  - `ex-workspace` (port 8216) and `ex-checkpoint-f` (port 8217), with their pages and READMEs. Every manual port had an example, so these share theirs with `ex-sim-3d` and `ex-sim-stage`.

## How to try it

```sh
cargo run --release -p ex-workspace
cargo run --release -p ex-checkpoint-f
```

`ex-workspace` serves at http://127.0.0.1:8216/, and took 27 s at real time:

```text
ex-workspace: a snake over 1.00 × 0.75 mm at 20×, stitched; the counting recipe on 200 × 150 µm of it: 153 nuclei, 5100 per mm² (P46's count of the slide: 4582)
  made again: the same table; with nuclei of 25 µm² at least: 96, the difference [{"path":"/recipe/workspace.run/recipe/nodes/measure/params/min_area","before":10,"after":25}]
  open http://127.0.0.1:8216/ to see the results and their provenance
```

`ex-checkpoint-f` serves at http://127.0.0.1:8217/, and took 28 s:

```text
ex-checkpoint-f: an H-DAB slide, a snake over 1.00 × 0.75 mm at 20×, stitched; 4450 nuclei, 4599 per mm² of tissue
  by their DAB: 2674 negative, 743 weak, 978 moderate, 55 strong: 39.9 % positive
  across the tissue: DAB positive 6.7 %, H-score 12
  a report of 5 sections: report.pdf of 564 kB and report.html of 683 kB
  open http://127.0.0.1:8217/ to read it
```

Its PDF has six pages:
- the H-DAB mosaic, and a region of it in colour;
- the count's summary, with each class's nuclei;
- the areas' chart, and the first rows with each nucleus's DAB and class;
- the three stain results, each with a region;
- the provenance.

The READMEs give the routes and the outputs' files.

## Performance

§5.20 has no budget for analyses. On this PC, with a release build, `ex-workspace`'s whole run took 27 s. Most of that is the scan at real time. Each analysis is a fraction of a second, over a region of 208 × 156 cells.

## Design notes and deviations from the plan

- **What the recipe reads:**
  - grey images as one channel, RGB as three;
  - a raw image's 2 × 2 cells in colour, as P45 and P46 read them;
  - in linear light from 0 to 1.

  The recipe takes one input, `image`. A larger set of inputs, several images or a reference, is for later.
- **The region is in µm** from the image's top-left corner, as annotations are, so the app can draw one and run on it. A run reads 25 megapixels at most, so larger images need a region.
- **The hash is over what the recipe read:** the region's samples as 32-bit floats, little-endian, so the same input gives the same hash whatever its file's chunks or compression.
- **Made again, the same:** on another engine, the outputs are byte-identical, and the provenance doesn't differ. The memo is the engine's to keep or not. The test uses fresh engines, so a re-run computes.
- **The comparison** is over the provenance's JSON:
  - lists whose items carry a distinct `id`, `node` or `name` are compared by it, so a recipe's nodes go by their IDs;
  - an empty object or list counts as none, as parameters left out mean their defaults;
  - paths are JSON Pointers (RFC 6901).
- **The job is the pipeline's batch job of one item,** its kind and parameters set to the Workspace's. Its lane, its cancel and its tiling come with it.
- **The Workspace has an engine of its own,** built as the preview's is, with its own compute pools. Sharing one `Compute` between Things is a follow-up: with CUDA, each opens a context.
- **Annotations moved from `projects` to `workspace`,** as the spec's "consolidated" asks. `projects` keeps the Gallery's affordances and reports.
- **Results made by the other jobs** (stitch, count, stains, depth, deconvolution) carry no hashes yet; only analyses do. Their provenance stays as it was.

### What the mutants' reading found

Thirty-eight mutants, listed in advance from the code, covering:

- deleting a result: what was made from it, a missing one, its row, a deletion cut short, the row removed;
- the job's checks: its input, its outputs' names, stacks, the size limit;
- the region: its rounding, the pixel size, a raw image's cells;
- the provenance: the hash, the nodes once, the extensions, the hashes kept;
- the outputs: CSV's quoting and nulls, the mask's pixels, shapes, the first image, the image's pixel size, the rows;
- tiles and a cancel;
- making again: changes, nulls, unknown nodes;
- the comparison: keys, escaping, which way, what changed, empty parameters;
- the Thing: the files listed, the changes passed, the pair compared, the extensions' versions and nodes.

Reading the tests against the list first found gaps, now covered:

- a refusal checked by its reason;
- a region whose edges lie between pixels;
- a second image, and a node used twice;
- a recipe that can't run in tiles, run so;
- CSV's quoting and nulls, by a unit test;
- a key to escape;
- a raw mosaic as the input;
- the mask's pixels, decoded;
- an image over the limit, never written, so it costs nothing;
- a stack, from a node of the test's own;
- an extension's node, from P46's Python stub, with `OSCOPE_PYTHON=1`.

The run then missed one, and the mutant was at fault. `false && …` bound tighter than the `||` after it, so only the first of the input's three checks was turned off. Turned off whole, the mutant is caught.

So **all 38 are caught.** Two ran with `OSCOPE_PYTHON=1`: the extensions' versions and their nodes.

### The pages

Neither page was seen in a browser here. Under Node, against the running examples, with a stand-in DOM:

- **`ex-workspace`'s page** listed the five results, each with:
  - its nodes, its input's hash and its files;
  - against the first analysis, "the same" for the re-run, and "measure.params.min_area: 10 → 25" for the changed one.

  Every link answered 200.
- **`ex-checkpoint-f`'s page** listed the report, linked its PDF and HTML, and showed the HTML, sandboxed.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each, with `OSCOPE_PYTHON=1`. P45's and P46's suites are among them, for the shared reader, and so are the examples whose routes moved:

| Target | Passed | Time |
|---|---|---|
| `oscope-store:projects` | 5 of 5 | 9.3–9.6 s |
| `oscope-workflows:workspace` | 5 of 5 | 3.2–5.9 s |
| `oscope-workflows:lib` | 5 of 5 | 0.3–1.3 s |
| `oscope-workflows:stain` | 5 of 5 | 6.6–6.8 s |
| `oscope-workflows:cells` | 5 of 5 | 10.4–10.6 s |
| `oscope-api:workspace` | 5 of 5 | 2.3–6.1 s |
| `oscope-api:projects` | 5 of 5 | 2.4–4.5 s |
| `xtask:layering` | 5 of 5 | 0.3–0.5 s |
| `ex-workspace --smoke` | 5 of 5 | 26.0–27.9 s |
| `ex-checkpoint-f --smoke` | 5 of 5 | 36.1–42.6 s |
| `ex-annotate --smoke` | 5 of 5 | 19.2–23.7 s |
| `ex-count-cells --smoke` | 5 of 5 | 24.4–31.2 s |
| `ex-projects --smoke` | 5 of 5 | 3.1–6.3 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-store` `projects` (one new) | 9 | A result deleted only once nothing was made from it; its row and folder gone; one not there; a deletion cut short, gone at the next rebuild |
| `oscope-workflows` `workspace` (new) | 4 | **The counting recipe:** twelve nuclei; the mask white where they are, exactly; the first image the result's own, the second its own store; the summary. **The provenance:** each node once with its version, an extension named, the inputs, the software, and the hash, checked against one made apart. **Again:** the same, byte for byte, with no difference; a parameter changed is the one difference; back to its default. **Paths escaped.** **Other inputs:** a region whose edges lie between pixels; the raw mosaic, whole and in a region, its cells the slide's pixels; a grey picture; tiles as whole. **Refused**, saying why: a recipe without `image`; an output named as the result's files; a stack; a recipe that can't run in tiles; regions outside, empty or not a number; an image over 25 megapixels; a result that isn't; one not made here; a node that isn't. **A cancel** keeps nothing |
| `oscope-workflows` lib (one new) | 1 | Tables as RFC 4180 has them: quoted, doubled quotes, null empty |
| `oscope-api` `workspace` (new) | 4 | **Over HTTP:** a run, its result listed with its files and provenance, the mask through the data route, made again with a parameter changed and compared; refusals; deletions in order. **Annotations** (moved from `projects`). **The Thing's description.** With `OSCOPE_PYTHON=1`, P46's Python stub in a recipe, named with its extension's version |
| `oscope-api` `projects` | 3 | Its description, without the annotations |
| `ex-workspace --smoke` | — | The recipe's count, within 25 % of P46's density; again the same; the changed parameter its difference; three analyses hashed; the page |
| `ex-checkpoint-f --smoke` | — | Over 50 nuclei, 20–60 % of them DAB-positive; tissue DAB and H-score above 0; the PDF, the HTML's count and DAB, three sections or more; the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `2593890`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 13 minutes: 65 of 65 steps, 597 tests in 225 suites, the Python tests real (the extension's node among them), and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 13 minutes from cold: 65 of 65 steps, 597 tests in 225 suites; 836 files, none with a CR.
  - Its first run stopped two minutes in, after clippy: C: had filled, and WSL wouldn't start.
  - With the owner's word, `cargo clean --profile dev` freed 301.6 GiB, and WSL's debug build went.
  - It then ran again, alone, and passed.

## Follow-ups and known gaps

- **Hashes for every job's results,** not only analyses.
- **Recipes kept by name,** and the `pipeline` Thing's catalogue and recipes of plan §5.4: the Workspace takes recipes whole, and the node catalogue is still the `preview` Thing's.
- **More than one input:** several images, or a reference, in one analysis.
- **One `Compute` shared** by the preview and the Workspace.
- **A count's outlines on the report's region** (P47's follow-up) would show a recipe's objects too.

## Clean room

Nothing was copied from `microscope` or any other codebase. The comparison writes its paths as RFC 6901's JSON Pointers, and tables as RFC 4180's CSV. SHA-256 is the `sha2` crate's.
