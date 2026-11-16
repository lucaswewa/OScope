# ext-python-report

A sample Python extension for OScope (P47, [ADR-0030](../../docs/adr/0030-reports.md)): the report generator `example.fields`. It writes up a camera's field of view and pixel size at the sample under each objective.

- **`schema()`** gives its parameters: the `sensor`'s pixels across and down and their `pitch_um`, and the objectives' `magnifications`, from 1 to 12 of them.
- **`generate(params)`** gives the report as OScope's model, with one section of four blocks:
  - a line of text on the sensor;
  - a table of each objective's field in mm and pixel in µm;
  - a bar chart of the fields' widths;
  - a sketch of the fields about one centre, to scale, as a PNG it draws itself, in base64.

  The date is left empty, for the job to fill in.
- **What it refuses,** a missing sensor or a magnification of 0, fails the job, saying why.

## How to try it

The examples' servers don't load extensions, so [`ex-report`](../ex-report/README.md) doesn't use it. With uv installed, the API's test `a_python_generator_s_report_is_made_and_served` packs it, installs it, and makes a report of the MIChrome 20's sensor under three objectives: `OSCOPE_PYTHON=1 cargo test -p oscope-api --test reports`. By hand, pack it, then install the package into a server's extensions:

```sh
cargo run -p oscope-server -- ext pack examples/ext-python-report
```

Then submit the job `report.make` to make a report into a project:

```sh
curl -X POST http://127.0.0.1:8210/api/v1/jobs/submit -H 'content-type: application/json' \
  -d '{"kind": "report.make", "params": {"project": "<project>", "options": {"generator": "example.fields",
       "params": {"sensor": {"width": 5472, "height": 3648, "pitch_um": 2.4}, "magnifications": [5, 10, 20, 50]}}}}'
```

At 20×, that sensor sees 0.657 × 0.438 mm, in pixels of 0.120 µm.
