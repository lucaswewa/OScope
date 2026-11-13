# ex-live-view

The live preview: the simulated camera's frames as JPEGs, each the size of the page's view, through the `preview` Thing's binary WebSocket (P17, ADR-0014).

## How to try it

```sh
cargo run -p ex-live-view
```

It starts the simulated microscope on the scaled-down sensor (1368 × 912 pixels of 0.96 µm at 10×), homes the stage, places it over the H&E slide's tissue, turns the light on and starts streaming. Then it prints `ex-live-view: open http://127.0.0.1:8218/`.

**The page:**

- **The live image,** decoded with `createImageBitmap` and drawn with WebGL.
- **Scroll on it to zoom:** the page asks the server for that region of the sensor, and gets it at up to full resolution. **Whole frame** zooms out again.
- **Under it:** the frames a second, the size sent and the sensor's region it shows, the JPEG quality, the latency, and the bytes a second.
  - **The latency** has two parts: the server's, from the end of the frame's exposure to its message being ready; and the page's, from the message arriving to the frame being drawn.
- **Jog buttons:** held down, the stage jogs along that axis at the speed set; let go, it halts.
  - **The image slides as the stage and the camera's mounting say:** left while jogging x +, and up while jogging y +, at the speed over the pixel's size. The page says how many sensor pixels a second to expect. `jogging_moves_the_image_at_the_stage_s_speed` in `oscope-api`'s tests measures it: 103.7 pixels a second on the half-size preview, against 104.2 expected at 200 µm/s.
  - **z** goes in and out of focus.
- **The quality slider** sets the JPEG quality.

**With the GPU:** `cargo run -p ex-live-view --features cuda` makes the previews with CUDA, when there is a GPU. `GET /api/v1/preview/gpu` says whether there is one.

**The stream, with your own client:**

- Connect a WebSocket to `ws://127.0.0.1:8218/api/v1/preview/stream`.
- Send a JSON text message with what you want: `{"width": 800, "height": 600, "quality": 80}`.
  - Add `"roi": {"x": 400, "y": 300, "width": 300, "height": 200}` for a region of the sensor.
  - Add `"max_fps": 5` to slow it down.
  - Each message replaces the last, so give every field each time. What you leave out takes the `preview` Thing's `defaults`.
- Each binary message is a 72-byte header, then a JPEG. ADR-0014 gives the layout.
- **Acknowledge** what you've shown: `"ack"`, the number of the next message you're ready for, with your other fields. That is the header's message number plus one, and 0 in your first message. Then you have at most two messages you haven't shown, always of the newest frame. Without it, the operating system's socket buffers can hold many stale frames for a slow client.

```sh
# The defaults, which you can change; the sessions open, with their statistics.
curl http://127.0.0.1:8218/api/v1/preview/defaults
curl http://127.0.0.1:8218/api/v1/preview/sessions
curl -X PUT -H "Content-Type: application/json" \
     -d '{"black": 0.0, "white": 0.8, "gamma": null, "gains": [1.0, 1.0, 1.0]}' http://127.0.0.1:8218/api/v1/preview/look
```

**Things to know:**

- **The simulator runs in real time** once it has homed the stage, at 50 times real time. Frames come at the simulated camera's rate: 7.2 a second at full resolution in 16 bits.
- **A preview is never larger than the region it shows:** the page enlarges it.
- **Settings are saved** in the data root's `settings` folder, under `.oscope/ex-live-view/`.

**`--smoke`** serves on a port the system picks, with a temporary data root, then checks:

- that the page is served;
- that four previews of 342 × 228 arrive during a jog, each acknowledged;
- that the `preview` Thing lists one session.

```sh
cargo run -p ex-live-view -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
