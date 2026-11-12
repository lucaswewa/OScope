# P08b: Colour, display transform, crop and binning, writers

- Status: In review
- Branch / PR: `m1/p08b-colour` / [#17](https://github.com/lucaswewa/OScope/pull/17)
- Size: L · estimated ~800 when P08 was split, and ~735 from the work itself just before starting · measured **586** hand-written lines added (code 332, tests 187, examples 61, glue 6), and 8 removed. Docs and `Cargo.lock` aren't counted. **Within L's 900.**

## Summary

Demosaiced images can now be corrected, shown on an 8-bit display, cropped and binned, and saved ([spec](../phases.md#p08b)). The colour module now runs in this order:

1. **the black level,** on the mosaic or on RGB;
2. **demosaicing,** of the whole mosaic or of a region, binned;
3. **white-balance gains, then a 3 × 3 colour matrix;**
4. **the display transform:** black and white points, then a linear, gamma or sRGB curve, from 16 bits to 8 through a table;
5. **files:** PNG for viewing, and 16-bit TIFF.

**`ex-demosaic` now also writes, for each method:**
- a PNG of the whole image, binned by 4;
- a 512 × 512 region of the fine rings at full resolution, as PNG and 16-bit TIFF.

## What was built

| Where | What |
|---|---|
| `oscope-imaging::color` (`tone`) | **`subtract_black(image, black)`:** `(v − black) / (1 − black)`, and 0 below black. **`ColourCorrection`** (gains, matrix; the default changes nothing) and **`correct(rgb, correction)`**, unclamped. **`Curve`** (`Linear`, `Gamma { gamma }`, `Srgb`), **`Display`** (black, white, curve; by default 0, 1, sRGB) and **`to_display(image, display)`**, which gives the 8-bit samples |
| `oscope-imaging::color` (`region`) | **`crop(image, rect)`.** **`demosaic_region(mosaic, pattern, method, rect, bin)`:** the rectangle, in the RGB image's pixels (half of them for superpixel), demosaiced with a 2-pixel margin, then binned by averaging |
| `oscope-imaging::color` (`files`) | **`encode_png(writer, rgb8, width, height)`** with `png` 0.18. **`encode_tiff16(writer, rgb)`:** baseline TIFF, written here: RGB, 16 bits, uncompressed, little-endian, one strip, 72 dpi |
| `oscope-imaging::color` | **`ColorError`** gains `Region`, `Setting` and `File`. The module's documentation gives the order of the steps |
| `examples/ex-demosaic` | The files above, in `.oscope/ex-demosaic/`, or a temporary folder with `--smoke` |

**Dependencies:**
- **New:** `png` 0.18.1, and `tiff` 0.11.3 for the tests only, without its codecs. With them came 11 packages, all at least two weeks old, under MIT, Apache-2.0, Zlib or 0BSD.
- **Pinned back:** `crc32fast` (1.5.3) and `flate2` (1.1.10), whose newest versions were 2 and 6 days old.

## How to try it

```sh
cargo run --release -p ex-demosaic            # then look in .oscope/ex-demosaic/
cargo run -p ex-demosaic -- --smoke
cargo test -p oscope-imaging --test colour    # 5 tests
```

The rings' crops show the methods' differences best. Bilinear leaves colour fringes on the grey rings, and Malvar–He–Cutler few.

## Performance

The display transform's table has 65,536 entries and is built for each call. That costs little next to a 20 MP image's 60 million lookups. P08c's CUDA twin and the preview (P17) will keep it.

## Design notes and deviations from the plan

- **The black level comes first, on the mosaic,** as cameras subtract it, and as P02b found the MIChrome 20 needs: a black offset of about 2,000. It also works on RGB.
- **White balance comes before the matrix:** `matrix × (gains ⊙ rgb)`. The result isn't clamped, since the display transform clamps.
- **The display transform goes through 16 bits,** as the spec's "16 → 8 bits" says. Each sample is rounded to one of 65,536 levels, then looked up.
  - **Why:** the curve is computed 65,536 times, rather than once per sample.
  - **The cost:** a level that falls exactly halfway between two 8-bit values may round down. A test found that, at a midpoint, and the test now uses a value away from it.
- **A region is demosaiced with a 2-pixel margin,** which Malvar–He–Cutler's 5 × 5 filters need. It gives exactly the same pixels as demosaicing everything, then cropping. At the frame's edges the margin is the same mirror.
- **A region is given in the output image's pixels,** so a superpixel region is in half pixels, and maps to whole 2 × 2 cells.
- **Binning averages,** like `cpu::downsample`, so the result stays in 0–1 for display. P06's `bin2x2` sums, as a sensor does.
- **The TIFF writer is written here,** about 70 lines, and the `tiff` crate only reads the files back in the tests. That keeps runtime dependencies to `png`, and the test reads them with a decoder that isn't ours.
- **The example writes small files.** Whole 20 MP TIFFs would be 120 MB each, so it writes the whole image only as a PNG binned by 4, and the full-resolution TIFF only of the rings.

## Tests

- **`oscope-imaging/tests/colour.rs`,** 5 tests:
  - **the black level:** below black is 0, the rest is stretched, white stays 1, and a level out of 0–1 or NaN is refused;
  - **white balance and the matrix:** applied in that order, to a known pixel. The default changes nothing, and a mosaic is refused;
  - **the display transform:**
    - sRGB at 0, 0.5 and 1 (0, 188, 255), and on its linear toe;
    - linear and gamma 2.2 at 0.5;
    - black and white points;
    - every 16-bit level, in order, under each curve, from 0 to 255;
    - out-of-range samples, and white not above black refused;
  - **regions:**
    - 120 seeded rectangles, edges and corners included, for every pattern and method, binned by 1 to 4. Each equals demosaicing everything, then cropping and binning, to the bit;
    - empty and outside rectangles are refused;
  - **files:**
    - the PNG decoded by `png` gives back the display's bytes, size, colour type and depth;
    - the TIFF decoded by `tiff` gives back the size, RGB at 16 bits, and every sample;
    - wrong sizes and channels are refused.
- **Mutation checks,** each restored afterwards:

  | Broken on purpose | Failed |
  |---|---|
  | The black level not stretched | `the_black_level_is_subtracted_and_the_rest_stretched` |
  | The gains left out | `white_balance_and_the_matrix_apply_in_that_order` |
  | sRGB's toe at the wrong slope | `the_display_transform_follows_its_curves_and_points` |
  | The black point ignored | the same |
  | A margin too small for Malvar's filters | `a_region_with_binning_matches_demosaicing_all_then_cropping_and_binning` |
  | Superpixel's region at half its offset | the same |
  | The TIFF said to be grey | `png_and_16_bit_tiff_files_read_back` |
  | TIFF samples truncated, not rounded | the same |

- **The images** were looked at: the whole chart, with its patches, gradient and rings, and Malvar–He–Cutler's crop of the rings.

### CI

- **Windows** (the working tree at `b4e3e03`): all 8 CI steps passed, with 64 suites and 100 tests. All seven examples' `--smoke` passed.
- **Ubuntu 24.04.5** (WSL, a fresh clone of `b4e3e03`, 175 files, no CR in any): all 8 steps passed, with the same 64 suites and 100 tests.
- **GitHub CI:** to come.

## Follow-ups and known gaps

- **P08c:**
  - the CUDA twins of demosaicing and of these steps;
  - the ≤ 10 ms budget;
  - `ex-demosaic`'s page comparing the methods.
- **A colour matrix for the MIChrome 20** needs calibration with a real chart (M3). The identity is the default.
- **The TIFF writer** writes one strip. It handles up to 4 GB, enough for 20 MP at 16 bits (120 MB). Larger images and OME-TIFF are `oscope-store`'s.

## Clean room

- **The sRGB curve** is IEC 61966-2-1's: linear below 0.0031308 with slope 12.92, then `1.055 t^(1/2.4) − 0.055`.
- **The TIFF file** follows Adobe's TIFF 6.0 specification: a baseline RGB image, and the tags listed there.
- **The rest,** black level, white balance, colour matrix and binning, are the plain definitions.
- **Nothing was copied** from `microscope` or any other codebase.
