# ex-checkpoint-e

Checkpoint E: the third dimension, from one server, through the API.

1. **The reflective chip** (P42): a z-scan of a die's dome at 50× in reflected light, 21 planes 0.5 µm apart. The job `volume.depth_from_focus` makes its heights, their confidence, its all-in-focus image and a mesh.
2. **The fluorescent cells** (P43): loaded through `simulation/load`, then a widefield z-scan at the coverslip's middle, lit by the blue line. The job `volume.deconvolve` deconvolves it by Born and Wolf's PSF.
3. **The page** shows both in VTK.js, side by side. It holds `ex-depth-from-focus`'s page, with the heights as a surface textured with the all-in-focus image, and `ex-deconvolve`'s, with the cells as a volume. The server serves them under `/depth/` and `/deconvolve/`.

## How to try it

```sh
cargo run --release -p ex-checkpoint-e
```

It serves at http://127.0.0.1:8249/. On the owner's PC, at real time, it printed:

```text
ex-checkpoint-e: the chip's dome in heights and the cells deconvolved, in 85.1 s
  open http://127.0.0.1:8249/ to see both drawn by VTK.js
```

- **Each half of the page** has its own controls, as its example's page has.
- **For the errors against the truth,** run the two examples themselves: [`ex-depth-from-focus`](../ex-depth-from-focus/README.md) and [`ex-deconvolve`](../ex-deconvolve/README.md).

## `--smoke`

```sh
cargo run -p ex-checkpoint-e -- --smoke
```

It runs the same at ten times real time, over the frame's middle ninth, with a temporary data root. It checks:

- the four results made: the heights, their confidence, the all-in-focus image, and the deconvolved volume;
- the heights' and the deconvolved volume's metadata over HTTP;
- the page, both examples' pages and their scripts, and VTK.js served.

It ends with `smoke: ok`, or says what failed and exits with code 1.
