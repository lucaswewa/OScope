# ex-http-walkthrough

The instrument on the control plane: the simulated microscope's Things over HTTP, with `curl`, and a plain control page (P16).

**The Things:**

| Thing | Properties | Actions | Events |
|---|---|---|---|
| `camera` | `capabilities`, `settings`, `exposure` (ms), `gain`, `roi`, `trigger`, `mode`, `format`, `streaming`, `temperature` | `set_mode`, `start_streaming`, `stop_streaming`, `trigger_exposure` | `mode_changed` |
| `stage` | `capabilities`, `position` (µm), `status`, `moving`, `speed` (µm/s, saved) | `move_to`, `move_by`, `jog`, `halt`, `home`, `clear_alarm` | `moved`, `alarm` |
| `illumination` | `capabilities`, `lights` | `set_light` | |
| `objective` | `capabilities`, `catalogue`, `fitted`, `pixel_at_sample` (µm) | `fit` | |
| `simulation` | `samples`, `loaded`, `rate`, `tile_cache`, `confocal` | `load`, `unload`, `import` | |

- **Which driver each device uses** comes from the configuration's `[devices]`: here, the simulator for all of them.
- **Settings are saved** in the data root's `settings` folder, and set again when the server starts: the camera's, the stage's speed, the lights, the objective fitted.
- **A driver's error answers with its HTTP status:**
  - 400 for a value it can't take;
  - 409 when the device is busy, or in alarm;
  - 503, 504 or 502 when it's disconnected, too slow or failing.

  The problem's `title` names the kind (ADR-0012, ADR-0034).

## How to try it

```sh
cargo run -p ex-http-walkthrough
```

It serves the simulated microscope on the scaled-down sensor, over the H&E slide, and prints `ex-http-walkthrough: open http://127.0.0.1:8212/`.

**The control page** shows:

- the stage's position, four times a second, and its status;
- buttons to home, halt, and move to a position;
- jog buttons, which jog while held down;
- the objective to fit;
- the transmitted light, on or off, and its intensity;
- the sample to load, or unload.

**With `curl`,** from another terminal, in the same order:

```sh
# Each Thing describes itself: its properties, actions and events.
curl http://127.0.0.1:8212/api/v1/stage/

# Properties: read with GET.
curl http://127.0.0.1:8212/api/v1/stage/position
curl http://127.0.0.1:8212/api/v1/camera/exposure

# Writable properties: set with PUT.
curl -X PUT -H "Content-Type: application/json" -d 20 http://127.0.0.1:8212/api/v1/camera/exposure

# A value the camera can't take answers 400, with what was wrong.
curl -i -X PUT -H "Content-Type: application/json" -d 999 http://127.0.0.1:8212/api/v1/camera/gain

# Actions: POST answers 201 at once, with the invocation to follow.
curl -X POST http://127.0.0.1:8212/api/v1/stage/home
curl -X POST -H "Content-Type: application/json" \
     -d '{"x": 45000, "y": 12500, "z": 5000}' http://127.0.0.1:8212/api/v1/stage/move_to
curl http://127.0.0.1:8212/api/v1/action_invocations/<id>     # pending, running, then completed

# Events: the stage says where it arrived, as server-sent events.
curl -N http://127.0.0.1:8212/api/v1/stage/moved

# The rest of the instrument.
curl -X POST -H "Content-Type: application/json" -d '{"name": "50x"}' http://127.0.0.1:8212/api/v1/objective/fit
curl -X POST -H "Content-Type: application/json" \
     -d '{"channel": "transmitted", "on": true, "intensity": 0.5}' http://127.0.0.1:8212/api/v1/illumination/set_light
curl http://127.0.0.1:8212/api/v1/simulation/samples
curl -X POST -H "Content-Type: application/json" -d '{"name": "test target"}' http://127.0.0.1:8212/api/v1/simulation/load

# Your own image, from the server's disk, at its scale; then load it.
curl -X POST -H "Content-Type: application/json" \
     -d '{"name": "mine", "path": "C:/data/section.png", "um_per_pixel": 0.25}' http://127.0.0.1:8212/api/v1/simulation/import
```

The OpenAPI document of every route is at `http://127.0.0.1:8212/openapi.json`, with pages to try them at `/docs`.

**Things to know:**

- **The stage starts where it was switched on.** It needs homing before it moves, as grblHAL's does: `home` first.
- **Moves take time:** 10 mm/s by default. A `move_to`'s invocation completes when the stage is still at the target, within its 2 µm of backlash.
- **Stopping is `halt`,** not `stop`, which every Thing has for itself in `teta-wot` (ADR-0034). Cancelling a move's invocation (`DELETE` it) also stops the stage.
- **Streaming** sends frames to acquisition, which completes them with the stage's position, the objective and the lights, and publishes them on the frame bus. P17 shows them as a live preview.

**`--smoke`** serves on a port the system picks, with a temporary data root, then checks:

- that the page is served;
- that the stage homes, and moves to x = 30 mm, and reports being there;
- that a gain of 999 is refused with 400;
- that the simulator offers five samples.

```sh
cargo run -p ex-http-walkthrough -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
