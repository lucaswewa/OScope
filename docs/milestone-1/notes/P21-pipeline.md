# P21: Pipeline engine

- Status: Done
- Branch / PR: `m1/p21-pipeline` / [#34](https://github.com/lucaswewa/OScope/pull/34)
- Size: L · estimated ~3,100 before it started (values, nodes, the registry and the parameters' validator ~650; recipes and their checks ~330; the plan ~250; running it, with the memo cache ~400; tiles, live mode and the batch job ~420; trivial nodes ~250; tests ~800; `ex-pipeline` ~350); one pull request, at the owner's standing choice for P12–P23, rather than the spec's split of tiled execution into P21b · measured **5,106** hand-written lines added (code 3,421; tests 1,122; examples 524; glue 39), and 3 removed. Docs and `Cargo.lock` aren't counted. **That is 4,206 over L's 900, and about 2,000 over the estimate:** doc comments, blank lines and rustfmt's wrapping, which the estimate undercounted again, and more tests and a fuller example than planned.

## Summary

Processing is a recipe of typed nodes, checked, planned and run live or in batch ([spec](../phases.md#p21), D12, plan §5.7).

- **`oscope-pipeline`:**
  - **nodes:** a descriptor (ID, version, category, typed ports, parameters as a JSON Schema, capabilities), and the steps a node makes from its parameters;
  - **the registry,** and OScope's own validator for the part of JSON Schema that parameters use;
  - **recipes,** as versioned JSON, checked for types, cycles, missing inputs and parameters, every problem at once;
  - **plans:** the order, the placement on the CPU or the GPU with as few copies as it can, and the halo tiles need;
  - **the engine:** each step as soon as its inputs are ready, so branches run in parallel, with results memoised;
  - **four modes:** whole, tile by tile, live within a latency budget, and batch as a compute job;
  - **trivial nodes** for the tests and the example.
- **`ex-pipeline`:** a recipe run on a field of the simulated slide from the command line: its graph, each step's time, the memo cache, an edit, tiles and a batch.
- **ADR-0017** records it.
- **GitHub CI no longer runs the examples' `--smoke`,** as the owner asked, until the owner turns it back on.

## What was built

| Where | What |
|---|---|
| `oscope-pipeline` `value.rs` | `PortType`, `Value` (shared, immutable; images in memory or on the GPU), `Mask`, `Table`, `Values` |
| `oscope-pipeline` `node.rs` | `Descriptor`, `Port`, `Capabilities`, the `Node` and `Step` traits, `node()` for nodes with typed parameters, `StepCtx`, `Inputs`, `NodeError`, `Device` |
| `oscope-pipeline` `schema.rs` | `check` (a schema uses only known keywords; its `$ref`s resolve) and `validate` (problems as JSON pointers) |
| `oscope-pipeline` `registry.rs` | `Registry`, which refuses malformed descriptors |
| `oscope-pipeline` `recipe.rs` | `Recipe` (format 1), `NodeUse`, `Edge`, `RecipeOutput`, `Problem`, `RecipeError`, and `set_param` for editing |
| `oscope-pipeline` `plan.rs` | `Plan::new`: the checks, Kahn's order, only what the outputs need, placement, copies, halo, memo keys. Its `Display` is the graph |
| `oscope-pipeline` `run.rs`, `memo.rs` | `Engine`, `RunOptions`, `Run`, `Report`, `RunError`; the step scheduler on a rayon scope; copies between memory and the GPU; the memo cache |
| `oscope-pipeline` `tile.rs` | `tiles()` and `Engine::run_tiled` |
| `oscope-pipeline` `live.rs` | `Live`, `LiveStats`, `LiveResult` |
| `oscope-pipeline` `batch.rs` | `Batch` and `Engine::batch_job`, an `oscope-jobs` compute job |
| `oscope-pipeline` `test_nodes.rs` | `test.box_blur`, `test.add`, `test.threshold`, `test.statistics`, `test.bin2x2` (CPU and CUDA), `test.sleep` |
| `examples/ex-pipeline` | The CLI, two recipes, `--smoke` |
| `.github/workflows` | `RUN_EXAMPLES: "false"` in `ci.yml` and `cuda.yml`, and the examples' steps run only when it is `"true"` |

## How to try it

```sh
cargo run -p ex-pipeline
```

It prints the graph of [`small-dark-features.json`](../../../examples/ex-pipeline/recipes/small-dark-features.json), then runs it on a 1024 × 768 field of the H&E slide:

- **once,** with each step's time;
- **again,** every step `cached`;
- **after `mask.level=0.06`,** only `mask` running again;
- **in 12 tiles,** the same as the whole run to the bit;
- **as a batch job** over three more fields.

The mask, in `.oscope/ex-pipeline/data/out/mask.png`, is white on the nuclei. The [README](../../../examples/ex-pipeline/README.md) has more to try: other edits, smaller tiles, a refused parameter, the binning recipe on the GPU, and your own image.

## Performance

None of plan §5.20's budgets is P21's. For reference, `ex-pipeline` in a release build on the owner's PC (24 threads), on a 1024 × 768 RGB field:

| Run | Time |
|---|---|
| Run 1, five steps | 11.7 ms |
| Run 2, all from the memo cache, the input hashed | 0.6 ms |
| Run 3, one step run again | 0.9 ms |
| Tile by tile, 12 tiles of 256 px, each read with 12 px around it | 39.2 ms |

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** not the spec's split of tiled execution into P21b, as you decided for P12–P23. It came in about 2,000 lines over the estimate.
- **GitHub CI's examples are off, from this pull request on,** as you asked on 2026-11-13. There is one switch at the top of `ci.yml` and `cuda.yml`, `RUN_EXAMPLES: "false"`; setting it to `"true"` in a pull request turns them back on. Clippy and the tests still build every example in CI, and the local replays (`scripts/ci-local.sh`, `scripts/ci-clone.sh`) still run every `--smoke` before each pull request.
- **Parameters are checked by OScope's own validator, not the `jsonschema` crate.** That crate brings about 30 crates, `syn` 3 among them, each needing the licence and two-week checks. The validator knows the keywords that `schemars` generates and that parameters need, in about 480 lines with its tests. A schema using any other keyword, such as `pattern`, is refused when its node is registered, so nothing is ignored silently.
- **Steps on the GPU aren't memoised:** holding GPU memory in the cache would take it from the jobs' GPU budget (ADR-0016). An edit after a GPU step re-runs it, on the GPU.
- **Live mode still gives a result that took longer than its budget,** and counts it. It drops only frames that wait: one a newer frame replaces, or one already older than the budget when its turn comes. Dropping slow results would leave a recipe always over its budget showing nothing.
- **There is no server interface to recipes yet:** no Thing and no job kind. P22's live recipes and M2's editor will say what they need. Batch jobs are made in code, with `Engine::batch_job`.

**Other choices:**

- **Node versions are integers,** raised when what a node gives, or its parameters, change. A recipe may name the version it was made for, and must then match.
- **Nodes are pure,** so any step may be memoised. A node that writes files, such as a later exporter, will need a way to opt out.
- **Each step is prepared once a plan,** so live mode parses no parameters per frame.
- **The scheduler starts each step from the last step it waited for,** on a rayon scope in the run's pool. It never blocks a pool thread waiting, so it also runs inside the pool, as P22's preview chain will call it.
- **Steps' times are wall times.** While a step waits on its own threads, rayon may give them a parallel branch's work, so the times of parallel steps overlap. `ex-pipeline`'s README says so.
- **A step alone on the GPU stays on the CPU:** if everything it reads and everything reading it is on the CPU, running it on the GPU would cost two copies for one step. Without measured costs, that is the only exception to "on the GPU when it can".
- **Recipe inputs are hashed in parallel** for their memo keys, with the standard library's SipHash: about 0.6 ms for a 1024 × 768 RGB image.
- **Tiled runs run the whole plan on each tile,** read with the summed halo, rather than cutting each step's input to its own halo. It works for any tileable node, at the cost of computing some halo pixels more than once.
- **The trivial nodes are public,** in `oscope_pipeline::test_nodes`, as the example uses them too. `test.bin2x2` uses `oscope-compute`'s kernel and its CUDA twin, so the GPU path has a real node to run.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-pipeline` | `schema` (3, unit) | Types, ranges, enums, lengths, array and object shapes, as JSON pointers; `$ref`, `oneOf`, `anyOf` and `not`; unknown keywords and missing references refused; a `$ref` loop ends as a problem |
| `oscope-pipeline` | `pipeline` (11) | **Checks:** three recipes with 14 faults between them, each named with its place, among them a cycle in the direction data flows; GPU-only nodes without a GPU; a misspelt field. **The registry:** duplicates and four kinds of malformed descriptor refused. **Order and parallelism:** steps after what they read, only what the outputs need, and two branches that must meet at a barrier to finish. **Tiles:** a 1000 × 700 image in 12 tiles of 256 px, and a 120 × 90 one in 234 tiles of 7 px, smaller than the halo of 9, give exactly what whole runs do. **Memoisation:** a second run all cached; an edited parameter re-runs its step and those after; an earlier edit re-runs all after it; other pixels re-run everything; hits counted; off when asked. **Cancelling:** a run in a 60 s step stops in it, and starts nothing after. **Failures:** a step's error and panic, named; a missing input. **Batch:** three items as a compute job, tiled, with rising progress, results recorded, the same as whole runs; a cancelled batch stops in its item. **Live mode:** a frame past its budget given and counted; waiting frames replaced by the newest; a stale frame dropped; non-live steps refused. **Placement:** two GPU steps in a row cross twice; a lone GPU step stays on the CPU; an image read on both sides crosses once; a GPU plan refused by an engine without a GPU |
| `oscope-pipeline` | `gpu_steps_match_the_cpu_s` (with `OSCOPE_CUDA=1` and `cuda`) | Two binning steps on the GPU give what the CPU's do, within 1e-5, with two copies |
| `ex-pipeline` | `--smoke` | Run 2 cached; the edit re-runs only `mask`; 64 px tiles the same as the whole run; the batch job completes with a result for each field; the files written |

**Where they ran:**

- the `pipeline` suite: 5 times out of 5 with its binary pinned to two cores, as GitHub's runners have, in about 4.5 s each; and on Linux, under WSL;
- the CUDA test: on the owner's GPU, with `OSCOPE_CUDA=1`;
- `ex-pipeline`: with `--features cuda`, its binning recipe on the GPU.

**Mutation checks** (each broke the code on purpose, then the test named was run): all twelve caught.

| Broken on purpose | Caught by |
|---|---|
| Cycles not detected | `recipes_are_checked_and_every_problem_is_named` |
| Missing inputs not reported | `recipes_are_checked_and_every_problem_is_named` |
| Halos along a path taken as their greatest, not their sum | `tiled_runs_give_exactly_what_whole_runs_do`. The first version, with two blurs in a row, missed it: its chain is now three deep |
| Memo keys without the parameters | `results_are_memoised_and_only_what_changed_runs_again` |
| Memo keys without the inputs | `results_are_memoised_and_only_what_changed_runs_again` |
| A lone GPU step left on the GPU | `images_cross_to_the_gpu_and_back_as_seldom_as_they_can` |
| The first steps started one after the other | `steps_run_after_what_they_read_and_branches_at_once` |
| Live mode keeping the oldest waiting frame | `live_mode_drops_frames_it_can_t_run_in_time` |
| Live mode running stale frames | `live_mode_drops_frames_it_can_t_run_in_time` |
| `required` not checked | `values_are_checked_by_type_range_and_shape` |
| Steps not told of a cancel | `a_cancelled_run_stops_in_its_step_and_starts_no_more` |
| A batch's cancel not passed to its run | `a_cancelled_batch_stops_in_its_item` |

## Follow-ups and known gaps

- **The server's interface to recipes:** a job kind, and a Thing to list nodes and check recipes, with P22's live recipes and M2's editor.
- **Tiles from the store:** P28's store and P39's mosaics will read and write tiles, so a large slide never sits in memory.
- **Placement from measured costs,** and **memoising GPU outputs** within a budget of the cache's own.
- **More JSON Schema keywords,** such as `pattern`, when a node needs them.
- **An opt-out of memoisation** for nodes that aren't pure.
- **GitHub CI's examples,** off until the owner says.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The order** is Kahn's topological sort (A. B. Kahn, "Topological sorting of large networks", *Communications of the ACM* 5(11), 1962).
- **The validator** follows *JSON Schema Validation*, draft 2020-12 (json-schema.org), and JSON Pointer (RFC 6901), in our own code.
- **The memo keys** use SipHash (Aumasson and Bernstein, 2012), through Rust's standard library. The cache drops the least recently used entries first.
- **The test pictures** use Marsaglia's xorshift generator (*Journal of Statistical Software* 8(14), 2003).
