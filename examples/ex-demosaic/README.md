# ex-demosaic

The colour pipeline on a 20 MP test chart (P08, P08b, P08c). The example makes a chart of known colours, and mosaics it as the MIChrome 20's RGGB sensor would see it, in 16 bits. Then each demosaicing method turns the raw frame back into RGB:

- **superpixel:** each 2 × 2 cell becomes one pixel, at half the width and height;
- **bilinear:** each missing colour is the mean of its neighbours;
- **Malvar–He–Cutler:** bilinear, corrected by the gradient of the pixel's own colour.

**For each method it measures:**

- **its time on the CPU,** in a live pool of 8 threads, against §5.20's budget of 80 ms;
- **its time on the GPU,** with the `cuda` feature and a GPU, against 10 ms, from the raw frame in the pool's pinned memory. It also checks that the GPU's result equals the CPU's to the bit;
- **its PSNR** against the chart's true colours.

**Then it serves a page** comparing the methods side by side, with:

- the times and the PSNR;
- a 512 × 512 region of the chart's finest rings at full resolution, with its 16-bit TIFF to download;
- the whole chart, binned by 4.

## How to try it

```sh
cargo run --release -p ex-demosaic --features cuda     # with the GPU: needs the CUDA 13.4 toolkit
cargo run --release -p ex-demosaic                     # the CPU only
```

Use `--release`, since a debug build is several times slower. On the owner's PC it prints this, then `ex-demosaic: open http://127.0.0.1:8214/`:

```text
a 5472 × 3648 chart as RGGB16; the median of 10 runs, from the raw frame to RGB:
  method                    CPU          GPU     PSNR  GPU against CPU
  superpixel            11.9 ms       3.3 ms  21.8 dB  to the bit
  bilinear              27.5 ms       5.2 ms  21.3 dB  to the bit
  Malvar–He–Cutler      29.8 ms       5.7 ms  29.9 dB  to the bit
```

Open the address in a browser. You should see:

- **the table,** each time marked ✓ within its budget;
- **the rings:**
  - bilinear leaves blue and orange fringes on the grey rings, and Malvar–He–Cutler stays grey;
  - superpixel's rings are half the size, and alias;
- **the whole chart:** 24 patches, the gradient, and the rings, whose moiré comes from binning by 4.

Ctrl-C stops the server.

**The files** are written to `.oscope/ex-demosaic/data/page/`:

- `{method}.png` and `{method}-rings.png`;
- `{method}-rings.tif`, at 16 bits per sample;
- `results.json`.

**Options:**

- `--runs N`: how many runs make each median;
- `--threads N`: the CPU pool's threads, 8 for the budget;
- `--port N`: another port.

**`--smoke`** runs everything three times, on a port the system picks, with a temporary data root. Then it checks the page:

- the page, its script, an image, a TIFF and `results.json` are served;
- each method meets its PSNR mark;
- each CUDA twin that ran matched the CPU.

It doesn't check the times, which a debug build misses.

```sh
cargo run -p ex-demosaic -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.

For criterion's statistics, run `cargo bench -p oscope-imaging --bench demosaic`.
