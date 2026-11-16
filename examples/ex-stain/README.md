# ex-stain

P45's stain analysis on stitched slides, through the API as a client would.

1. **The H&E slide.** A snake over 2 × 1.5 mm across the edge of the simulated H&E slide's first island is scanned and stitched: a raw mosaic, half tissue and half glass.
2. **Its stains separated** by the job `stain.separate`, by colour deconvolution (Ruifrok and Johnston, 2001):
   - with H&E's vectors;
   - with Macenko's estimate of them from the slide.

   Each run makes three results: the stains' amounts, the hematoxylin image and the eosin image. The white, the light through glass, is estimated from the slide.
3. **The H-DAB slide:** the simulator's same section, stained for immunohistochemistry, is loaded, scanned and stitched at the same place. Its stains are separated with H-DAB's vectors, and its DAB is binned and scored.
4. **The page** shows any mosaic or stain image in OpenSeadragon, vendored, chosen from a list, with each separation's summary.

## How to try it

```sh
cargo run --release -p ex-stain
```

It serves at http://127.0.0.1:8231/. On the owner's PC, at real time on the simulator's quarter sensor, it took 34 s and printed:

```text
ex-stain: H&E, a snake over 2.0 × 1.5 mm across the island's edge, stitched; the white 0.752, 0.751, 0.753
  H&E's vectors: hematoxylin 0.159, eosin 0.225 over the tissue; the residual's RMS 0.0292
  Macenko's: hematoxylin 1.5° and eosin 6.0° from H&E's
ex-stain: H-DAB, the same place: of 1091331 tissue pixels, 1003531 negative, 46287 weak, 24845 moderate, 16668 strong
  DAB positive 8.0 % of the tissue, H-score 13
  open http://127.0.0.1:8231/ to see the mosaics and their stains
```

- **The white** is each colour's 99.5th percentile over the slide: the glass, from 0 to 1 of full scale.
- **The mean amounts** are each stain's optical density along its vector, over the tissue.
- **The residual** is what H&E's two stains don't explain: camera noise, and colours mixed in the mosaic's 2 × 2 cells.
- **Macenko's vectors** lean a few degrees from the standard ones: no pixel of the simulated section is one stain alone.
- **DAB is binned** from 0.15 (weak), 0.35 (moderate) and 0.60 (strong). 40 % of the H-DAB section's nuclei are positive, and nuclei cover only part of the tissue.

**The job,** for any stored RGB or raw image:

```sh
curl -X POST http://127.0.0.1:8231/api/v1/jobs/submit -H 'content-type: application/json' \
  -d '{"kind": "stain.separate", "params": {"project": "<project>", "input": {"kind": "result", "id": "<mosaic>"}, "options": {"stains": "hdab"}}}'
```

- `stains` is `he`, `hdab`, or `{"vectors": [[r, g, b], [r, g, b]]}`.
- `estimate: true` asks for Macenko's vectors.
- `white` gives the glass's light, and `thresholds` DAB's bins.

The summary is `summary.json` in the amounts' folder: `/api/v1/data/<project>/results/<amounts>/summary.json`.

## Its checks

`cargo run -p ex-stain -- --smoke` scans 1.2 × 0.9 mm at ten times real time, then checks that:

- the H&E slide's residual is under a tenth of its stains' amounts, and under half of what H-DAB's vectors leave there;
- Macenko's vectors are within 7° of H&E's;
- the H-DAB slide's positive percentage is between 1 % and 40 %, and its H-score between one and three times it;
- DAB is scored on the H-DAB slide alone, and the page and its script are served.
