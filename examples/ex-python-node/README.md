# ex-python-node

Run this port in the Linux development container, using its Python 3.12 and uv.
The standalone worker is available; server integration remains a later phase.
See the [R05 procedure](../../docs/phases/r05-storage-processing.md#manual-test).

A Python extension through OScope (P26, [ADR-0020](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0020-the-python-extension-runtime.md)): the sample [`ext-python-node`](../ext-python-node/README.md) installed, its environment made by uv, loaded at a start, and run in a recipe by its worker.

**It needs uv** ([docs.astral.sh/uv](https://docs.astral.sh/uv/)): on `PATH`, in `~/.local/bin`, or named by `OSCOPE_UV`. The image provides Python; uv installs scikit-image and the SDK, reusing its cache on later installs.

## How to try it

```sh
cargo run -p ex-python-node
```

It:

1. **packs the sample** into `.oscope/ex-python-node/org.oscope.examples.tv-denoise-0.1.0.omext`;
2. **installs it,** making its environment with uv in `.oscope/ex-python-node/extensions/org.oscope.examples.tv-denoise/0.1.0/.oscope/venv`: a Python, scikit-image and OScope's SDK. Then it starts the extension's worker once, for the nodes it describes, and keeps them;
3. **starts again,** as the server does, which loads the extension without starting Python;
4. **runs a recipe** of `example.denoise_tv` three times on a 768 × 576 field of the simulated H&E slide, with noise of σ 0.08 added. The first run starts the worker, which imports scikit-image; the next two find it warm, in the same process;
5. **times a call** on an 8 × 8 image, with one iteration: what the boundary costs;
6. **measures** the noisy and denoised fields against the clean one;
7. **writes** `noisy.png` and `denoised.png` in `.oscope/ex-python-node/out/`.

On the owner's PC, in release, with uv's cache warm:

```text
installed in 4.1 s, with its environment: Python, scikit-image and OScope's SDK
run 1, starting its worker: 1088 ms, in worker 17080
run 2, warm: 293 ms, in worker 17080
run 3, warm: 291 ms, in worker 17080
a call on an 8 × 8 image, one iteration: 0.28 ms, in all
against the clean field: 21.9 dB noisy, 35.4 dB denoised
```

Running it again says *installed already*.

**In the server:** `oscope-server` runs Python extensions with the settings of its configuration's `[python]` table (uv, the SDK, a wheelhouse for offline installs, timeouts). Install the package through the upload route, as in [`ex-omext`](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/examples/ex-omext/README.md), and start the server again: live recipes may then use `example.denoise_tv`, though at hundreds of milliseconds a frame it suits batch work better.

**`--smoke`** does the same on a 128 × 96 field, in a temporary folder, and checks that denoising gains more than 3 dB. Missing uv or a worker failure makes it fail.
