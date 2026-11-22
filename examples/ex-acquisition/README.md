# ex-acquisition

A hardware-free checkpoint: seeded H&E tissue, a quarter-size simulated MIChrome
20 and stationary stage feed acquisition metadata and the lossless/latest-frame
bus. It checks ordered frames, byte layout, timestamps, exposure-midpoint
position, objective, lights and returned frame slabs after cancellation.

Run inside the development container:

```sh
cargo run -p ex-acquisition --locked -- --smoke
cargo run -p ex-acquisition --locked -- --require-cuda --smoke
cargo run -p ex-acquisition --no-default-features --locked -- --smoke
```

The normal build prefers CUDA and allows CPU fallback. `--require-cuda` fails
without CUDA and checks the camera stays on the GPU throughout capture. The
explicit CPU-only build excludes CUDA. Smoke captures six 1368 × 912 frames;
the default captures thirty. The eight raw frame slabs take about 20 MB.
No files or physical device connections are created. See the
[R04 manual procedure](../../docs/phases/r04-devices-acquisition.md).
