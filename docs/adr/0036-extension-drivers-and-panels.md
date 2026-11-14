# ADR-0036: Extension drivers and panels

- Status: Accepted
- Date: 2026-11-14
- Phase: P27

## Context

- **D11:** extensions add hardware drivers and describe UI panels, among other things. P27 brings drivers for cameras, stages and lights (Rust: all three; Python: stages and lights, and cameras whose frames travel through shared memory), and panels the server describes for the M2 app to draw.
- **The HAL** (ADR-0012) is async traits with exacting promises. Its conformance suite checks each: refusals by error kind, settings within capabilities, moves that end idle, a position track, frames that match their settings, frames lost to a full pool counted.
- **ABI 1** (ADR-0019) carries nodes only, and any change to it is a new version. Python workers (ADR-0020) answer messages.
- **teta-wot fixes its Things when the server is built** (ADR-0004), so extension drivers must be registered before the devices are made from the configuration.
- **No planned ADR covers this.** This one takes the next free number, 0036.

## Decision

**1. Drivers are primitive; OScope keeps the HAL's promises.** An extension's driver only applies, starts, reports and captures. OScope wraps it in a HAL driver of its own, which:
- **checks** camera settings, stage targets and speeds, and lights against the driver's capabilities, refusing with the HAL's errors;
- **refuses** what can't change while a camera streams, and a second start;
- **waits** for software triggers, and **times, numbers and pools** frames, counting those lost to a full pool;
- **waits** for a move, a homing or a stop to end, reporting the position each report interval, and keeps the position track;
- **refuses** homing and a trigger output that the capabilities don't offer, and moves while in alarm.

So an extension's driver passes the conformance suite without knowing it.

**2. The drivers' protocol,** the same for both languages: a method name and its parameters as JSON, answered with JSON, and a camera's capture into OScope's buffer. Parameters are the JSON of the HAL's types (serde's forms):

| Kind | Methods |
|---|---|
| camera | `capabilities`, `configure(settings)`, `start(settings)`, `stop`, `temperature`; and `capture`, into OScope's buffer, blocking until the frame is ready |
| stage | `capabilities`, `status`, `position`, `move_to({target, speed})`, `jog({axis, towards_max, speed})`, `stop`, `home`, `clear_alarm`, `arm_trigger({axis, positions})`: moves start and return; `status` says when they end |
| illumination | `capabilities`, `state`, `set(light)` |

A driver's error is a string, which becomes the HAL's `Device` error.

**3. ABI 2.** The Rust ABI's version becomes 2:
- **the extension's table** adds `driver_call(driver, method, params)` and `capture(driver, pixels, len)`;
- **the host's table** adds `reply`, for a driver call's answer;
- **the description** becomes an object: `{"nodes": [...], "drivers": [{"name", "kind"}]}`.

The rest of ADR-0019 holds: the host owns every buffer, frames included, and panics are caught on both sides. No library of ABI 1 exists outside this repository, whose samples are built with the SDK, so nothing needs to load both. The SDK has a trait for each kind (`CameraDriver`, `StageDriver`, `IlluminationDriver`) in the HAL's types, and `export!(nodes: [...], drivers: [camera("name", driver), ...])`. `export!(node, ...)` still lists nodes alone.

**4. Python drivers.** `@driver(name, kind=...)` declares a class. The worker makes one instance on first use, and answers `{"driver": {"name", "method", "params"}}` with `{"result": ...}`. A method gets one argument, the parameters' JSON, or none. For `{"capture": {"name", "segment", "bytes"}}`, the camera's `capture()` returns its frame as an array, which the worker copies into a segment the host made (ADR-0020). The description lists `drivers` beside `nodes`. P26's description, a list of nodes, still reads.

**5. Registration at start-up.** A start loads extensions before devices:
- each extension's drivers must be exactly its manifest's `provides.drivers`, as nodes are, and none may take the name of one of OScope's own: `sim`, `fake` or `manual`;
- `Extensions::drivers()` maps each name to its kind and its link to the code;
- `Devices::from_config_with` takes them, so `[devices]` may name `camera = "example.fakecam"`. A name no one knows is refused with the drivers there are;
- the printed documents (`--print-td`, `--print-openapi`) load no extension, so there a driver that isn't OScope's own is taken as a fake.

**6. Panels** are JSON documents, `panels/<name>.json`, for each name in `provides.panels`, which must be among the manifest's `assets`:

```json
{ "title": "…", "description": "…",
  "form": { "type": "object", "properties": { … } },
  "submit": { "kind": "node", "node": "example.denoise_tv" },
  "view": "image" }
```

- The form is a JSON Schema of an object that the pipeline's checks accept (ADR-0017).
- `submit` runs one of the extension's own nodes on the live view's newest frame, or invokes a Thing's action (`{"kind": "action", "thing", "action"}`).
- `view` is `image`, `table`, `json` or `text`.

A panel that doesn't hold refuses the install, and the extension at a start. The `extensions` Thing serves the loaded extensions' panels: the `panels` property lists them, and `GET panel/{name}` gives one. The M2 app draws and submits them; OScope ships no code for them.

**7. The `pipeline.batch` job** (Checkpoint B). The `preview` Thing registers this job kind with the job service, on the same nodes live recipes use, extensions' among them. Its parameters are a recipe and `frames`, 1 to 100. Each item is the camera's next frame in linear light, as live recipes see it, with `region` the whole frame if the recipe takes one. Each image output is saved as `{data root}/batch/<time>-<n>/frame-<i>-<output>.png`. The camera must be streaming: an item waits 10 s for a frame, then fails.

## Consequences

- **Writing a driver is small:** the sample camera is about 150 lines, and passes the conformance suite as it is, because OScope's adapter does what the HAL promises.
- **Every driver call crosses a boundary:** a C call for Rust, a message for Python, well under a millisecond. Polling a stage for the end of a move costs a call each report interval.
- **A Python camera** costs a message and a copy into shared memory each frame. That suits slow or unusual cameras, not the MIChrome 20 at full rate.
- **ABI 1 libraries are refused.** None exist beyond the repository's samples, rebuilt with the SDK.
- **Panels are data,** so they can't run code in the app; what a panel does is limited to running a node or invoking an action.
- **The batch job writes PNG files,** 8 bits a channel, clamped. Projects and their stores (P28) will give results a proper home.

## Alternatives considered

- **Drivers that implement the whole HAL themselves.** Each extension would repeat the checks, the timing, the pool and the track, and most would fail the conformance suite somewhere. Keeping them in OScope's adapters keeps one copy, tested.
- **A C table of functions for each kind of driver.** Three more tables, each with its own layout to keep stable. JSON for the methods reuses the HAL's serde forms, and only frames, the bulk, get a call of their own.
- **Callbacks from the extension into the host for each frame (push).** A real camera's SDK pushes frames from its own threads. Pulling, with a blocking `capture` on OScope's thread, keeps every buffer and thread OScope's own, and a driver with its own thread can block until its next frame is in.
- **Panels in HTML or JavaScript.** Extensions would ship code into the app, which plan §2 rules out. A schema-driven form and a submission are enough for settings and results.
- **A shell script for Checkpoint B.** An example program does the same through the API, runs headless as a smoke check in the local replays on both platforms, and needs no `curl` or WebSocket tool.
