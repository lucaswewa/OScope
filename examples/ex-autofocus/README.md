# ex-autofocus

P34's autofocus, auto exposure and auto white balance, on the simulator, all through the API, as a client would.

For each of the simulator's samples, it:

1. loads the sample, and turns on its light: transmitted for the H&E tissue, reflected for the reflective chip, blue (488 nm) for the fluorescent cells;
2. asks the simulation Thing where the sample is sharpest (`in_focus`), and moves the stage 15 µm off;
3. runs the `autofocus` Thing's job `run`, stepped, and for the H&E once more continuously;
4. prints where the stage was left against the truth, in µm and in quarters of the depth of field, with the frames measured and the time taken.

Then it tries clear glass beside the coverslip, which autofocus refuses as an empty field. There it sets the exposure (the `exposure` Thing's `auto`) and the white balance (`white_balance`), and gives the gains to the preview's look.

## How to try it

```sh
cargo run --release -p ex-autofocus
```

It serves at http://127.0.0.1:8237/. Its page plots the latest run's focus curve: each frame's measure against the stage's z, the coarse sweep grey, the fine sweep red, and where the stage was left blue. It updates as each run ends. On the owner's PC, at real time through a 1024 × 768 region of the 20 MP sensor's middle, it printed:

```text
sample             sweep         truth µm   found µm    error   ÷ d/4  frames     time
H&E tissue         stepped       5002.000   5002.131   +0.131    0.07 22   11.8 s
H&E tissue         continuous    5002.000   5001.836   -0.164   -0.08 33   16.0 s
reflective chip    stepped       5002.761   5003.660   +0.899    0.46 22   15.6 s
fluorescent cells  stepped       5001.747   5000.529   -1.218   -0.65 22   15.5 s

clear glass: the field is empty: its sharpest frame measures under 1.2 times the median. Move to the sample
auto exposure: 15.0 ms, the 99.5th percentile at 0.755 of full scale, in 1 steps
white balance: gains [1.0001708380997485,1.0,1.0000551490710217], given to the preview
```

- **A region of the sensor.** The simulator renders each new depth of a frame afresh, on the CPU. Over the whole 20 MP, a stepped sweep took 212 s, and the fluorescent cells' frames took longer than the 30 s each frame is waited for. A camera gives its frames at its own rate, wherever the stage is.
- **Each light has its measure:** Brenner's gradient in transmitted light, Tenengrad in reflected light, and the variance of the Laplacian in fluorescence. The `autofocus` Thing's `defaults` say which, with the depth of field that sets the steps.
- **The stepped sweep** takes 22 frames: 13 coarse, a depth of field apart, then 9 fine, a quarter of one apart. Each is reached moving up, so the stage's backlash shifts them all alike.
- **The continuous sweep** moves the stage at a constant speed, and puts each frame at the z where the stage was at its exposure's midpoint. The simulator renders each new depth afresh, so its frames come more slowly while the stage moves than a camera's would. Where they lie sparse about the best one, the run finishes with the stepped sweep's fine pass.
- **The cells' truth** is their dye's height, weighted by how much each depth holds: they lie thicker near the glass.
- **White balance** looks at the region's lightest part, the light itself in brightfield. Over tissue with no glass in view, its gains would be the tissue's colour.

## `--smoke`

```sh
cargo run -p ex-autofocus -- --smoke
```

It runs the same on the quarter sensor at ten times real time, through a 256 × 192 region of the sensor's middle, quick to render in a debug build, with a temporary data root. It holds each run within a quarter of the depth of field of the truth, the glass to a refusal, and the exposure and white balance to jobs that complete. It ends with `smoke: ok`, or says what failed and exits with code 1.
