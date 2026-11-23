# ex-pipeline

Run in the Linux development container. CUDA is enabled by default, with CPU
fallback. `--require-cuda` requires availability and an actual CUDA recipe step;
use the bin-and-measure recipe for that check. See the
[R05 procedure](../../docs/phases/r05-storage-processing.md#manual-test).

The pipeline engine (P21, [ADR-0017](https://github.com/lucaswewa/OScope/blob/44bc08a53f98dc29db1fa6b488a4f6269732a71a/docs/adr/0017-the-pipeline-model.md)): a recipe of typed nodes, run on an image from the command line.

It prints the recipe's graph, then runs it:

1. **once,** with each step's time;
2. **again,** every step's result from the memo cache;
3. **after an edit** to a parameter: only that step, and those after it, run again;
4. **tile by tile,** each tile read with the halo its steps need, and compared with the whole run;
5. **over three more fields of the slide,** as a compute job with progress.

The nodes are `oscope-pipeline`'s trivial test nodes (`test.box_blur`, `test.add`, `test.threshold`, `test.statistics`, `test.bin2x2`, `test.sleep`). The stock nodes come in P22 and P23.

## How to try it

```sh
cargo run -p ex-pipeline
```

It renders a 1024 × 768 field of the simulated H&E slide through the 5× objective, and runs [`recipes/small-dark-features.json`](recipes/small-dark-features.json) on it. The recipe finds the nuclei:

- **`fine` then `finer`:** a light blur, twice;
- **`coarse`:** a wide blur, beside them;
- **`contrast`:** `coarse − finer`, which is high where a small feature is darker than what is around it;
- **`mask`:** where the contrast reaches 0.04.

**You should see:**

- **The graph:** the five steps in the order they run, each with its node and version, its device, what it reads and its parameters. Tiles read 12 px around them: the widest blur's radius.
- **Run 1:** each step's time. `fine` and `coarse` run at once, on the batch pool's threads. The time each spends waiting on its own threads may go to the other's work, so their times overlap.
- **Run 2:** every step `cached`.
- **Run 3,** after `mask.level=0.06`: every step `cached` but `mask`.
- **Tile by tile:** 12 tiles of 256 px, *the same as the whole run, to the bit*.
- **The batch:** a compute job over three more fields, with its progress by item, then *completed* and the files it wrote.

The outputs are PNGs in `.oscope/ex-pipeline/data/out/`: `contrast.png`, stretched from its least value to its greatest, and `mask.png`, white on the nuclei. The batch writes `field-1-mask.png` and the like. The job's history is in `.oscope/ex-pipeline/data/index.sqlite`.

**Things to try:**

- **Another edit:** `cargo run -p ex-pipeline -- --set coarse.radius=20`. Now `coarse`, `contrast` and `mask` run again, and the tiles read 20 px around them.
- **Several edits:** `--set mask.level=0.02 --set fine.radius=2`.
- **Smaller tiles:** `--tile 32`. Still the same to the bit, with more tiles.
- **A bad edit:** `--set coarse.radius=100`. The recipe is refused, saying where: `nodes[2] (coarse) params/radius: 100 is above the maximum, 64`.
- **Another recipe:** [`recipes/bin-and-measure.json`](recipes/bin-and-measure.json) bins twice, then gives each channel's statistics as a table and a JSON number:

  ```sh
  cargo run -p ex-pipeline -- --recipe examples/ex-pipeline/recipes/bin-and-measure.json
  ```

  It can't run tile by tile: binning changes the size.
- **On the GPU:** enter the GPU-enabled container and add `--require-cuda` to the last command. The two binning steps run on the GPU. The image goes up once, and comes back once for `stats` and the output. Steps on the GPU aren't memoised, so run 2 runs them again.
- **Your own image:** `--image photo.png`, or a JPEG or TIFF.

## `--smoke`

```sh
cargo run -p ex-pipeline -- --smoke
```

It runs on 256 × 192 fields, with 64 px tiles and a temporary data root, and checks:

- that run 2 is all cached;
- that the edit re-runs only `mask`;
- that the tiled run is the same as the whole one;
- that the batch job completes, with a result for each field;
- that the outputs were written.

It ends with `smoke: ok`, or says what failed and exits with code 1.
