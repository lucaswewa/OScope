# ex-sim-samples

The simulator's sample catalogue: listing samples, loading and unloading them, and importing your own image (P14).

**The catalogue** offers five built-in samples, each with ground truth:

- the H&E tissue (P10);
- the test target (P10);
- the reflective surface (P13);
- the fluorescent cells (P13);
- the sub-diffraction structures: filaments 25 nm across, and hollow tubules like mitochondria (P14).

A configuration adds more, as `[[samples]]`: images (PNG, JPEG or TIFF) and stacks (multi-page TIFF), each with its scale. Imports have no ground truth beyond themselves.

**The slide loader** puts a sample on the stage, in place of any other, and takes it off, leaving the stage empty.

**The example:**

- **It writes two files to import** into its folder:
  - `demo.png`: 400 × 300 pixels of grey squares, with a dark ring;
  - `demo-stack.tif`: three pages of a bright disc that moves along.

  It also writes a configuration, `samples.toml`, that lists them as `your image`, at 0.5 µm a pixel, and `a fluorescence stack`. It reads the configuration back into the catalogue.
- **It loads each sample in turn,** and renders it at 5× and at 50× in its own light:
  - brightfield for the tissue, the target and the image;
  - reflected light for the surface;
  - widefield fluorescence for the cells, the structures and the stack.
- **It unloads the slide,** and renders the empty stage.

## How to try it

```sh
cargo run --release -p ex-sim-samples
```

On the owner's PC it prints:

```text
the samples in .oscope\ex-sim-samples\samples.toml:
  name                         kind         truth    mean 5×  mean 50×     time
  H&E tissue                   tissue       yes       0.6236    0.6426   216 ms
  test target                  test-target  yes       0.5250    0.5463   138 ms
  reflective surface           surface      yes       0.4061    0.3880   443 ms
  fluorescent cells            cells        yes       0.0052    0.0023   945 ms
  sub-diffraction structures   structures   yes       0.0030    0.0027   170 ms
  your image                   image        no        0.7318    0.5234    56 ms
  a fluorescence stack         stack        no        0.0017    0.1148    67 ms
your image: 200 × 150 µm, its middle at (47000, 12500) µm on the slide
unloaded "your image": the stage is empty, uniformly 1 in transmitted light
the images are in .oscope\ex-sim-samples
```

**The images** are in `.oscope/ex-sim-samples/`, two for each sample (`{name}-5x.png`, `{name}-50x.png`), and `empty-stage.png`. Open them to see:

- **the structures at 50×:** filaments blurred to a fifth of a micrometre, crossing, and short tubules whose hollows the optics can't show. At 5×, a dense network;
- **your image at 5×:** 200 × 150 µm in the middle of the field, its ring centred. At 50×, a part of it, its pixels blocky at 0.5 µm;
- **the stack at 50×:** the page in focus, sharp, over the others' blur.

**Your own image:**

```sh
cargo run --release -p ex-sim-samples -- --image path/to/your.png --um-per-pixel 0.25
```

It's imported as a brightfield image, centred on the coverslip. It isn't committed anywhere: the configuration lists it by its path.

**`--smoke`** runs at 192 × 128, in a temporary folder, then checks:

- that the catalogue holds seven samples, the five built-in ones with ground truth;
- that every sample renders something other than an empty stage;
- that the empty stage is uniformly 1 once unloaded;
- that the image landed 200 µm across, centred on the coverslip.

```sh
cargo run -p ex-sim-samples -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
