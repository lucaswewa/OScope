# ex-checkpoint-b

**Checkpoint B** (plan §7): one recipe of stock, Rust and Python nodes, run live on the preview, then as a batch job.

It needs uv, as [`ex-python-node`](../ex-python-node/README.md) does.

## How to try it

```sh
cargo run -p ex-checkpoint-b
```

It packs both sample extensions, [`ext-rust-node`](../ext-rust-node/README.md) (an emboss node) and [`ext-python-node`](../ext-python-node/README.md) (scikit-image's denoising), and installs them in `.oscope/ex-checkpoint-b/`. uv makes the Python one's environment. Then it starts the simulated microscope with them, and on `http://127.0.0.1:8232/` it:

1. **starts the camera,** reading out the sensor's middle 960 × 720: the batch takes whole frames, so that it takes seconds, not minutes, on a small machine;
2. **sets the live recipe:** a stock Gaussian blur, then the Rust emboss, then the Python denoising. Its check runs it on a 320 × 240 image, against a budget of 2 s: Python nodes run live, but at low rates;
3. **watches five previews** come through it: a recipe that failed would stop them;
4. **submits the same recipe as a `pipeline.batch` job** over the camera's next three frames, waits for it, and lists the PNG files it saved in `.oscope/ex-checkpoint-b/data/batch/`.

```text
live: the recipe ["smooth","emboss","denoise"] is on the preview: 144.5 ms on a 320 × 240 image
live: 5 previews through it
batch: three frames through it, saved in the data root's batch/:
  batch/20261114-062214-1/frame-1-image.png
  …
checkpoint B: one recipe of stock, Rust and Python nodes, live and as a batch job
```

Then it keeps serving [`ex-live-view`](../ex-live-view/README.md)'s page: the live view with the recipe on, embossed and smoothed.

**`--smoke`** does the same in a temporary folder, and exits. Without uv, it says so and passes.

## The batch job

`pipeline.batch` is a job kind of the server's (P27), for any recipe of the nodes live recipes may use:

```sh
curl -X POST -H "Content-Type: application/json" \
     -d '{"kind": "pipeline.batch", "params": {"recipe": {…}, "frames": 3}}' \
     http://127.0.0.1:8232/api/v1/jobs/submit
```

Each item is the camera's next frame in linear light, as live recipes see it, so the camera must be streaming. A recipe's `region` input, if it has one, is the whole frame. Each image output goes to `{data root}/batch/<time>-<n>/frame-<i>-<output>.png`, and the job's results list them.
