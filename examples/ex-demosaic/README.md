# ex-demosaic

Demosaicing on the CPU (P08), and its images as files (P08b). It makes a 20 MP test chart of known colours, and mosaics it as the MIChrome 20's RGGB sensor would see it, in 16 bits. Then each method turns the raw frame back into RGB.

**The methods:**

- **superpixel:** each 2 × 2 cell becomes one pixel, at half the width and height;
- **bilinear:** each missing colour is the mean of its neighbours;
- **Malvar–He–Cutler:** bilinear, corrected by the gradient of the pixel's own colour.

**For each method it reports:**

- **its time,** in a live pool of 8 threads, against §5.20's budget of 80 ms;
- **its PSNR** against the chart's true colours.

**Then it writes, for each method,** in `.oscope/ex-demosaic/`:

- **`{method}.png`:** the whole image, binned by 4, through the sRGB display transform;
- **`{method}-rings.png` and `{method}-rings.tif`:** a 512 × 512 region of the fine rings, demosaiced at full resolution on its own. The TIFF has 16 bits per sample.

P08c adds the CUDA twins, and a page that shows the images side by side.

## How to try it

```sh
cargo run --release -p ex-demosaic
cargo run --release -p ex-demosaic -- --threads 24 --runs 20
```

Use `--release`, since a debug build is several times slower. On the owner's PC it prints:

```text
a 5472 × 3648 test chart, mosaicked as RGGB16, demosaiced in a live pool of 8 threads;
the median of 10 runs, from the raw frame to RGB:

  method                time  within 80 ms PSNR
  superpixel         12.8 ms  yes          21.8 dB, 2736×1824
  bilinear           27.9 ms  yes          21.3 dB, 5472×3648
  Malvar–He–Cutler   29.8 ms  yes          29.9 dB, 5472×3648
```

**The chart** has three parts:

- **24 colour patches;**
- **a smooth gradient,** which every method rebuilds;
- **a zone plate of fine rings.** No method can rebuild them, since red and blue are sampled only every other pixel. It is where Malvar–He–Cutler gains most.

**Superpixel's PSNR** is measured against the chart averaged over 2 × 2 cells, since its image is half size.

**The files.** Open the rings' PNGs side by side: bilinear leaves colour fringes on the grey rings, and Malvar–He–Cutler few. Superpixel's rings are half the size, and alias. The whole images show the chart: 24 patches, the gradient, and the rings, whose moiré comes from binning by 4.

**`--smoke`** runs each method three times, and writes only the rings' files, to a temporary folder. It checks that each method meets its PSNR mark, and that the six files were written. It doesn't check the times, which a debug build misses:

```sh
cargo run -p ex-demosaic -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.

For criterion's statistics, run `cargo bench -p oscope-imaging --bench demosaic`.
