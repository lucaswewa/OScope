# ex-live-view

The live preview, and its benchmark: the simulated camera's frames as JPEGs through the `preview` Thing's binary WebSocket (P17, ADR-0014), always 1000 × 667 of the whole 5472 × 3648 sensor, with each step's times from the camera to the page. [The live view's baseline](../../docs/performance/live-view-baseline.md) is what it measured.

## How to try it

```sh
cargo run --release -p ex-live-view
```

It starts the simulated microscope on the MIChrome 20's full sensor (5472 × 3648 pixels of 0.24 µm at 10×), homes the stage, places it over the H&E slide's tissue, turns the light on and starts streaming. Then it prints `ex-live-view: open http://127.0.0.1:8218/`. A debug build works too, more slowly.

**The page:**

- **The live image,** 1000 × 667 of the whole sensor, decoded with `createImageBitmap` and drawn with WebGL.
- **Under it:** the frames a second, the size sent and the sensor's region it shows, the JPEG quality, the latency, and the bytes a second.
  - **The latency** has two parts: the server's, from the end of the frame's exposure to its message being ready; and the page's, from the message arriving to the frame being drawn.
- **Jog buttons:** held down, the stage jogs along that axis at the speed set; let go, it halts.
  - **The image slides as the stage and the camera's mounting say:** left while jogging x +, and up while jogging y +, at the speed over the pixel's size. The page says how many sensor pixels a second to expect.
  - **z** goes in and out of focus.
- **The quality slider** sets the JPEG quality.
- **Timings:** each step's rate, mean, median, 95th percentile and longest over the last 30 s, in the order a frame goes through them:
  - **the server's:** the camera's rendering, acquisition, the session and the preview chain's steps, read from `/bench/timings.json`;
  - **the page's:** the message's transit, the JPEG's decoding, the draw, and the time from the exposure's end to the frame drawn.
  - **Copy the timings** copies them all as JSON.

**With the GPU:** `cargo run --release -p ex-live-view --features cuda` makes the previews with CUDA, when there is a GPU. `GET /api/v1/preview/gpu` says whether there is one. The simulated camera still renders on the CPU.

**The benchmark:** with the example serving, `node examples/ex-live-view/bench/bench.mjs --label cpu` opens the page in headless Chrome. It records the timings over three windows of 30 s:

- from the start, while the camera's tiles are rendered for the first time;
- with the stage still;
- jogging along x at 500 µm/s.

It writes them to `bench-cpu.json`. It needs Playwright, from `npm ci` in `web/`. `--channel msedge` uses Edge, and `--headed` shows the window.

**How the times are taken:**

- `oscope_core::timing` keeps each step's times over the last 30 s. The example turns it on.
- With it on, the GPU's steps wait for the GPU to finish, so each is timed on its own.
- `/bench/timings.json` also gives the offset from the camera's clock to the UNIX epoch, so the page can put the headers' times on its own clock: that is how it measures transit and end to end.

**The stream, with your own client:**

- Connect a WebSocket to `ws://127.0.0.1:8218/api/v1/preview/stream`.
- Send a JSON text message with what you want: `{"width": 800, "height": 600, "quality": 80}`.
  - Add `"roi": {"x": 400, "y": 300, "width": 300, "height": 200}` for a region of the sensor.
  - Add `"max_fps": 5` to slow it down.
  - Each message replaces the last, so give every field each time. What you leave out takes the `preview` Thing's `defaults`.
- Each binary message is a 76-byte header, then a JPEG. ADR-0014 gives the layout.
- **Acknowledge** what you've shown: `"ack"`, the number of the next message you're ready for, with your other fields. That is the header's message number plus one, and 0 in your first message. Then you have at most two messages you haven't shown, always of the newest frame. Without it, the operating system's socket buffers can hold many stale frames for a slow client.

```sh
# The defaults, which you can change; the sessions open, with their statistics.
curl http://127.0.0.1:8218/api/v1/preview/defaults
curl http://127.0.0.1:8218/api/v1/preview/sessions
curl -X PUT -H "Content-Type: application/json" \
     -d '{"black": 0.0, "white": 0.8, "gamma": null, "gains": [1.0, 1.0, 1.0]}' http://127.0.0.1:8218/api/v1/preview/look
# The server's steps.
curl http://127.0.0.1:8218/bench/timings.json
```

**Things to know:**

- **The simulator runs in real time** once it has homed the stage, at 50 times real time. Frames come at most at the simulated camera's rate: 7.2 a second at full resolution in 16 bits. It renders more slowly than that, so it skips frames.
- **A preview is never larger than the region it shows.**
- **Settings are saved** in the data root's `settings` folder, under `.oscope/ex-live-view/`, and the timings in its `bench` folder.

**`--smoke`** serves on a port the system picks, with a temporary data root, then checks:

- that the page is served;
- that four previews of 1000 × 667 arrive during a jog, each acknowledged;
- that the `preview` Thing lists one session;
- that `/bench/timings.json` has the camera's, the preview's and the session's steps.

```sh
cargo run -p ex-live-view -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
