# ADR-0007: Units, coordinates and the clock

- Status: Accepted
- Date: 2026-11-11
- Phase: P02

## Context

- **Every crate passes lengths, positions and times,** and each device speaks its own units:
  - grblHAL works in millimetres;
  - camera SDKs in microseconds, or in lines;
  - images in pixels;
  - optics in nanometres.

  Mixed units are an old source of quiet errors.
- **Frames and stage positions must be matched in time.**
  - A frame's position is interpolated from the stage's position track at its exposure's midpoint (plan §5.5).
  - Live stitching, autofocus sweeps and calibration all depend on that match.
- **The simulator runs time at other rates** (D30): real time, scaled, or stepped by tests. When it is in use, the rest of the server must take its time from the simulation clock too (§5.10).
- **Image and stage coordinates meet** in calibration, in click-to-move and in stitching. All three need one convention for where a pixel is, and which way the axes run.

## Decision

**1. Units.**

- **Every length and position at an API is in micrometres**, as `Um`, an `f64` newtype: stage positions, distances, pixel sizes, wavelengths.
- **Every duration a user sets or reads is in milliseconds**, as `Ms`, an `f64` newtype: exposures, intervals, timeouts. Inside the code, timing arithmetic uses `std::time::Duration`.
- **Conversions happen at the edges.** A driver converts to and from its device's units (millimetres for grblHAL, whatever the camera SDK uses), and nothing past the driver sees them.
- **In JSON,** both are plain numbers in their unit. Their JSON Schemas say which unit, from the types' documentation.
- **Pixels** are plain numbers, in image coordinates (below).

**2. Coordinates.**

- **Image coordinates** are in pixels.
  - x runs right and y runs down, from the top-left corner of the top-left pixel.
  - Pixel `(i, j)` covers `[i, i + 1) × [j, j + 1)`, so its centre is `(i + 0.5, j + 0.5)`.
  - Unless an API says otherwise, they are the full sensor's coordinates at full resolution, even for a frame with an ROI or binning. A frame's metadata (P03) says how its pixels map to the sensor's.
- **Rectangles** are half-open: they hold their edges of least x and y, not the others, so tiles that touch don't share points.
- **Stage coordinates** are x, y and z in micrometres, the controller's machine coordinates after homing.
  - The signs are the controller's own, and z is the focus axis.
  - Which way each axis moves the sample is recorded for the real stage in P02b (`docs/hardware.md`); the simulator sets its own.
- **Image to stage** is an `Affine2`, from image pixels to stage micrometres. The camera-to-stage calibration (P32) fits it and defines its reference point.

**3. The clock.**

- **One `Clock` per process,** passed as `Arc<dyn Clock>` to whatever needs time. No code takes timestamps from `Instant::now()` or `SystemTime::now()` directly, so a simulation clock or a fake can stand in everywhere.
- **A `Timestamp` is nanoseconds since the clock started,** in a `u64`: 584 years of range. It never goes back.
- **UTC is mapped once.** The clock records the UTC time of its start, and the UTC time of any timestamp is that start plus the timestamp.
  - Changes to the host's wall clock after start (NTP, or a user setting it) don't move timestamps.
  - Over a long run, the UTC times shown can therefore drift from the wall clock, by as much as NTP corrects it.
- **Three implementations:**
  - `SystemClock`, on the host's monotonic clock;
  - `FakeClock`, for tests: it moves only when told to;
  - the simulation clock (P11), at 1×, scaled or stepped.
- **Timestamps from devices,** such as the sensor's own or the stage's reports, are converted to the clock when they arrive, after subtracting an estimated latency (§5.5).
- **Waiting on the clock is left out for now.** Sleeps and timeouts that follow a scaled or stepped clock need async, and `oscope-core` has none. P11 adds them, in ADR-0013 or a successor of this one.

## Consequences

- **Gains:**
  - A length can't be passed for a duration, nor millimetres for micrometres: the types don't match.
  - Numbers stay readable at a microscope's scale. A pixel is 0.05–2.4 µm, a field of view a few millimetres, and the travel 100 mm.
  - Every timestamp in a process is comparable, and tests and the simulator control time completely.
  - Calibration, click-to-move and stitching share one meaning for a pixel's position.
- **Costs:**
  - `f64` lengths aren't exact. At 100 mm they still resolve 10⁻¹¹ µm, far below the stage's own resolution, but tests compare with tolerances.
  - Code that wants time must be handed a clock, rather than call `Instant::now()`.
  - UTC times shown after a long run can be off from the wall clock by NTP's corrections.

## Alternatives considered

- **Integer nanometres (`i64`) for lengths:** exact, but every computation converts back and forth, and an affine map or a fit needs floating point anyway.
- **Millimetres, grblHAL's unit:** pixel sizes would read as 0.00024 mm. Micrometres keep the common numbers between 0.01 and 100,000.
- **A units library** (`uom`): units checked at the type level for every quantity, but with heavy generic signatures, slower builds, and a new dependency, for the two units OScope uses everywhere.
- **Wall-clock timestamps (`DateTime<Utc>`) everywhere:** they jump when the host's clock is set, and can't run at a simulated rate.
- **`std::time::Instant` directly:** it can't be serialised, faked, or driven by the simulator.
- **A global clock:** easy to reach, but tests running side by side couldn't each have their own.
- **y up in images:** it matches the stage, but no image format, library or screen works that way. The calibration's flip handles the difference instead.
