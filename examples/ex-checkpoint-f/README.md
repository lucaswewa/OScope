# ex-checkpoint-f

Checkpoint F: a demo script, over HTTP as a client would run it. It counts cells and quantifies DAB on a stitched slide, then makes a PDF report.

1. **The slide.** The simulated H-DAB slide is loaded (P45). In it, 40 % of the nuclei hold DAB, at three strengths. A snake over 1 × 0.75 mm across its first island's edge is scanned at 20× and stitched.
2. **Its nuclei,** counted by P46's `cells.count` with `dab`: each nucleus classed by its mean DAB as negative, weak, moderate or strong.
3. **Its DAB across the tissue,** by P45's `stain.separate` with H-DAB's stains: the tissue that is DAB-positive, and the H-score.
4. **A report,** by P47's `report.make`, through OScope's template:
   - a section for each result: the mosaic, the count, and the three stains;
   - their pictures, summaries, the nuclei's areas as a chart, and their provenance;
   - as `report.pdf` and `report.html`.
5. **The page** lists the report's files, and shows its HTML.

## How to try it

```sh
cargo run --release -p ex-checkpoint-f
```

It serves at http://127.0.0.1:8217/. On the owner's PC, at real time on the simulator's quarter sensor, it took 28 s and printed:

```text
ex-checkpoint-f: an H-DAB slide, a snake over 1.00 × 0.75 mm at 20×, stitched; 4450 nuclei, 4599 per mm² of tissue
  by their DAB: 2674 negative, 743 weak, 978 moderate, 55 strong: 39.9 % positive
  across the tissue: DAB positive 6.7 %, H-score 12
  a report of 5 sections: report.pdf of 564 kB and report.html of 683 kB
  open http://127.0.0.1:8217/ to read it
```

**Two measures of DAB, and why they differ:**
- **Of the nuclei:** each nucleus is classed by its mean DAB, so about 40 % of them are positive, as the slide holds.
- **Of the tissue:** each pixel is classed. DAB lies only in nuclei, so far less of the tissue is positive.

## Its checks

`cargo run -p ex-checkpoint-f -- --smoke` scans 0.6 × 0.45 mm at ten times real time, then checks that:

- more than 50 nuclei are counted, and between 20 % and 60 % of them are DAB-positive;
- some of the tissue is DAB-positive, with an H-score above 0;
- the report's PDF is one, and its HTML holds the count's section, the DAB summary's `positive_percent`, `h_score` and `strong`, and a chart;
- it has three sections at least;
- the page and its script are served.
