# P45: Stain analysis

- Status: In review
- Branch / PR: `m1/p45-stain` / #61
- Size: M · estimated ~2,500 before it started (in `oscope-imaging`, optical density, stain vectors and colour deconvolution, Macenko's estimate with a 3 × 3 eigensolver, and the DAB bins, ~350; in `oscope-sim`, the H-DAB section and the stains' amounts as truth, ~120; the pipeline's four nodes ~220; in `oscope-workflows`, the job over a stored slide, tile by tile, ~320; tests ~850; `ex-stain` and its page ~600; glue ~40); one pull request, at the owner's standing choice

## Summary

To come with the code.

## Behaviour spec

Written before the code, from the sources at its end ([spec](../phases.md#p45); plan §5.17, D25).

### What it does

1. **Its input is a brightfield image in colour:** red, green and blue, the light each pixel passed. A raw image, a camera's Bayer mosaic, is taken on its 2 × 2 cells, each one RGB pixel, at the cells' size.
2. **Optical density,** channel by channel, against the white, the light through glass: OD = −log₁₀(I / W).
   - I is held at W/1024 or more, so OD is 3.01 at most.
   - **The white** is given, or else estimated: each channel's 99.5th percentile over the image's pixels.
3. **Stain vectors** are a stain's optical densities in red, green and blue for one unit of it, normalised to length 1 (Ruifrok and Johnston, 2001):
   - **hematoxylin:** (0.650, 0.704, 0.286);
   - **eosin:** (0.072, 0.990, 0.105);
   - **DAB:** (0.268, 0.570, 0.776);
   - the user's own, two or three.

   **H&E** is hematoxylin and eosin; **H-DAB** is hematoxylin and DAB. With two stains, the third vector is the cross product of the two, normalised. It's a residual: what neither stain explains.
4. **Colour deconvolution** (Ruifrok and Johnston, 2001):
   - By Beer and Lambert, a pixel's OD is its stains' amounts times their vectors: OD = a · M, the rows of M being the vectors.
   - So its amounts are a = OD · M⁻¹, each in units of its stain's vector.
   - **Refused:** vectors that aren't independent (|det M| under 10⁻³), or that have a component that is negative or not finite. The residual's components may be negative.
5. **Macenko's estimate** of two stains' vectors from the image (Macenko et al., 2009):
   - The pixels used are those whose optical densities, as a vector, have a length of at least β = 0.15: glass and the faintest stain are left out. (Revised while writing the tests: requiring β in every channel would leave out eosin alone, whose red and blue densities are small, so its vector couldn't be found.)
   - Their OD vectors' second moments, about the origin, have three eigenvectors (Jacobi's method). The two with the largest eigenvalues span the plane of the stains.
   - Each pixel's OD is projected on that plane, at angle φ. The stains' vectors are at φ's 1st and 99th percentiles, so outliers don't decide them.
   - Each vector is turned so its components sum positive. The one with more red OD comes first: hematoxylin, then eosin or DAB.
   - **Refused** if fewer than 100 pixels are dense enough.
6. **DAB quantification,** from an H-DAB deconvolution:
   - **Tissue** is a pixel whose OD, summed over its channels, is at least 0.05; glass isn't counted.
   - **Each tissue pixel is binned** by its DAB amount: negative under 0.15, weak from 0.15, moderate from 0.35, strong from 0.60.
   - **What it gives:** the counts of tissue pixels in each bin; the percentage of tissue that is positive (weak, moderate or strong); and the H-score, 1 × weak % + 2 × moderate % + 3 × strong %, from 0 to 300 (McCarty et al., 1985).
   - The thresholds can be given.
7. **Each stain's image,** the stain alone as it would look: W × 10^(−a·v) for its amount a and vector v, as 8-bit RGB.
8. **The pipeline's nodes:**
   - **`stain.deconvolve`:** an RGB image → its stains' amounts, an image of three channels (the first stain, the second, the residual). Its parameters are `stains` (`he`, `hdab` or `vectors`) and `white`, [1, 1, 1] by default. It works pixel by pixel, so it runs tile by tile.
   - **`stain.estimate`:** an RGB image → JSON of Macenko's two vectors, with `white`, `beta` and `percentile`. It needs the whole image.
   - **`stain.dab`:** an RGB image → a mask of classes, with `white`, `stains` and the thresholds. The classes are 0 for glass, 1 negative, 2 weak, 3 moderate and 4 strong. It works pixel by pixel, so it runs tile by tile.
   - **`stain.dab_score`:** a mask of classes → a table of the counts, the positive percentage and the H-score. A count, cheap over a whole slide.
9. **The job `stain.separate`,** a compute job, takes {`project`, `kind`, `id`, `options`}, over a stored image read tile by tile in its chunks.
   - **Options:** `stains`, `vectors`, `estimate` (Macenko's), `white` and the DAB thresholds.
   - **A first pass** keeps every 8th pixel each way, for the white and Macenko's estimate where they're asked for.
   - **A second pass** deconvolves each tile into three results of the project:
     - the amounts, 32-bit floats, its channels named by stain;
     - each stain's image, 8-bit RGB;
     - in the amounts' folder, `summary.json`: the white, the vectors, each stain's mean amount over tissue, the residual's RMS, and for H-DAB the DAB counts, percentage and H-score.
   - Progress counts tiles, and the job is cancelled between them.
   - **Refused:** an image that isn't RGB or raw, and stains it doesn't know.
10. **The simulator's H-DAB section,** "H-DAB tissue", is the H&E section's islands and nuclei, stained as an immunohistochemical slide:
    - hematoxylin as in H&E, no eosin;
    - DAB in 40 % of the nuclei, at three levels, weak, moderate and strong, in equal numbers, which land in the bins above;
    - DAB's vector as above.

    Both sections give each stain's amount at any point, summed over the section's depth, as the tests' truth.
11. **`ex-stain`** (port 8231):
    - **H&E:** the H&E slide scanned and stitched, separated with the H&E vectors and with Macenko's estimate. It prints the white, the vectors and their angles from the H&E ones, the stains' mean amounts, and the residual's RMS.
    - **H-DAB:** the H-DAB slide loaded, scanned, stitched and quantified. It prints the bins' counts, the positive percentage and the H-score.
    - **The page** shows a mosaic or one stain's image in OpenSeadragon, chosen from a list, beside each summary.

### What the tests show

- **Deconvolution:**
  - **Synthetic pixels** made from known amounts by Beer–Lambert give their amounts back within 10⁻⁵, for H&E, H-DAB and three vectors given, and within 10⁻² once quantised to 8 bits.
  - **The simulator's H&E and H-DAB sections,** rendered at known places, give back its amounts of each stain within 10⁻⁴, pixel by pixel. (Tightened from 10⁻³ while checking the tests: 10⁻³ couldn't tell the vectors' lengths, which differ from 1 by up to 0.2 %.) DAB lies only in the positive nuclei.
- **Macenko's estimate:**
  - from synthetic two-stain images with spread amounts, within 2° of the vectors;
  - from the simulator's H&E section, within 7°. (Revised while writing the tests from 5°: the section's nuclei hold eosin and its stroma hematoxylin, so no pixel is one stain alone, and each vector leans towards the other, 5.7° and 5.3°.)
  - from the simulator's H-DAB section, hematoxylin within 1°, and DAB within 20°. (Added while writing the tests: DAB lies only in nuclei that hold hematoxylin too, so its vector leans 16.5° towards hematoxylin. So the job uses the stains' own vectors unless asked to estimate.)
- **DAB:** synthetic patches with known fractions of each bin give their counts, percentage and H-score exactly, and pixels at the thresholds fall in the bins above them.
- **The nodes** give what the library gives, and `stain.deconvolve` and `stain.dab` give the same tiled or whole.
- **The job:**
  - over a stored RGB image and a raw one, its results recorded, with the amounts the library gives;
  - its summary, and a cancel;
  - refusals.
- **The example's smoke:**
  - the H&E slide's residual is under a tenth of its stains' mean amounts, and under half of what H-DAB's vectors leave there (made definite while writing the example);
  - Macenko's vectors are within 7° of H&E's;
  - the H-DAB slide's positive percentage is between 1 % and 40 %, and its H-score between one and three times it (made definite while writing the example: 40 % of the nuclei are positive, and they cover only part of the tissue).

### Sources

- Ruifrok, A. C. and Johnston, D. A. (2001). Quantification of histochemical staining by color deconvolution. *Analytical and Quantitative Cytology and Histology* 23(4), 291–299.
- Macenko, M., Niethammer, M., Marron, J. S., Borland, D., Woosley, J. T., Guan, X., Schmitt, C. and Thomas, N. E. (2009). A method for normalizing histology slides for quantitative analysis. *Proceedings of the IEEE International Symposium on Biomedical Imaging*, 1107–1110.
- McCarty, K. S. Jr., Miller, L. S., Cox, E. B., Konrath, J. and McCarty, K. S. Sr. (1985). Estrogen receptor analyses: correlation of biochemical and immunohistochemical methods using monoclonal antireceptor antibodies. *Archives of Pathology and Laboratory Medicine* 109(8), 716–721: the H-score.
- Golub, G. H. and Van Loan, C. F. (2013). *Matrix Computations*, 4th edition, §8.5: Jacobi's method for a symmetric matrix's eigenvalues.

## What was built

To come with the code.

## How to try it

To come with the code.

## Design notes and deviations from the plan

To come with the code.

## Tests

To come with the code.

## Follow-ups and known gaps

To come with the code.

## Clean room

To come with the code.
