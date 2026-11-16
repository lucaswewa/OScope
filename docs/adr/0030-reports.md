# ADR-0030: Reports

- Status: Accepted
- Date: 2026-11-16
- Phase: P47

## Context

- **Plan §5.17 wants reports:** templates in Typst giving PDF, HTML through minijinja, and a report generator interface that extensions can implement. Results carry their provenance, and a report should show it.
- **A report is shared.**
  - Its PDF should print the same anywhere.
  - Its HTML should open as one file, with no server behind it.
- **Templates are the user's to change.**
  - The M2 app will edit them.
  - A broken template should be caught when it is saved, saying where it broke, not when a report is due.
  - A template is code: Typst is a language, and HTML can carry scripts.
- **Typst is a Rust library,** and compiles in process: no program to ship or start, and no network. Its tree is large, though, and some of it falls outside ADR-0002's rules:
  - one licence: `ar_archive_writer`, a build-time crate through `psm` and `stacker`, is Apache-2.0 with LLVM's exception;
  - eight RustSec advisories:
    - six for unmaintained crates: `bincode`, `paste`, `rustybuzz`, `ttf-parser`, `unsafe-libyaml` and `yaml-rust`;
    - two for `quick-xml` 0.38, through `citationberg`, the bibliography styles.
  - `typst-library` 0.15.1 has no features to leave them out: bibliographies (`hayagriva`), code highlighting (`syntect`) and `yaml()` (`serde_yaml`) are always built.

## Decision

**1. The model,** `oscope_report::Report`, which JSON gives in full (unknown fields refused):

- a title, an optional subtitle and a date;
- metadata, as fields of a name and a value;
- sections, each a heading and its blocks:
  - text;
  - an image by name, with a caption and a width;
  - a table: a caption, columns and rows;
  - a chart: bars, or a line, of labels and values with their axes' labels;
- provenance, as fields.

Images travel beside the model, a name to PNG or JPEG bytes. Charts are data, which OScope draws as SVG, to scale.

**2. PDF, by Typst in process.** The template compiles in a sandbox of its own:

- the model at `/report.json`, each chart given its `file`;
- the images at `/images/<name>`, and the charts at `/charts/<file>`;
- no other file, and no packages, so no network;
- Roboto's regular, bold and italic (Apache-2.0), embedded in the crate, as the only fonts;
- `datetime.today()` is the report's date, when that is written `YYYY-MM-DD`, so a PDF depends on its model alone.

**3. HTML, by minijinja,** with escaping on. The template gets `report`, `images` (a name to a data URI) and `charts` (a file to its SVG): one file that holds everything.

**4. A template's errors** say which file broke (`typst` or `html`), its line and column, and Typst's or minijinja's message.

**5. The templates:**

- **`standard`** is built in, and can't be replaced or removed.
- **The user's** are folders of `<data>/templates/<name>/`, each with `report.typ` and `report.html`.
  - A name is 1 to 64 of `a`–`z`, `0`–`9`, `-` and `_`.
  - A template is kept only once a sample report, with every kind of block, renders through both files.
- **On the `projects` Thing:** `GET templates`, `GET template/{name}`, and the actions `save_template` and `delete_template`.

**6. The generators.**

- **OScope's own, `results`,** writes a section for each of a project's results, or for those chosen. A section holds:
  - its thumbnail, and a region at full size as its deep zoom shows it, or else its input's thumbnail;
  - its `summary.json`, as a table;
  - for a count of cells, its areas as a chart, and its first 10 rows.

  Each result's provenance becomes a field.
- **An extension's** are of a new driver kind, `report`, in both SDKs:
  - `schema()` gives its parameters' JSON Schema;
  - `generate(params)` gives the model, with `images`: a name to its bytes in base64;
  - a manifest lists them under `[provides] reports`.

**7. The job, `report.make`,** a compute job.

- **Its parameters:** `project`, and `options`:
  - `generator`, `results` unless given;
  - `params`, the generator's;
  - `template`, `standard` unless given;
  - `formats`: `pdf`, `html` or both;
  - `date`.
- **What it makes:** a report record of the project, with `report.pdf`, `report.html` and `model.json`, the model as rendered. It is named by its title, with the template's name after it if the template is the user's.
- **A cancel keeps nothing.**

**8. Serving.** The data route serves `.pdf` as `application/pdf`. It serves `.html` as `text/html` with `Content-Security-Policy: sandbox`, so the page runs no scripts and has an origin of its own.

**9. The dependencies:** `typst`, `typst-layout` and `typst-pdf` 0.15.1, and `minijinja` 3.0.5 with `serde`. `deny.toml` takes Typst's tree as it is, which the owner chose on 2026-11-16:

- **The licence:** `Apache-2.0 WITH LLVM-exception` is allowed for `ar_archive_writer` alone. The exception only grants more than Apache-2.0.
- **The advisories:** the eight are ignored, each with its reason. The two of `quick-xml` are reachable only through bibliography styles, which reports don't use.

**10. The crate,** `oscope-report`, is in L3 Engines (ADR-0005), and depends on no OScope crate. `oscope-workflows` and `oscope-api` use it. `oscope-ext`'s test does too, through a new edge within L3.

## Consequences

- **Gains:**
  - PDFs need no program beside the server.
  - One model gives both forms, and keeps the report as data.
  - Extensions write data, not documents, and every template renders theirs.
  - A template is checked when it is saved.
  - Reports are whole files, to send as they are.
- **Costs:**
  - **The size:** Typst's tree adds about 40 MB to a release binary. Release `ex-report` is 73 MB; `ex-count-cells`, built before P47, is 33 MB.
  - **Build time** grows with the tree.
  - **The ignored advisories** need a look at each Typst upgrade, and so does the licence exception: `cargo deny` reports an ignore that no longer matches.
  - **Typst's embedded data,** in `typst-assets` without its fonts, which aren't turned on, comes with notices for P54's licence bundle to carry:
    - ICC profiles, CC0;
    - HTML and MathML tables, under BSD-3-Clause and the W3C's document licence.
- **To watch:**
  - **A template's compile can't be stopped:** a template that loops runs until Typst's own limits stop it. Templates are the user's own, saved through the `projects` Thing, which P49's accounts and the control lease will guard.
  - **HTML outside the server:** a report's HTML opened as a file runs whatever its template holds. `standard` holds no scripts.
  - **A raw mosaic's thumbnail is grey,** because the store's thumbnails don't demosaic. Its region is in colour.

## Alternatives considered

- **HTML to PDF,** through headless Chromium, wkhtmltopdf or WeasyPrint: a browser or another program to ship and run on both platforms, with the PDF only as good as that program.
- **Drawing the PDF ourselves,** with `printpdf` or `lopdf`: tables across pages, figures, captions and text layout would all be ours to write.
- **Typst's command-line program:** a binary per platform to ship and start, and the same tree.
- **Tera or Handlebars for HTML:** the plan names minijinja. It needs little beyond serde, and its Jinja syntax is widely known.
