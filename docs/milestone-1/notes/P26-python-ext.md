# P26: Python extension runtime and SDK

- Status: In review
- Branch / PR: `m1/p26-python-ext` / [#40](https://github.com/lucaswewa/OScope/pull/40)
- Size: L · estimated ~3,000 before it started (the Python SDK, its node decorator, schemas from type hints, shared-memory views and message loop, with its pytest suite, ~650; finding uv and building each extension's environment at install, online or from a wheelhouse, ~250; the worker supervisor and the shared-memory transport, ~650; Rust tests ~450; the scikit-image sample and `ex-python-node` ~350; CI, scripts and the server's settings ~150; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,278** hand-written lines added (Rust code 971; the Python SDK 352; tests 621, of which Python 192; examples 295; CI and scripts 39), and 58 removed. **That is 1,378 over L's 900, and about a quarter under the estimate.**

## Summary

Third parties write nodes in Python, with their own dependencies, isolated from the server ([spec](../phases.md#p26); plan §5.8; D10; [ADR-0020](../../adr/0020-the-python-extension-runtime.md)).

- **The Python SDK,** `python/oscope-ext` (the `oscope_ext` package):
  - `@node`, with a parameter schema from type hints or given;
  - NumPy views of the host's shared-memory segments;
  - the worker's message loop;
  - its pytest suite.
- **`oscope-ext`:**
  - **uv:** found, and used to build each extension's environment when it is installed, online with packages at least two weeks old, or offline from a wheelhouse;
  - **the worker supervisor:** started on first use with a ping, kept warm, stopped on a time-out or when idle, started again after it dies;
  - **frames** in and out through shared memory the host owns;
  - **nodes** that run in the pipeline. Python runs only through `Extensions::open_with`.
- **`oscope-shm`:** `Segment`, named shared memory that another process maps by name.
- **`oscope-server`:** runs Python extensions, with a `[python]` table in its configuration.
- **The sample** `ext-python-node`, scikit-image's total-variation denoising, and **`ex-python-node`,** which installs it and runs it in a recipe, with timings.
- **CI:** `setup-uv` (uv 0.13.6) and the SDK's tests on Ubuntu, with `OSCOPE_PYTHON=1` for the Rust tests that need Python. The local replays run both on both platforms. **CLAUDE.md** lists `OSCOPE_PYTHON`.

## What was built

| Where | What |
|---|---|
| `python/oscope-ext` | `oscope_ext.node`, `nodes`, `get`, `schema_of`, `Node` (descriptor, run); `_shm.Segment`, the host's segments mapped with `mmap` as arrays; `oscope_ext.worker`, the loop; `pyproject.toml` (NumPy; pytest; `exclude-newer`; `cache-keys`, so that uv builds it again when its sources change), `uv.lock`, `.python-version` (3.13); 8 tests |
| `oscope-ext` `python.rs` | `PythonRuntime` (uv, the SDK, a wheelhouse, time-outs); `find_uv`; `prepare`: the environment, the packages, and the nodes described once; `OSCOPE_FOLDER` |
| `oscope-ext` `worker.rs` | The worker: spawn and ping, calls with a deadline, failures that stop it and failures that don't, the idle watch, its stderr logged and kept; the run's segments; the pipeline's node and step |
| `oscope-ext` `installed.rs` | `Extensions::open_with`; install makes a Python extension's environment and keeps its nodes, checked; a start reads them; `workers()` and `WorkerState`; the checks shared with Rust extensions |
| `oscope-ext` `package.rs` | `.oscope` refused in packages and folders to pack |
| `oscope-shm` `segment.rs` | `Segment`: `create`, `os_name`, `write_f32`, `read_f32` |
| `oscope-server` | `[python]` (`PythonConfig`), its folders relative to the file; the server opens extensions with Python |
| `examples/ext-python-node` | The sample, outside the Cargo workspace |
| `examples/ex-python-node` | The walk-through, with timings and the denoising measured |
| `examples/ex-omext` | Its dummy module declares its node, `hello.greet`, which a server running Python now checks |
| `.github/workflows/ci.yml`, `scripts/ci-local.sh` | uv; the SDK's tests; `OSCOPE_PYTHON` |

## How to try it

```sh
cargo run -p ex-python-node      # needs uv
cd python/oscope-ext && uv run --locked pytest
```

The [README](../../../examples/ex-python-node/README.md) says what `ex-python-node` prints, and how to use the sample in the server.

## Performance

No budget of plan §5.20 is P26's. The plan expects about 1 ms a call (§5.8). On the owner's PC (release; uv's cache warm; a 768 × 576 RGB field):

| | Time |
|---|---|
| Installing the sample: its environment, scikit-image, the SDK, and its nodes described | 4.1 s |
| The first run: the worker started, scikit-image imported, then denoising | 1,054–1,088 ms |
| A warm run: denoising, 200 iterations | 280–319 ms |
| A call on an 8 × 8 image, one iteration, in all: the pipeline, the segments, the messages, Python's dispatch | 0.22–0.28 ms |

The sample's environment takes 213 MB on disk.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in about a quarter under the estimate: the SDK and the supervisor were smaller than allowed for.
- **Python runs only when asked:** `Extensions::open` loads no Python code, as in P24, so that tests and examples without uv run as before; `Extensions::open_with` takes a `PythonRuntime`, and the server always gives one. A server without uv can't install a Python extension.
- **The interpreter isn't held to the two-week rule.** uv's managed Pythons carry no dates; an extension's `requires-python` bounds it. Asked for `>=3.12`, uv picked CPython 3.15.2, the newest, which wheels may not support yet: the sample and the tests ask for `>=3.12,<3.15`.
- **Offline installs** need a uv-managed Python already in uv's cache, besides the wheelhouse.
- **One call at a time per extension,** as D10 has one worker per extension. Tiles of a Python node run one after the other.
- **The SDK installed into each environment comes from this source tree** by default (`python/oscope-ext`). The installer (P54) will have to ship it, and set `[python] sdk`.
- **Tests that need uv** are gated by `OSCOPE_PYTHON=1`, added to CLAUDE.md, and run in CI on Ubuntu alone, as you decided. `ex-python-node --smoke` passes without uv, saying it skipped.

**Other choices:**

- **The host owns every segment,** as for Rust extensions' buffers: it copies each input into one, and makes each output's when the worker asks, so a crash can't leave files in `/dev/shm`.
- **The worker maps segments with `mmap`,** not `multiprocessing.shared_memory`, whose resource tracker unlinks segments it didn't make before Python 3.13.
- **Inputs are read-only arrays:** a node that writes into its input fails, rather than changing the host's frame.
- **The nodes are described once, at install,** and kept in `.oscope/nodes.json`, so a start loads the extension without starting Python. The worker starts on the first run.
- **A node's exception** is the step's error, with its traceback in the log; its worker goes on. **A dead worker** is found before the next call and replaced; one that dies or takes too long during a call fails that call, saying what it last wrote on stderr.
- **On Windows,** killing the worker's process leaves no Python behind: the tests found none after killing workers.
- **`ex-omext`'s dummy** now declares its node with the SDK. A server running Python makes its environment and checks its nodes when it installs it.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `python/oscope-ext` | pytest (8) | **Schemas** from type hints (each type, required and default parameters); a hint without a schema refused. **A node's descriptor** as the pipeline's, and a run. **Outputs** made float32 images, their count checked; an explicit schema kept; a node declared once. **The worker:** a ping, a description, an unknown message answered; a run through segments made as the host makes them, both ways; a failing run said, and the worker goes on |
| `oscope-ext` | `python` (5, with `OSCOPE_PYTHON=1`) | **Installing** makes the environment and keeps the nodes; their schemas from type hints; the worker started on first use. **Shared memory** both ways, to the bit; a node doubling; a node with two outputs. **A failing node** said, the same worker going on. **A worker that dies mid-call,** or is killed mid-call, stopped, the call failed, and the next call starts another; **one killed between calls** found dead and replaced. **A call over its time-out** stops the worker within seconds; **an idle worker** is stopped. **A module whose nodes aren't its manifest's** refused at install, leaving nothing |
| `oscope-ext` | `packages` (a case more) | An entry under `.oscope` refused |
| `oscope-shm` | `shm` (a test more) | A segment: zeros, then its samples read back; its name as the system has it; too many samples, a name in use and a bad name refused; an empty one a byte long; the name free again once it goes, and on Linux the file gone |
| `oscope-server` | `config` (a test extended) | `[python]` read, its folders relative to the file, its time-outs |
| `ex-python-node` | `--smoke` | Installed and loaded; three runs on a small field; denoising gains more than 3 dB |

**Where they ran:** on Windows, and on Linux under WSL, both with uv; the `python`, `packages` and `shm` suites and `ex-python-node --smoke` 5 times out of 5 each with their binaries pinned to two cores. The pinned Python runs took 9.4–9.8 s each.

**Mutation checks** (each broke the code on purpose, then the tests were run): eleven of twelve caught. The one missed was equivalent: it put `int` before `bool` in the hint lookup, which compares with `is`, so the order never mattered. Its comment, which said it did, was wrong and is gone. A mutant dropping `Literal` hints took its place, and was caught.

| Broken on purpose | Caught by |
|---|---|
| Inputs not copied into their segments | `installing_makes_an_environment_whose_worker_runs_nodes_through_shared_memory` |
| Outputs not read from theirs | the same |
| A dead worker not noticed before a call | `a_worker_that_dies_or_is_killed_mid_call_is_an_error_and_starts_again` |
| The call time-out ignored | `a_call_that_takes_too_long_stops_the_worker_and_an_idle_worker_is_stopped` |
| An idle worker kept | the same |
| A node's failure stopping its worker | `a_failing_node_says_why_and_its_worker_goes_on` |
| A module's nodes not checked at install | `a_module_whose_nodes_are_not_its_manifest_s_is_refused_at_install` |
| `.oscope` allowed in packages | `a_package_that_could_escape_or_clash_is_refused_and_leaves_nothing` |
| Parameters without defaults not required (SDK) | `test_a_schema_comes_from_type_hints_and_defaults` |
| `Literal` hints not understood (SDK) | the same |
| The worker not writing an output into its segment (SDK) | `test_a_run_reads_and_writes_the_host_s_segments` |

## Follow-ups and known gaps

- **Installing as a job:** an install with a large environment holds the upload route for seconds, or minutes without uv's cache.
- **A pool of workers** per extension, if batch work needs Python nodes in parallel.
- **The installer** shipping the SDK and uv (P54).
- **Drivers and panels** from extensions (P27).

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **The sample** calls scikit-image's `denoise_tv_chambolle`, an implementation of A. Chambolle, "An Algorithm for Total Variation Minimization and Applications", *Journal of Mathematical Imaging and Vision* 20, 2004. OScope implements no part of it.
- **The noise** in `ex-python-node` is a sum of twelve uniform numbers from a xorshift generator (G. Marsaglia, "Xorshift RNGs", *Journal of Statistical Software* 8(14), 2003), close to Gaussian by the central limit theorem.
