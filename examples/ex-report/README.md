# ex-report

P47's reports on P46's results, through the API as a client would.

1. **The results.** As in [`ex-count-cells`](../ex-count-cells/README.md): a snake over 1 × 0.75 mm across the edge of the simulated H&E slide's first island is scanned at 20× and stitched, and its nuclei are counted.
2. **A report on them,** by the job `report.make`, through OScope's template `standard`:
   - **a section per result:**
     - the mosaic's thumbnail, and a region of it at full size, as its deep zoom shows it (P38), the raw mosaic in colour;
     - the count's summary as a table;
     - its nuclei's areas as a bar chart, drawn as SVG;
     - its first 10 rows;
   - **the provenance:** each result's software and nodes;
   - **three files:** `report.pdf` (Typst, compiled in process), `report.html` (minijinja, self-contained: its pictures inline, its charts as SVG), and `model.json`, the report as data.
3. **A template of the example's own,** `blue`: the standard one with its title in blue.
   - The action `projects/save_template` keeps it, once a sample report renders through both of its files.
   - A second report goes through it, as PDF alone. A report through a template of the user's is named for it: `ex-report: results (blue)`.
4. **The page** lists the reports with their files, and shows the newest HTML report.

## How to try it

```sh
cargo run --release -p ex-report
```

It serves at http://127.0.0.1:8233/. On the owner's PC, at real time on the simulator's quarter sensor, it took 29 s and printed:

```text
ex-report: a snake over 1.00 × 0.75 mm at 20×, stitched and its nuclei counted; a report on its 2 results: a slide stitched; a slide stitched: cells
  report.pdf of 151 kB and report.html of 162 kB, through OScope's template; another through the example's own, "blue"
  open http://127.0.0.1:8233/ to read them
```

- **The files:** `/api/v1/data/<project>/reports/<report>/report.pdf`, `report.html` and `model.json`.
  - A PDF is served as `application/pdf`.
  - An HTML file is served with `Content-Security-Policy: sandbox`, so whatever its template holds runs no scripts and has an origin of its own.
- **A raw mosaic's thumbnail is grey:** the store's thumbnails don't demosaic. Its region beside it is in colour.

**The job,** on any project:

```sh
curl -X POST http://127.0.0.1:8233/api/v1/jobs/submit -H 'content-type: application/json' \
  -d '{"kind": "report.make", "params": {"project": "<project>", "options": {"formats": ["pdf", "html"]}}}'
```

- `generator` is `results`, OScope's own, or an extension's report generator by name.
- `params` are the generator's. For `results`, `{"results": ["<id>", …]}` chooses the results, all of them otherwise.
- `template` is `standard` or one of the user's; `date` is today's unless given.

**The templates:**
- **Listed and read:** `GET /api/v1/projects/templates` gives their names, and `GET /api/v1/projects/template/<name>` gives one's Typst and HTML.
- **Kept and removed:** the actions `save_template` and `delete_template` keep and remove them, in `<data>/templates/<name>/`.
- **What a template reads:**
  - **Typst:** the model at `json("/report.json")`, images at `/images/<name>` and charts at `/charts/<file>`. It is set in Roboto, embedded, and reads no other file or font.
  - **HTML:** `report`, `images[name]` (data URIs) and `charts[file]` (SVG).

A report generator of an extension's own: see [`ext-python-report`](../ext-python-report/README.md).

## Its checks

`cargo run -p ex-report -- --smoke` scans 0.6 × 0.45 mm at ten times real time, then checks that:

- the PDF is served as `application/pdf`, and is one;
- the model has a section for each result: the mosaic and its count;
- the HTML holds the count's section, a chart, a picture and the summary's density;
- the report through `blue` is another PDF, and the reports are named for their templates;
- the templates are `standard` and `blue`;
- the page and its script are served.
