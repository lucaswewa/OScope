# ext-python-workflow

A sample Python extension for OScope (P36, [ADR-0025](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0025-scan-planning-and-the-scan-engine.md)): the scan workflow `example.spiral`. It plans tiles in a square spiral out from a centre, a ring at a time, spaced by the field's steps. A scan then starts where the user is looking and grows from there.

- **`schema()`** gives its parameters: the centre's `x_um` and `y_um`, and how many `rings` of tiles go about the centre's, from 0 to 50.
- **`plan({"params", "field"})`** gives the tiles in order, each `{"x_um", "y_um", "name"}`. Ring r has 8r tiles, so r rings make (2r + 1)² in all.

## How to try it

The examples' servers don't load extensions, so [`ex-scan`](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/examples/ex-scan/README.md) doesn't use it. With uv installed, the API's test `a_python_workflow_plans_a_spiral_that_the_engine_runs` packs it, installs it, and plans and runs a ring of 9 tiles: `OSCOPE_PYTHON=1 cargo test -p oscope-api --test scan`. By hand, pack it, then install the package into a server's extensions:

```sh
cargo run -p oscope-server -- ext pack examples/ext-python-workflow
```

Then, through the `scan` Thing, `patterns` lists `example.spiral` with its schema, and `plan` or `run` with `{"pattern": "example.spiral", "params": {"x_um": 35350, "y_um": 14060, "rings": 2}}` plans 25 tiles about the first island of the simulated H&E slide.
