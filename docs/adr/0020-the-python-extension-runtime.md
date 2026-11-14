# ADR-0020: The Python extension runtime

- Status: Accepted
- Date: 2026-11-14
- Phase: P26

## Context

- **D10 and plan §5.8:** third parties write nodes in Python, with their own dependencies, isolated from the server. Each extension gets its own environment, made by uv with a uv-managed Python 3.12 or later, and runs in a worker process of its own. Frames are shared as NumPy arrays through shared memory, at about 1 ms a call.
- **The owner's decisions for P24–P34:**
  - uv is installed on the owner's PC, and set up in CI with Astral's `setup-uv`. The SDK's tests run in GitHub CI on Ubuntu alone, and in the local replays on both platforms;
  - an extension's Python packages are its own, outside OScope's licence list, but still held to the two-week rule, through uv's `exclude-newer`.
- **Rust extensions** run in the process (ADR-0019). A Python worker must fail without taking the server down.
- **`oscope-shm`** (P19) already maps named shared memory on both platforms: `Local\oscope-…` on Windows, `/dev/shm/oscope-…` on Linux.

## Decision

**1. The SDK** is the `oscope_ext` package, in `python/oscope-ext/`:
- **`@node(id, …)`** declares a function as a node. Its positional parameters are its inputs: images as NumPy arrays of `float32`, shaped `(height, width, channels)`. Its keyword-only parameters are its parameters. It returns an image or a tuple of them, one for each of its `outputs`.
- **A parameter schema** comes from the type hints (`int`, `float`, `bool`, `str`, `Literal`) and defaults, a parameter without a default being required. `params=` gives one explicitly instead.
- **A node's descriptor** is the pipeline's (ADR-0017). As in ABI 1 (ADR-0019), every port is an image, none optional, on the CPU.
- **`oscope_ext.worker`** is the message loop.
- Its own dependencies are NumPy, and pytest for its tests, locked in `uv.lock` with `exclude-newer` at two weeks before P26.

**2. Environments, made at install.** When OScope runs Python, installing a Python extension unpacks it, then:
1. makes its environment in `<version>/.oscope/venv`: `uv venv --managed-python --python <requires-python>`;
2. installs its `dependencies` and OScope's SDK into it with `uv pip install`. Online, it adds `--exclude-newer`, two weeks before the install. With a wheelhouse, it adds `--offline --no-index --find-links <wheelhouse>`;
3. starts its worker once, for the nodes it describes, and keeps them in `<version>/.oscope/nodes.json`.

The nodes are checked as a Rust extension's are: exactly the manifest's `provides.nodes`, as the registry checks nodes, and none with a stock node's ID. If any step fails, the install fails and leaves nothing behind. A start reads the kept nodes without starting Python. Packages may not use the name `.oscope`.

**3. Finding uv:** the configured path, then `OSCOPE_UV`, then each folder of `PATH`, then `~/.local/bin` and `~/.cargo/bin`, where uv's installers put it. Without uv, installing a Python extension fails, saying so.

**4. The worker.** `python -m oscope_ext.worker <entry>` runs with the environment's Python, in the package's folder, which puts the extension's modules on `sys.path`. There is one worker for each extension, serving one call at a time:
- **started on the first run** of one of its nodes, and answering a ping within the start time-out: two minutes by default;
- **kept warm** between calls;
- **stopped** when a call takes longer than its time-out (two minutes by default), when it breaks the protocol, or after an idle time (five minutes by default). The next call starts another;
- **started again** when it is found dead before a call. When it dies during a call, that call fails;
- **killed** when the server stops.

**5. Messages** go on the worker's stdin and stdout: a little-endian `u32` length, then that many bytes of UTF-8 JSON.
- `ping` gets `pong`.
- `describe` gets the nodes.
- `run` (node, parameters, inputs) gets an `output` request for each output in turn, which the host answers with a segment's name, then `done`, or `error` with a traceback.

The worker sends a node's prints to stderr. The host logs stderr, and adds its last lines to the error when the worker dies.

**6. Frames travel through shared memory the host owns** (`oscope-shm`'s `Segment`):
- each input is copied into a new segment;
- each output goes into a segment the host makes when the worker asks for it, sized as the worker says (at most 2³⁰ samples), and is copied out after `done`;
- segments are named `py-<pid>-<n>`;
- the worker maps them with Python's `mmap`, as NumPy arrays that share their memory: inputs read-only. It unmaps them after each call. `multiprocessing.shared_memory` isn't used: before Python 3.13, its resource tracker unlinks segments it didn't make;
- the host's segments go when it drops them: the name at once on Linux, the memory once both sides have unmapped it.

**7. Failures.** A node that raises gives its message as the step's error, and its traceback to the log. Its worker goes on. A worker that dies, takes too long or breaks the protocol is stopped. The step's error says why, with what it last said on stderr.

**8. Python runs only when asked.** `Extensions::open` loads no Python code, so that tests and examples without uv run as before. `Extensions::open_with` takes a `PythonRuntime`. `oscope-server` always gives one, from its configuration's `[python]` table: uv, the SDK's folder, a wheelhouse, and the time-outs.

**9. Tests that need uv** run only with `OSCOPE_PYTHON=1` (CLAUDE.md). CI sets it on Ubuntu, after `setup-uv` (uv 0.13.6); the local replays set it wherever they find uv. The SDK's pytest suite runs in the same places.

## Consequences

- **A crashing or hanging extension costs a call,** not the server.
- **A call costs about a quarter of a millisecond** beyond the node's own work, measured on an 8 × 8 image, with the pipeline, the segments, the messages and Python's dispatch. That is within the plan's 1 ms. Starting a worker and importing scikit-image costs about a second, once.
- **Environments are large:** the scikit-image sample's is about 200 MB. uv keeps the packages in its cache once, and links or copies them into each environment.
- **Installing needs the network,** unless a wheelhouse is given. Offline, uv needs a managed Python in its cache, as it can't download one.
- **One call at a time per extension.** A pool of workers would let tiles run in parallel. It can come later if batch work needs it.
- **The interpreter isn't held to the two-week rule:** uv's managed Pythons carry no dates. An extension's `requires-python` bounds it.

## Alternatives considered

- **Embedding Python in the server (PyO3).** No isolation: a crash or a hung GIL takes the server with it, and every extension would share one interpreter and one environment.
- **One environment for every extension.** Their dependencies would clash.
- **conda.** Heavier, slower, and its packages' licences are mixed. uv is one binary and fast.
- **Frames through the pipes.** Each frame would be copied through the kernel twice, and the plan wants zero-copy NumPy arrays.
- **The worker making the output segments.** Then the worker would own their names. On Linux it would have to unlink them after the host had read them, and a crash would leave files in `/dev/shm`. The host owning every segment keeps one owner, as for Rust extensions' buffers (ADR-0019).
- **HTTP or gRPC to the worker.** More moving parts for messages that pipes carry well.
