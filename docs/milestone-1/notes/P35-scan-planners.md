# P35: Scan planners and focus maps

- Status: In review
- Branch / PR: `m1/p35-scan-planners` / [#49](https://github.com/lucaswewa/OScope/pull/49)
- Size: M · estimated ~2,500 before it started (regions ~220; the plan model and the field ~220; the planners ~200; the focus map and its solver ~330; estimates ~130; measuring a map ~80; the simulator's tilt ~40; tests ~600; `ex-plan` ~550; glue ~30; and a little on top); one pull request, at the owner's standing choice · measured **2,595** hand-written lines added (code 1,278: `oscope-workflows` 1,231, `oscope-sim` 47; tests 757; examples 535; glue 25), and 27 removed. **That is 1,995 over M's 600, and 4 % over the estimate.**

## Summary

Every scan pattern, planned and previewable before the stage moves ([spec](../phases.md#p35); plan §5.14).

- **Regions** of the stage: rectangles, polygons and masks.
- **The field** one frame covers, from the calibration's camera-to-stage matrix, and the steps between tiles that give the overlap asked for. With the camera turned, the tiles still cover what they should.
- **The planners:** raster, snake, tiled z-stack, z-scan and multi-position. Each gives a plan: its tiles in order, with how each one's z is found (fixed, from a focus map, or by autofocus as the scan reaches it) and the planes taken there.
- **Focus maps:** focus measured by autofocus at sample points, then a plane or a thin-plate spline fitted to it, outliers rejected one at a time.
- **Estimates** of a plan's time and data size. Plans, maps and estimates serialise as JSON: the preview P36's `scan` Thing will serve.
- **The simulator's slide can tilt,** and the focus map recovers the tilt.
- **`ex-plan`** measures a focus map on the tilted slide against the truth, prints seven plans with their estimates, and draws them on a page.
- **P34 is marked Done.**

## What was built

- **`oscope_workflows::scan`,** a new module:
  - **`Region`:** a rectangle, a polygon by its corners, or a `Mask` of cells. Each checks itself, gives its bounds, and says whether it holds a point and whether it touches a tile's cell.
  - **`Field`:** from the calibration's matrix M (`Field::new`) or a nominal pixel (`Field::nominal`). It gives the frame's corners on the stage, the cell a grid tiles, the overlap and the steps.
  - **The plan model:** `Plan`, `Tile`, `Pattern`, `ZPolicy` (`fixed`, `map`, `autofocus` every n tiles), `Stack` and `Position`.
  - **The planners:** `raster`, `snake`, `tiled_stack`, `z_scan` and `positions`, refusing more than 100,000 tiles.
  - **Focus maps:** `sample_points`, then `FocusMap::fit` with a `Surface` (`plane`, or `thin_plate` with its smoothing) and `FitOptions` (the threshold and the floor), and `FocusMap::z_at`.
  - **`measure_map`:** autofocus at each sample point, each `MapSample` with its z or its refusal.
  - **`estimate`:** from a `Timing` and a `FrameSize`, an `Estimate` of tiles, frames, bytes, travel, autofocus runs and seconds.
- **`oscope-sim`:**
  - `World::tilt`, with `World::tilted` and `World::focus_at`. A field renders at its centre's tilt, and the truth (`World::in_focus`) adds it.
  - The tile cache drops its tiles when the tilt changes.
  - `SimulatorConfig::tilt`: `tilt = [x, y]` under `[devices.simulator]`, in µm a mm.
- **`autofocus`'s** helpers `device` and `check` are now crate-wide, for `measure_map`.
- **`examples/ex-plan`** (port 8239).

## How to try it

`cargo run --release -p ex-plan`, then open http://127.0.0.1:8239/. The [README](../../../examples/ex-plan/README.md) says what it prints and what the page draws.

On the owner's PC, at real time through 1024 × 768 of the 20 MP sensor's middle, the map of 9 points found the plane rising 1.02 µm a mm along x and falling 0.55 along y, where the slide tilts 1 and 0.5. Between the points, the plane came within 0.12 µm of the truth and the thin-plate spline within 0.31, against a quarter of the depth of field of 1.97 µm. The snake over the coverslip took 1,333 tiles and the one over the tissue 370.

## Performance

No budget in §5.20 applies to planning. On the simulator at real time, through 1024 × 768 of the 20 MP sensor, each autofocus of a map took 10.8 s on average, which `ex-plan`'s estimates take as an autofocus's time.

## Design notes and deviations from the plan

### The field and its cell

A grid tiles **cells**: the largest rectangles, square with the stage, that each frame holds about its centre. A cell has the frame's proportions, or the turned frame's for a camera turned near a right angle, whichever is larger.

- **The cell's size.** Its corners, taken back to the image through M⁻¹ = N, must stay within the frame: |N₀₀| a + |N₀₁| b ≤ w/2 and |N₁₀| a + |N₁₁| b ≤ h/2, for half sides a and b in µm. With a and b in the frame's proportions, the larger scale that meets both is the cell's.
- **Square with the stage,** a frame is its cell, and neighbours overlap by exactly the share asked.
- **Turned,** the cell is smaller, and the frames overlap more than their cells do. The MIChrome 20's 3:2 frame through the 10×, turned 0.5°, keeps 98.7 % each way. Turned 2°, its height binds, and it keeps 95 %.
- **Coverage follows.** The cells span the region's bounds, each kept cell is inside its frame, and every point of the region is in a cell that touches it. The tests check every 100 µm of three regions over real distances.

### The grid and its order

- **Enough tiles** that n − 1 steps and a cell span the bounds, centred on them, so the overhang is even on both sides.
- **A tile is kept** where its cell touches the region, edges included.
- **Rows run along x,** from the least y. A raster runs each row from its least x. A snake runs each row the other way from the last row that has tiles, the boustrophedon of Choset (2000). Over the coverslip, the snake's travel is about half the raster's.
- **A region in parts,** such as tissue in islands, is one snake across all of it, each row crossing the gaps. An order by island is P37's, if it pays.
- **Positions** keep the order given. A position's own z wins over the plan's policy.

### Focus maps

- **The plane** is z = c₀ + c₁ u + c₂ v, by least squares, with u and v in mm from the points' middle, so its slopes read in µm a mm.
- **The thin-plate spline** is Bookstein's (1989) form of Duchon's (1977) spline: z = a₀ + a₁ u + a₂ v + Σ wᵢ U(rᵢ), with U(r) = r² ln r. It solves [K + λI, P; Pᵀ, 0] [w; a] = [z; 0] by Gaussian elimination with partial pivoting. With λ = 0 it passes through each point; with more, it smooths (Wahba, 1990).
- **Outliers,** one at a time, the worst first:
  - each point's miss is its leave-one-out residual, the surface fitted to the others against it (Craven and Wahba, 1979), so an outlier can't pull the surface to itself;
  - the scale is 1.4826 times the misses' median absolute deviation (Hampel, 1974; Rousseeuw and Croux, 1993), but never less than a floor;
  - a point missing by more than 3 scales is rejected, while more than four points, and more than half of those given, are left.
- **The floor** is what focus can tell apart. The example and the test take a quarter of the depth of field. P36 should set it from the objective.
- **What a spline can't judge.** A spline judges a point at a corner of the sampled area by extrapolating the others. The first test's bowl, 15 µm deep over 4.5 mm with points 1.5 mm apart, lost two good corners that way, after its true outlier. Over real distances a slide's sag is far gentler: a bowl 5.6 µm deep across 21 mm keeps every corner, and the floor keeps misses under three floors. A plane extrapolates well, and judges its corners.
- **`rms_um`** is the leave-one-out misses' root mean square: how well the map foretells focus where it wasn't measured. It is `None` with only three points.

### Measuring a map

`measure_map` moves to each point at the z last found and autofocuses there. A refusal, an empty field or a focus out of reach, is kept with its reason and no z. The map fits the points found. Where the stage starts, near focus, is the caller's: the user's focus, or in P36 the overview's.

### Estimates

- **The moves** go in straight lines between tiles, as G-code moves them, at the stage's default speed. Acceleration isn't counted.
- **Each plane** takes its settling and a frame: the exposure, or the camera's time between frames, if longer.
- **Each autofocus** takes the time given.
- **The data** is the frames' raw bytes, before compression and the pyramid.

### The simulator's tilt

- **The tilt** is µm a mm along x and y, from the slide's corner.
- **Rendering.** A field renders at its centre's tilt. The camera's frames come from cached tiles of 256 pixels, each rendered at its own centre's, so a frame follows the tilt tile by tile. At 1 µm a mm, a tile at the 10× spans 0.06 µm of it.
- **The truth** adds the tilt where it is asked, and the tile cache keeps it in the mark that clears its tiles.

### Other choices

- **Limits:** at most 100,000 tiles in a grid, an overlap of at most 0.9, at most 10,000 planes in a stack. Each refusal says why.
- **A plan carries its map,** so a preview or the engine evaluates the same surface.
- **The mask's JSON** is a value for each cell. A tissue mask at 100 µm over the coverslip is 120,000 cells; the page gets runs of cells instead.
- **Pixels are taken as square** for the cell's proportions. The calibration's aspect is within a hair of 1.

### The tests' turns, and two cores

- **One simulator in the scan suite:** the focus map on the tilted slide. The suite's other tests only plan, and it stops its camera at its end.
- **Two cores, five times each.** Pinned to two cores, every suite this phase adds or changes passed 5 times in 5. The workflows' `scan` suite took 437–442 s a run there, nearly all of it the focus map's ten autofocus runs, and `ex-plan --smoke` 189–194 s.

### The size

M's budget is 600; P35 measured 2,595, about 4 % over its estimate of ~2,500.

- **One pull request,** under the owner's standing choice, with the overrun explained. The regions, the field, the planners, the focus map and the estimates are one model, which P36's engine takes whole; the simulator's tilt is what the map is tested against.
- **Against the estimate,** the code took 1,278 lines against ~1,220, the tests 757 against ~600, and the example 535 against ~550.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-workflows` | `scan` (7, unit) | **A concave polygon** holds its inside and edges only, and touches a cell across an edge with no corner inside either. **A mask** is its cells that are in, with its bounds. **Regions that are none** are refused. **A tilted plane** recovered, and an outlier 12 µm off rejected. **A spline follows a bowl** a plane can't, through each point, rejecting an outlier. **Points that fit no surface** are refused. **Sample points** snake through the region |
| `oscope-workflows` | `scan` (6) | **Every point of a region in a tile's frame,** every 100 µm, for the coverslip, a C 20 × 15 mm, and two islands of a mask, by raster and by snake, the camera turned 0.3°. **Neighbours overlap by the share asked,** exactly with the camera square; turned, each cell the largest its frame holds, for frames wider than high and higher than wide. **A snake runs each row the other way,** turning only at rows that have tiles, travelling under 0.6 of the raster's distance. **Stacks, z-scans, positions and maps** set each tile's z and planes; what can't be is refused. **Estimates** add the moves, straight, the frames and the autofocus. **A focus map measured by autofocus recovers a tilted slide:** 9 points over 4 × 4 mm of an island, clear glass refused among them; the plane's slopes within 0.25 µm a mm of the tilt; both surfaces within a quarter of the depth of field of the truth between the points |
| `oscope-sim` | `truth` (1 more) | **A tilted slide** is sharpest higher along its tilt: the truth, and the rendering |
| `oscope-sim` | `tiles` (1 changed) | **A new tilt** is never seen through old tiles |
| `ex-plan` | `--smoke` | The map within a quarter of the depth of field between its points; every plan with tiles; the page and the plans served |

**Where they ran:**

- **On Windows,** `scripts/ci-local.sh`, run alone on `f4bb910`: 46 of 46 steps, 465 tests in 173 suites. The Python tests ran for real, and every example's `--smoke`, `ex-plan`'s new among them. The CUDA tests weren't run, as P35 touches no CUDA path.
- **On Linux,** `scripts/ci-clone.sh` under WSL, on a fresh clone of `f4bb910`, run alone: 46 of 46 steps, 465 tests in 173 suites, the Python tests real; 636 files, none with a CR.
- **On two cores,** pinned, 5 times out of 5 each:
  - `oscope-sim`'s `truth` and `tiles`;
  - `oscope-workflows`' unit tests and `scan`;
  - `ex-plan --smoke`.

**Mutation checks** (each broke the code on purpose, then the tests were run): thirty-two of thirty-three caught.

- **They covered:**
  - the regions: the even-odd rule, a polygon touching across an edge, a mask's cells and bounds;
  - the field: the cell's two constraints, the larger of its two shapes, the step, a frame's corners;
  - the planners: a stack's planes, the snake's direction and its turns at empty rows, the grid's count and centring, a position's own z, the limit on tiles;
  - the focus map: the robust scale, leave-one-out misses, the centre, pivoting, back substitution, the sample points' order and place;
  - the estimates: straight moves, autofocus runs, settling, bytes;
  - measuring: a refused point's z;
  - the simulator: the tilt in rendering, in the truth, in the tile cache's mark, and in the configuration.
- **Reading the tests against the list first** found three gaps, now covered: the snake's turn at an empty row (the mask's islands had none between them), a diagonal move in the estimates, and a refused point in a map.
- **The first run missed three more.** Two are covered now:
  - a polygon's inside on any crossing: no test point's ray crossed two edges;
  - the cell's width constraint: a frame wider than high binds by its height, so a frame higher than wide is tested too.
- **One is a known gap:** each point of a map started at the first point's z. Within autofocus's default range of ±6 depths of field, either start finds the focus on the simulator's tilt.

| Mutant | Caught by |
|---|---|
| A polygon's inside on any crossing | `scan::region`'s concave polygon, once a point left of it was added |
| A polygon touching a cell only by a corner | `scan::region`'s concave polygon |
| A mask's cell touching only by its first corner's cell | every point of a region in a frame; the snake |
| A mask's bounds a cell short; its cells by the nearest corner | `scan::region`'s mask |
| The cell's width not shrunk by the turn | the overlaps, once a frame higher than wide was added |
| The turned cell taken when smaller; the step not less the overlap | the overlaps |
| A frame's corner misplaced; a grid a tile short; a grid off centre | every point of a region in a frame |
| A stack a plane short; a position's own z ignored; no limit on tiles | stacks, positions and maps |
| The snake's first row backwards; turning at empty rows | the snake |
| The robust scale ten times too large; misses in the sample | `scan::focus_map`'s plane and spline |
| A map about the stage's origin; pivoting on the smallest; back substitution adding | `scan::focus_map`'s plane and spline |
| The sample points' order and place | `scan::focus_map`'s sample points |
| Moves along the axes; autofocus rounded down; no settling; a byte a pixel | the estimates |
| A refused point keeping a z; the configuration's tilt not set | the focus map on a tilted slide |
| Each point started at the first's z | **not caught**: a known gap |
| Fields rendered, or the truth, without the tilt | the tilted slide's truth |
| Tiles kept when the tilt changes | a new slide or tilt never seen through old tiles |

## Follow-ups and known gaps

- **P36's engine** takes these plans, and its `scan` Thing serves their previews. It should build a `Timing` from the devices, and set a map's floor from the objective's depth of field.
- **P37** gives masks from the overview's tissue, and may order islands one by one.
- **A spline's smoothing** is given, not chosen. Generalised cross-validation (Craven and Wahba, 1979) could choose it.
- **The estimates** leave out acceleration and the writing of frames, and take an autofocus's time as given.
- **Starting each point of a map at the z last found** isn't tested. Within autofocus's default range of ±6 depths of field, either start finds the focus on the simulator's tilt.

## Clean room

Nothing was copied from `microscope` or any other codebase. Each method is written from its sources:

- the snake's order, from the boustrophedon decomposition of Choset (2000);
- the even-odd rule, from Shimrat (1962) and Haines (1994), and segments meeting by cross products, from Cormen et al. (2009);
- the thin-plate spline, from Duchon (1977) and Bookstein (1989), with its smoothing from Wahba (1990);
- leave-one-out residuals, from Craven and Wahba (1979);
- the robust scale, from Hampel (1974) and Rousseeuw and Croux (1993);
- Gaussian elimination with partial pivoting, from Golub and Van Loan (2013);
- focus maps in whole-slide imaging, as Montalto et al. (2011) and Bian et al. (2020) survey them.

### Sources

- H. Choset, "Coverage of known spaces: the boustrophedon cellular decomposition", *Autonomous Robots* 9, 247–253, 2000.
- M. Shimrat, "Algorithm 112: Position of point relative to polygon", *Communications of the ACM* 5(8), 434, 1962.
- E. Haines, "Point in polygon strategies", in P. Heckbert (ed.), *Graphics Gems IV*, Academic Press, 24–46, 1994.
- T. H. Cormen, C. E. Leiserson, R. L. Rivest and C. Stein, *Introduction to Algorithms*, 3rd ed., MIT Press, 2009, §33.1.
- J. Duchon, "Splines minimizing rotation-invariant semi-norms in Sobolev spaces", in *Constructive Theory of Functions of Several Variables*, Lecture Notes in Mathematics 571, Springer, 85–100, 1977.
- F. L. Bookstein, "Principal warps: thin-plate splines and the decomposition of deformations", *IEEE Transactions on Pattern Analysis and Machine Intelligence* 11(6), 567–585, 1989.
- G. Wahba, *Spline Models for Observational Data*, SIAM, 1990.
- P. Craven and G. Wahba, "Smoothing noisy data with spline functions", *Numerische Mathematik* 31, 377–403, 1979.
- F. R. Hampel, "The influence curve and its role in robust estimation", *Journal of the American Statistical Association* 69(346), 383–393, 1974.
- P. J. Rousseeuw and C. Croux, "Alternatives to the median absolute deviation", *Journal of the American Statistical Association* 88(424), 1273–1283, 1993.
- G. H. Golub and C. F. Van Loan, *Matrix Computations*, 4th ed., Johns Hopkins University Press, 2013, §3.4.
- M. C. Montalto, R. R. McKay and R. J. Filkins, "Autofocus methods of whole slide imaging systems and the introduction of a second-generation independent dual sensor scanning method", *Journal of Pathology Informatics* 2, 44, 2011.
- Z. Bian, C. Guo, S. Jiang, et al., "Autofocusing technologies for whole slide imaging and automated microscopy", *Journal of Biophotonics* 13(12), e202000227, 2020.
