# ADR-0012: The hardware abstraction layer

- Status: Accepted
- Date: 2026-11-12
- Phase: P09

## Context

- **Three kinds of driver plug into the same seams:**
  - the simulator (P10–P15);
  - the real drivers in M3: the Tucsen SDK, and grblHAL over a serial port;
  - extension drivers (P27), in Rust or Python.

  The configuration picks them when the server starts (P16), so the server holds them as trait objects.
- **The hardware differs in ways that matter** (P02b, `docs/hardware.md`):
  - **The MIChrome 20:**
    - no trigger input, no sensor timestamps, and a frame index stuck at 0;
    - a rolling shutter;
    - three modes, each with its own rate for each bit depth;
    - an ROI that the camera rounds, and that doesn't raise the rate.
  - **The camera that replaces it** will have a trigger input.
  - **grblHAL** reports its position at intervals, with a latency, has homing switches, and has aux outputs.
  - **The lights** aren't chosen yet.
  - **Objectives** are swapped by hand (D18).
- **Device operations take time:** a move takes seconds, a frame an exposure. The server is async (tokio). On the pinned toolchain (Rust 1.98.1), a trait with `async fn` methods can't be used as a trait object: the compiler refuses `dyn` with error E0038.
- **Frames cross layers.** The frame bus is in `oscope-acquire`, at layer 4, and the HAL at layer 2 (ADR-0005), so a camera can't publish on the bus. A frame's metadata also needs things that a camera doesn't know: the stage's position at the exposure's midpoint, the objective and the lights (ADR-0008).

## Decision

**1. Four traits** in `oscope-hal`, one for each device: `Camera`, `Stage`, `Illumination` and `ObjectiveMount`.
- **Each is `Send + Sync`,** and is held as `Arc<dyn …>`. A driver must be safe to call from several tasks at once. P16's device actors serialise the commands to each device, so a driver needn't queue them itself.
- **Operations that talk to the device are async,** and return `HalFuture<'_, T>`: `Pin<Box<dyn Future<Output = Result<T>> + Send + '_>>`. A driver writes `Box::pin(async move { … })`.
- **What never waits stays synchronous:**
  - every device's `capabilities()`;
  - `Camera::streaming()`;
  - `Stage::track()`;
  - `ObjectiveMount::catalogue()`.

**2. Capabilities.** Each device reports a capability struct, which doesn't change while it is open. The structs are serialisable, with JSON Schemas, for P16's Things.
- **Optional features** are a flag in the capabilities and a trait method whose default fails with `Error::Unsupported`. So far:
  - the software trigger, `Camera::trigger`;
  - the temperature, `Camera::temperature`;
  - homing, `Stage::home`.

  A driver that offers one sets the flag and overrides the method.
- **The checks live with the capabilities:**
  - `CameraCapabilities::check` and `round_roi`;
  - `StageCapabilities::check_target` and `check_speed`;
  - `IlluminationCapabilities::check`.

  Drivers call them, so they all refuse the same things with the same errors. P09b's conformance suite checks that they do.

**3. The camera.**
- **Modes** are a table. Each has:
  - its frame size and binning;
  - its fastest rate for each pixel format it offers;
  - its line time, with a rolling shutter.
- **`RoiRules`** give the steps that an ROI's offsets and sizes are rounded to, its least size, and whether a smaller ROI raises the rate.
- **Triggers:** free-running, software, or external on a rising or falling edge. The capabilities say which the camera offers, and whether it has a trigger output.
- **Settings are one `CameraSettings` value:**
  - the mode, the pixel format, the exposure and the gain;
  - the ROI, in full-sensor coordinates (ADR-0007);
  - the trigger.

  `configure` applies them all, or none, and returns them as the camera applied them, with the ROI rounded. While the camera streams, changing the mode, the format, the ROI or the trigger fails with `Error::Busy`.
- **Frames go to a sink.** `start(pool, sink)` streams until `stop`. For each frame, the camera:
  1. takes a slab from the `FramePool`;
  2. fills it;
  3. calls the `FrameSink`, `Arc<dyn Fn(FrameBuffer, FrameMeta) + Send + Sync>`, on its own thread, in order.

  **The camera fills its own fields:** the ID, format and size, ROI, binning and mode, host time, sensor time if it has one, exposure start and exposure, line time, gain, black level, and `dropped_before`. A frame lost because the pool had no free slab counts in the next frame's `dropped_before`.

  **Acquisition fills the rest** (P16, in `oscope-acquire`):
  - the position, from the stage's `PositionTrack` at the exposure's midpoint;
  - the objective fitted;
  - the lights.

  Then it seals the frame and publishes it on the bus.

**4. The stage.**
- **Positions** are machine coordinates in `Um` (ADR-0007). Speeds are `UmPerS`, a newtype for the one speed unit.
- **The capabilities give:**
  - each axis's soft limits (`Travel`);
  - the fastest speed and the default speed;
  - homing, encoders and a trigger output;
  - the report interval.
- **The operations:**
  - `move_to` and `move_by` finish when the stage is still at the target. A stop that ends one makes it fail with `Error::Stopped`.
  - `jog` returns once the jog has started, and `stop` once the stage is still.
  - Also `home`, `clear_alarm`, `status` and `position`.
- **`StageStatus`** is idle, moving, homing, hold, or alarm with the controller's reason.

**5. The position track.** `PositionTrack` keeps a stage's latest `PositionSample`s in a ring of fixed size. Each sample's time is on OScope's clock, after the report's latency is subtracted.
- `at(t)` interpolates linearly between the samples around `t`. It answers `None` outside them: the track never guesses ahead.
- A sample older than the latest is ignored, and one at the latest's time replaces it.
- It is shared: the driver's thread pushes, and any thread reads.

**6. The lights** are named channels:
- each is transmitted, reflected or other, and dimmable or not;
- their state is `oscope-core`'s `LightState`;
- `set` returns the state as applied: a light that can't dim is at full intensity.

`ManualIllumination` records what the user says they set.

**7. Objectives.**
- **The catalogue** is a TOML file, `configs/objectives.toml`, in micrometres. It holds:
  - the instrument's tube lens;
  - each objective's name, model, magnification, numerical aperture and working distance;
  - the tube lens the objective was designed for (200 mm unless given).
- **Its built-in default,** `ObjectiveCatalogue::mitutoyo()`, is the same as the file, and a test keeps them equal.
- **Rules:** names unique, and the numbers positive. A numerical aperture may be at most 1.6.
- **The magnification on the instrument** scales with the tube lens. A pixel at the sample is the sensor's pixel divided by that magnification.
- **`ManualObjectiveMount`** records which objective the user says is fitted.

**8. Errors.** One `Error` enum, after `oscope-core`'s conventions:
- `Unsupported`, `OutOfRange` (with the value and its limits) and `Unknown` (a mode, light or objective);
- `Busy`, `Stopped` and `Alarm`;
- `Disconnected`, `Timeout` and `Device`;
- `InvalidCatalogue` and `CatalogueFile`.

P16 maps them to HTTP statuses:

| Errors | Status |
|---|---|
| `Unsupported`, `OutOfRange`, `Unknown` | 400 |
| `Busy`, `Stopped`, `Alarm` | 409 |
| `Disconnected` | 503 |
| `Timeout` | 504 |
| `Device` | 502 |

### The four targets

**The MIChrome 20** (M3, `oscope-tucsen`):
- **Its capabilities:**
  - a rolling shutter;
  - modes `full`, `bin2` and `bin3`, with `Bayer8` and `Bayer16` at their measured rates: 14.49 and 7.24 fps at full resolution;
  - ROI steps of 2 in x and 1 in y, with no rise in rate;
  - no trigger input or output, sensor timestamps or temperature. The SDK's software-trigger mode is checked at H0 and offered if it works.
- **What the driver does:**
  - It loads the SDK at run time, and sets every setting itself, overriding the SDK's remembered ones.
  - It waits for frames on its own thread. It fills pool slabs, or attaches them as the SDK's buffers, which the SDK allows (P02b), and calls the sink.
  - With no sensor timestamps, it estimates each exposure's start from the host time, the exposure and the readout. It counts frames lost from the gaps between them, since the frame index stays 0.
  - It may state a y step of 2 to keep the mosaic RGGB. If it doesn't, the frame's ROI tells acquisition the pattern (ADR-0008).

**The grblHAL stage** (M3, `oscope-grbl`):
- **Its capabilities come from `$$`:**
  - the travel from `$130`–`$132`, and the fastest speed from `$110`–`$112`;
  - homing from `$22`;
  - encoders and the trigger output as found at H0.
- **Status reports** (`?`), at the report interval, feed the track. Each is timestamped on arrival, less its latency.
- **The states map:**
  - Idle to idle; Run and Jog to moving; Home to homing; Hold to hold; Alarm to alarm;
  - Door and Tool (a tool change) to hold;
  - Check and Sleep to alarm, with their names as the reason.
- **The commands:**
  - `move_to` sends a move and waits for idle;
  - `jog` uses `$J=`, and `stop` a jog cancel or a feed hold;
  - `home` sends `$H`, and `clear_alarm` sends `$X`.
- **Units:** millimetres become micrometres at the driver's edge.

**A camera with a trigger input,** the MIChrome 20's successor:
- **Its capabilities** set `external`, and `output` if it has one. It is set to `Trigger::External { edge }`.
- **The pulses** come from the stage's trigger output, which `StageCapabilities::trigger_output` reports. Programming that output (pulses at positions) is P11's addition to `Stage`. It follows this ADR's pattern: a default that fails with `Unsupported`, behind the existing flag.
- **Sensor timestamps,** if it has them, fill `FrameMeta::sensor_time`, and `sensor_timestamps` says so.

**Another vendor's camera, as an extension** (P27):
- **The extension implements the camera** through the extension SDK. On the server's side, an adapter implements `Camera` over it, so the rest of the server can't tell it from a built-in driver.
- **Capabilities and settings** are serialisable, so they cross the extension ABI as JSON.
- **A Python driver's frames** arrive through shared memory, and its adapter turns them into sink calls.
- **P09b's conformance suite** runs against the adapter, as against any driver.

## Consequences

- **Gains:**
  - One seam for the simulator, the real drivers and extensions. The configuration picks among them, and one conformance suite (P09b) checks them all.
  - Optional features need neither a trait for each kind of camera nor downcasting. The capabilities say what a device offers, and P16 shows the user only that.
  - Shared checks give the same errors from every driver.
  - A camera never needs to know about the stage, the objective or the lights. The frame's metadata is completed in one place.
- **Costs:**
  - Every device call allocates its future: negligible at the rate of commands. Frames don't go through futures but through the sink.
  - Drivers write `Box::pin(async move { … })` in each method.
  - Acquisition can seal a frame only once the stage has reported after the frame's exposure midpoint, about one report interval plus its latency later. P16 bounds that wait. With no report in time, the frame's position stays `None`.
  - `configure` takes every setting at once, so a driver applies them all, or works out what changed.
- **To watch:**
  - The MIChrome 20's line times are estimates (`hardware.md`) until they are measured.
  - P11 adds the trigger output's programming to `Stage`.

## Alternatives considered

- **`async fn` in the traits:** they can't be trait objects on Rust 1.98.1 (E0038). Every user would be generic over its driver, and drivers couldn't be chosen by configuration or loaded from extensions.
- **The `async-trait` crate:** it does the same boxing behind a macro, at the cost of a new dependency and signatures that the code doesn't show.
- **Synchronous traits, on device threads:** a good fit for blocking SDKs. But the simulator's clock-driven waits (P11) and the async server would then bridge back to async everywhere. A blocking driver can still keep a thread of its own behind an async trait.
- **A trait for each capability** (`TriggeredCamera`, `HomingStage`, and so on): the combinations multiply, and using one means downcasting a trait object.
- **A getter and a setter for each camera setting:** no all-or-none change, a frame size changed in several steps, and no single answer for what the camera rounded. One settings value matches SDKs that take settings before streaming starts.
- **The camera publishing on the bus:** the bus is two layers up (ADR-0005), and the camera would need the stage, the objective and the lights.
- **A track that extrapolates past the latest sample:** it guesses wrong at the end of a move, where the stage slows down. Waiting for the next report gives the measured position.
- **Millimetres in the catalogue file:** friendlier numbers, but ADR-0007 puts lengths at every interface in micrometres. The file's header says so, and gives 200 mm as 200000.
