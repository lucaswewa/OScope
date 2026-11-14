# ext-rust-camera

A sample Rust extension for OScope (P27, [ADR-0036](../../docs/adr/0036-extension-drivers-and-panels.md)): a camera driver, `example.fakecam`, which streams a test pattern of diagonal stripes that move with each frame. It is what a third party writes: a library crate built against [`oscope-ext-sdk`](../../crates/oscope-ext-sdk) alone, with no `unsafe`.

It isn't a program. [`ex-ext-camera`](../ex-ext-camera/README.md) packs it, installs it, and runs the server with it in place of the simulator.

## The camera

- **A 640 × 480 sensor** of 3.45 µm pixels, with a global shutter.
- **Modes:** `full`, at 60 frames a second in 8 bits of grey or 30 in 16, and `bin2`, 320 × 240, at twice that.
- **ROIs** in steps of 2, of at least 16 pixels; exposures from 0.01 ms to 1 s; gain from 0 to 24; a software trigger.

## How it is made

[`src/lib.rs`](src/lib.rs) implements `CameraDriver`:

- **`capabilities`:** what it can do, in the HAL's types;
- **`configure` and `start`:** it keeps the settings OScope has checked;
- **`capture`:** it waits until the next frame is due, from its mode's rate or its exposure if longer, then fills OScope's buffer with the pattern, for the settings' format, ROI and binning;
- **`stop`.**

`oscope_ext_sdk::export!(nodes: [], drivers: [oscope_ext_sdk::camera("example.fakecam", FakeCam::default())])` makes the C ABI for it.

OScope does the rest of what its HAL promises: it checks settings against the capabilities, refuses what can't change while the camera streams, waits for software triggers, and times, numbers and pools frames. So the driver passes the HAL's conformance suite, as `ex-ext-camera`'s test shows.

[`manifest.toml`](manifest.toml) provides the driver, `drivers = ["example.fakecam"]`, which a server's configuration names: `camera = "example.fakecam"` in `[devices]`.
