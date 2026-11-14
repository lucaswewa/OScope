# ex-register

P31's registration on fields of the simulated H&E slide at 10×: the shift between two images, by phase correlation, to a hundredth of a pixel, on the CPU and, with `cuda`, the GPU.

Each case renders field A at the middle of the tissue's first island, and field B with its centre moved by a known number of pixels, so its content moves the other way. It registers B against A with `oscope_imaging::registration`, then checks the shift within 0.1 px and the confidence above 0.5, and, with the GPU, that the GPU's shift is within 0.01 px of the CPU's.

| Case | What it shows |
|---|---|
| A sub-pixel shift | B's field moved by (3.3, −1.7) px |
| A whole one | moved by (−12, 5) px |
| A large shift, everywhere | moved by 35 % of the field across and 20 % down, with every shift considered: the overlap pass takes away the lean that the window and the shrinking overlap give a large shift |
| The same, in a window of ±16 px | the shift a stage would expect, off by (3, −2) px, and only the shifts within 16 px of it considered: the confidence is higher, as nothing far away competes |

Each row shows the shift expected and found, the confidence and the peak, and the CPU's and the GPU's times: the medians of 3 and 11 runs.

## How to try it

```sh
cargo run --release -p ex-register
cargo run --release -p ex-register --features cuda               # with the CUDA toolkit
cargo run --release -p ex-register --features cuda -- --bench    # and a 2048² pair
```

`--bench` adds a 2048² pair, timed against §5.20's budget of 15 ms on the GPU, from the images in memory to the registration. It doesn't serve anything, so it needs no port.

On the owner's PC (i9-12900, 24 threads; RTX A2000 12 GB) it prints:

```text
fields of the simulated H&E slide at 10× (0.24 µm pixels), 512 × 512; the GPU: yes
                                   expected              found                 confidence   peak        CPU        GPU
a sub-pixel shift                  (  -3.300,    1.700)  (  -3.300,    1.704)       0.996  0.974    20.8 ms     2.4 ms
a whole one                        (  12.000,   -5.000)  (  12.000,   -5.000)       0.993  0.982    18.6 ms     2.6 ms
a large shift, everywhere          (-179.600,  102.700)  (-179.601,  102.700)       0.729  0.399    15.3 ms     1.9 ms
the same, in a window of ±16 px    (-179.600,  102.700)  (-179.601,  102.700)       0.970  0.399    14.4 ms     1.4 ms

a 2048² pair, §5.20's budget on the GPU 15 ms:
a sub-pixel shift, 2048²           ( -37.250,   18.500)  ( -37.249,   18.503)       0.995  0.951   325.4 ms    10.9 ms
```

- **The GPU's 10.9 ms** for a 2048² pair is mostly copies: 2.7 ms up for the two images and 2.9 ms down for the whole surface, as every shift is considered. The transforms take about 2 ms.
- **The CPU** registers the 2048² pair in 0.33 s, with all 24 threads.

## `--smoke`

```sh
cargo run -p ex-register -- --smoke
```

It runs the four cases on 256 × 256 fields, with the same checks, and ends with `smoke: ok`, or says what failed and exits with code 1.
