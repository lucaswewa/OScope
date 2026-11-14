# ex-ext-camera

The server with an extension's camera instead of the simulator (P27, [ADR-0036](../../docs/adr/0036-extension-drivers-and-panels.md)): the sample [`ext-rust-camera`](../ext-rust-camera/README.md) packed, installed and loaded at a start, its driver named by the devices' configuration, and its frames in the live view.

## How to try it

```sh
cargo run -p ex-ext-camera
```

It packs the sample for this platform and installs it in `.oscope/ex-ext-camera/`. Then it starts as the server does: the extensions first, then the devices, which name the extension's driver, as a configuration's `[devices]` would:

```toml
[devices]
camera = "example.fakecam"
stage = "fake"
illumination = "fake"
objective = "fake"
```

Then it starts the camera, and serves [`ex-live-view`](../ex-live-view/README.md)'s page on `http://127.0.0.1:8231/`: the live view of the extension's frames, diagonal stripes moving down and to the right. The camera Thing is the extension's (`curl http://127.0.0.1:8231/api/v1/camera/capabilities` names *OScope's example fake camera*), so its settings and modes work as any camera's.

**In the server:** install the package as in [`ex-omext`](../ex-omext/README.md), name its driver in the configuration's `[devices]`, and start the server again.

**`--smoke`** checks that the camera Thing is the extension's, starts it, and receives three previews of 320 × 240.

**Its test,** `tests/conformance.rs`, loads the camera from the packed library, as a start does, and runs the HAL's conformance suite against it: it passes.
