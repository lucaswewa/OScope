# P47: Reports, and report extensions

- Status: Done
- Branch / PR: `m1/p47-reports` / [#63](https://github.com/lucaswewa/OScope/pull/63)
- Size: L · estimated ~2,600 before it started (the report crate with its model, Typst and minijinja renderers, charts and templates; the report generator kind in both SDKs and the loader; the job and OScope's generator; the `projects` Thing's templates; tests; `ex-report`); one pull request, at the owner's standing choice · measured **2,994** hand-written lines added (code 1,513: `oscope-report` 836, `oscope-workflows` 495, `oscope-api` 94, `oscope-ext` 55, `oscope-ext-sdk` 26, the Python SDK 7; tests 959; examples 426, the Python generator among them; glue 96), and 10 removed. Roboto's licence, vendored, isn't counted. **That is 2,094 over L's 900, and 15 % over the estimate:** the estimate left out the sample Python generator and its test through the API, the regions read through deep zoom, and the data route's types

## Summary

Shareable reports, as PDF and HTML, with their provenance ([spec](../phases.md#p47); plan §5.17; [ADR-0030](../../adr/0030-reports.md)).

- **A report is data first:** a model of a title, metadata, sections of text, images, tables and charts, and provenance. A template renders it.
- **PDF by Typst, in process,** in a sandbox that holds the report's files and Roboto, and nothing else. **HTML by minijinja,** one file with its pictures inline and its charts as SVG.
- **Templates:** OScope's `standard`, and the user's, kept only once a sample report renders through them, and served by the `projects` Thing.
- **Generators:** OScope's own, `results`, writes up a project's results: pictures, summaries, a count's areas and rows, provenance. Extensions add theirs, as a new driver kind, `report`, in both SDKs.
- **The job `report.make`** makes a report of the project: `report.pdf`, `report.html` and `model.json`.
- **`ex-report`** reports on P46's count; **`ext-python-report`** is a sample generator in Python.

## What was built

- **`oscope-report`** (new, L3):
  - the model: `Report`, `Field`, `Section`, `Block`, `Chart` and `Images`;
  - `pdf`: Typst 0.15.1 through a `World` of its own, with a template's errors as their file, line and column;
  - `html`: minijinja 3.0.5, escaping on, images as data URIs;
  - `svg`: bars or a line to scale, the axis's top a round number;
  - `Templates`: `standard` built in, the user's in `<data>/templates/<name>/`, checked, listed, read and deleted;
  - `sample`: a report with every kind of block, which templates are checked against;
  - `fonts/`: Roboto 2.138, regular, bold and italic, with its Apache-2.0 licence.
- **The report kind:**
  - `oscope-ext-sdk`: `ReportGenerator` and `report(name, generator)`;
  - `oscope-ext`: `Reports`, from the drivers of kind `report` that a manifest lists under `reports`;
  - the Python SDK: the kind `report`, with `schema()` and `generate(params)`.
- **`oscope-workflows::report`:** `make`, `ReportOptions`, and the job factory `report_jobs` (`report.make`), with OScope's generator `results`. A result's region at full size is read through deep zoom's `StoredPyramid` (P38), so a raw mosaic comes in colour.
- **`oscope-api`:**
  - the `projects` Thing: `GET templates`, `GET template/{name}`, the actions `save_template` and `delete_template`, and the job registered with the extensions' generators;
  - the data route: `.pdf` as `application/pdf`, and `.html` as `text/html` with `Content-Security-Policy: sandbox`.
- **The layers** (ADR-0005's rule 4): `oscope-report` in L3, and the edge `oscope-ext` → `oscope-report` within it, for `oscope-ext`'s test, which reads a Rust generator's report as the model. Plan §5.2 has the crate's row.
- **`deny.toml`:** Typst's licence exception and its eight advisories, each with its reason (ADR-0030).
- **`.gitattributes`:** fonts are binary.
- **`examples/ex-report`** (port 8233), its page and README; **`examples/ext-python-report`**, excluded from the workspace like the other Python samples.

## How to try it

```sh
cargo run --release -p ex-report
```

Then open http://127.0.0.1:8233/. It takes about 29 s at real time, and prints:

```text
ex-report: a snake over 1.00 × 0.75 mm at 20×, stitched and its nuclei counted; a report on its 2 results: a slide stitched; a slide stitched: cells
  report.pdf of 151 kB and report.html of 162 kB, through OScope's template; another through the example's own, "blue"
  open http://127.0.0.1:8233/ to read them
```

The page lists both reports and shows the HTML one. The PDF has two pages:
- **The mosaic:** its thumbnail, and its region of 369 × 230 µm in colour.
- **The count:** its summary, the bar chart of 4,442 nuclei's areas, the first ten rows, and the provenance.

The [README](../../../examples/ex-report/README.md) gives the job's options and what a template reads. [`ext-python-report`](../../../examples/ext-python-report/README.md) shows an extension's generator.

## Performance

§5.20 has no budget for reports. On this PC, with a release build:

- **The job:** the example's report on two results, as PDF, HTML and model, took 46 ms; the PDF alone through `blue`, 35 ms. Saving `blue`, which renders the sample through both files, came between them.
- **The size:** Typst's tree adds about 40 MB to a release binary. Release `ex-report` is 73 MB; `ex-count-cells`, built before P47, is 33 MB.

## Design notes and deviations from the plan

- **Generators write data, not documents.** Every template renders every generator's report. `model.json`, kept beside the files, is the report as rendered. The store's record file is `report.json`, hence the name.
- **The sandbox:**
  - **Files:** a template reads the report's own files and nothing else. No file of the disk's, no other source and no package is served, so nothing goes to the network.
  - **The date:** `datetime.today()` is the report's date, so a PDF depends on its model alone.
  - **Fonts:** Roboto, vendored, is the only one. `typst-assets`' fonts are OFL, GUST and Bitstream Vera, outside ADR-0002's list, and its `fonts` feature stays off.
- **HTML is escaped,** and the images and charts that OScope makes are marked safe. minijinja's HTML mode escapes `/` as `&#x2f;` too, which is why the data URIs need `safe`.
- **The standard HTML report is light,** whatever the reader's scheme: it is a document, and its charts' colours are fixed.
- **The data route sandboxes HTML.** A user's template can hold anything; served with `Content-Security-Policy: sandbox`, it runs no scripts and has no access to the app's origin. A PDF isn't sandboxed, as Chrome's viewer won't show one that is.
- **A result's pictures:** its thumbnail, and a region of 768 × 480 pixels at most from its middle plane's middle, at full size, as its deep zoom shows it. The spec's "rendered regions" are these. The stored thumbnail of a raw mosaic is grey, because the store's thumbnails don't demosaic; its region is in colour.
- **A count's section shows its input's thumbnail,** since a count has no image of its own.
- **A report made through a user's template is named for it:** `ex-report: results (blue)`.
- **The deny exceptions** are the owner's choice, made before the code (ADR-0030), and flagged in the pull request.
- **Typst's embedded data** in `typst-assets` comes with notices for P54's licence bundle: ICC profiles (CC0), and HTML and MathML tables (BSD-3-Clause, W3C document licence).
- **Beyond the spec:**
  - the sample Python generator and its test through the API, so that the report kind is tried from end to end;
  - the data route's PDF and HTML types.

### What the mutants' reading found

Thirty-nine mutants, listed in advance from the code, covering:

- the charts' top, heights, labels and line;
- the renderers:
  - the charts' files;
  - HTML's escaping;
  - errors' lines;
  - the date;
  - the data URIs' type and padding;
  - reading base64;
- the templates' checks, listing and names;
- the report kind in the SDK, the loader and the Python SDK;
- the job:
  - its choice of results;
  - its tables, bins and rows;
  - the date;
  - its cancels and formats;
  - its names and regions;
- the API's types, its sandbox, its errors, and the generators' wiring.

Reading the tests against the list first found gaps, now covered:

- a label to escape, and the line through the bars' tops;
- half a template's folder;
- the sandbox's date, and what it serves;
- a grey image's region, and a region's size in µm;
- an extension's PNG, whole through base64;
- the largest area on the last bin's edge, and a null in a summary;
- a cancel at the first chance;
- where templates are kept.

The run then missed one, now caught. With the cancel before rendering dropped, the cancel after the PDF still stopped the job, with nothing kept. The test now checks that a cancel at the first chance is the last progress the job reports.

So **all 39 are caught.** Three ran with `OSCOPE_PYTHON=1`, through the API's test of the Python generator:

- report drivers not registered;
- `generate` calling `schema`;
- the extensions' generators not wired to the job.

One ran through the Python SDK's tests.

### The page

The page wasn't seen in a browser here. Under Node, against the running example, with a stand-in DOM, it:

- listed both reports, each with its name, time and files;
- linked `report.pdf` and `report.html`, which answered 200 as `application/pdf`, and as `text/html` with `sandbox`;
- showed the newest HTML report in its frame.

### Two cores

`scripts/two-core.sh`, five times each, side by side, on two CPUs each, with `OSCOPE_PYTHON=1`:

| Target | Passed | Time |
|---|---|---|
| `oscope-report:report` | 5 of 5 | 1.6 s |
| `oscope-ext:reports` | 5 of 5 | 0.2–0.9 s |
| `oscope-workflows:report` | 5 of 5 | 1.1–1.6 s |
| `oscope-api:projects` | 5 of 5 | 0.7–1.8 s |
| `oscope-api:capture` | 5 of 5 | 1.7–2.4 s |
| `oscope-api:reports` | 5 of 5 | 2.5–3.9 s |
| `xtask:layering` | 5 of 5 | 0.4–0.5 s |
| `ex-report --smoke` | 5 of 5 | 21.0–24.3 s |

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-report` `report` (new) | 6 | The sample as PDF: one page, its text, `1 / 1`; a long table over three pages or more, numbered. Its HTML as a snapshot, its image and chart inline, its text escaped. Template errors with their file, line and column: a field that isn't there, an unclosed bracket, a missing image, minijinja's syntax. The sandbox: today is the report's date, and a template reads its own files alone, not the disk's, another source or a package. Charts to scale: the axis to 5, bars 1 : 4 : 2, labels escaped, the line through the bars' tops. Templates kept only once they render; half a template not listed; names refused; `standard` kept; deletion |
| `oscope-ext` `reports` (new) | 1 | A Rust generator through the C ABI: described as `report`, its schema, its report read as the model, a refusal with its reason |
| `oscope-workflows` `report` (new) | 2 | OScope's generator on a mosaic and its count, as PDF, HTML and model. The mosaic's thumbnail and its region in µm. The count's input's picture, its summary (lists joined, null as none), its areas' chart (the largest on the last bin's edge) and first 10 rows. Provenance. One result chosen, HTML alone; a template of the user's, naming the report; a grey image's region. An extension's report, its PNG whole through base64, the date filled in. Refused: the generator's refusal, text that isn't base64, a generator without extensions, a template that isn't, no results. Cancels at the first chance and later keep nothing |
| `oscope-api` `projects` (one new) | 4 | Templates over HTTP: listed, read, a broken one refused with its line, kept in `<data>/templates/`, deleted, `standard` kept. A report's PDF and HTML through the data route, the page sandboxed. The Thing's description |
| `oscope-api` `reports` (new) | 1 | With `OSCOPE_PYTHON=1`: `ext-python-report` packed, installed and run by `report.make`; its HTML holds the fields, the chart and its PNG, and its PDF is one. A refusal fails the job, with its reason |
| Python `test_drivers` (one new) | 13 | A Python generator's report, through the worker |
| `xtask` `layering` | 7 | `oscope-report` in L3, with `oscope-ext`'s edge |
| `ex-report --smoke` | — | The PDF's type, the sections, the HTML's count, chart, picture and summary, the second template's PDF and the reports' names, the templates, the page |

**The local replays,** the merge gate (ADR-0038), each run alone on `9d4db41`. The commit since adds only these lines and the measurements above.

- **Windows:** `scripts/ci-local.sh`, in 13 minutes: 63 of 63 steps, 588 tests in 221 suites, the Python tests real (the new generator's among them), and every example's `--smoke`. No CUDA path changed, so the CUDA tests didn't run.
- **Linux:** `scripts/ci-clone.sh` on a fresh WSL clone, in 13 minutes: 63 of 63 steps, 588 tests in 221 suites; 820 files, none with a CR.

## Follow-ups and known gaps

- **Colour thumbnails of raw images:** the store's thumbnails could come from a raw image's 2 × 2 cells, as its deep zoom's overview does.
- **A count's outlines drawn on its region,** from its annotation layer.
- **A template's compile can't be stopped part-way;** a template that loops runs until Typst's own limits stop it. Templates are the user's, and P49's accounts and the control lease will guard who saves them.
- **Typst's ignored advisories,** to look at again with each Typst upgrade.
- **Editing templates in the app** is M2's.

## Clean room

Nothing was copied from `microscope` or any other codebase. Typst and minijinja are used through their documented interfaces. The rest is written from published descriptions:

- **The axis's round top,** 1, 2 or 5 times a power of ten, follows Heckbert's "nice numbers" (Graphics Gems, 1990).
- **The base64 coder** follows RFC 4648.
- **The sample generator's PNG writer** follows the PNG specification (W3C, second edition; ISO/IEC 15948).

Roboto is vendored unchanged, with its licence.
