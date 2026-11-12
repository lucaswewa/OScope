# P02: Core types: units, geometry, time, IDs

- Status: Done
- Branch / PR: `m1/p02-core-types` / [#9](https://github.com/lucaswewa/OScope/pull/9)
- Size: planned M, re-sized to L by the owner before it started · estimated ~765 (code ~430, tests ~220, examples ~85, glue ~30) · measured **1,648** hand-written lines (code 962, tests 527, examples 125, glue 34). Docs and `Cargo.lock` aren't counted. **That is 748 over L's 900.** The owner chose to take it in one pull request, with the overrun explained, rather than split it (see *Design notes*).

## Summary

`oscope-core` now holds the vocabulary that every later crate uses ([spec](../phases.md#p02)):

- **units:** micrometres and milliseconds;
- **the clock:** timestamps, mapped once to UTC;
- **geometry:** points, rectangles, stage positions, and an affine map with a least-squares fit;
- **IDs;**
- **the error conventions;**
- **versioned documents.**

The new [ADR-0007](../../adr/0007-units-coordinates-and-the-clock.md) records the units, the coordinate conventions and the clock. The example, `ex-core-tour`, shows all three, and CI now runs every example's `--smoke`.

## What was built

All in `crates/oscope-core/src/`, unless the table says otherwise.

| Module | What |
|---|---|
| `units` | **`Um` and `Ms`,** `f64` newtypes for lengths in µm and durations in ms.<ul><li>Arithmetic within a unit, scaling by a number, and the ratio of two values.</li><li>Conversions: `Um::from_mm`, `from_nm`, `mm()`, `nm()`; `Ms::from_secs`, `secs()`, `to_duration()`, and `From<Duration>`.</li><li>They print as `12.5 µm`, and serialise as plain numbers.</li></ul> |
| `clock` | **`Timestamp`:** nanoseconds since the clock started, in a `u64`.<ul><li>It takes a `Duration` added or subtracted, saturating at its ends.</li><li>`checked_since`, `saturating_since`, and `midpoint` for an exposure's middle.</li></ul>**The `Clock` trait:** `now()`, `start_utc()`, and `utc(t)` as the start plus `t`.<ul><li>**`SystemClock`** runs on `Instant`, with UTC taken once when it starts.</li><li>**`FakeClock`** moves only through `advance` and `advance_to`, and never back.</li></ul> |
| `geometry` | **`Point2`, `Size2`, `Rect` and `Xyz`:**<ul><li>`Point2`, `Size2` and `Rect` are in pixels or µm, as their API says. A `Rect` is half-open, with `contains`, `intersection` and `centre`.</li><li>`Xyz` is a stage position in `Um`.</li></ul>**`Affine2`,** `p ↦ M p + t`:<ul><li>`translation`, `scale`, `rotation`;</li><li>`apply`, `then` (composition), `determinant`, `inverse`;</li><li>`fit`, by least squares from point pairs, and `rms_error`.</li></ul> |
| `ids` | **`JobId`, `ProjectId`, `AcquisitionId` and `ResultId`,** made by one macro:<ul><li>UUID v7 newtypes, from `generate()`, ordered by creation;</li><li>they print, parse and serialise as hyphenated text.</li></ul>**`FrameId`** is a `u64` counter, with `next()`. |
| `error` | **The conventions every crate follows,** in the module's docs: one `Error` enum per crate (`thiserror`, `#[non_exhaustive]`), messages a user can act on, facts as fields, sources as `#[source]`, no panics on bad input, and `anyhow` only in binaries.<br>**`oscope-core`'s own `Error`** and `Result`. |
| `version` | **`Versioned`,** with `to_document` and `from_document`.<ul><li>A document is a JSON object with `schema` and `version` beside its fields.</li><li>An older version is upgraded one step at a time, through `Versioned::upgrade`.</li><li>A newer version, or another schema, is refused.</li></ul> |
| **Derives** | `serde` and `schemars` on every public data type. The JSON Schemas take their descriptions from the docs, and give IDs `"format": "uuid"`. |
| **Dependencies** | `chrono`, `schemars`, `serde`, `serde_json`, `thiserror` and `uuid` (`v7`), and `rand` for the tests. They are workspace dependencies, pinned to the versions `teta-wot` v0.2.0 locks. **The lock gained no packages,** only the edges between those already in it. |
| **`ex-core-tour`** | The tour, with `--smoke` (`examples/ex-core-tour/`) |
| **CI** | A step that runs `--smoke` for every example with a binary. It was a follow-up from P01 (`.github/workflows/ci.yml`) |
| **ADR-0007** | Units, coordinates and the clock, with its index row (`docs/adr/`) |
| **Plan** | P02 re-sized to L, and its overrun recorded in the change log; P01 *Done* (`docs/`) |

## How to try it

```sh
cargo run -p ex-core-tour             # the tour: see its README for the output
cargo run -p ex-core-tour -- --smoke  # the same, checked: ends with "smoke: ok"
cargo test -p oscope-core             # 23 tests
```

The tour's map is 0.24 µm per pixel, flipped and turned by 0.5°. From 24 points with ±0.05 µm of noise, the fit gives back:
- a pixel size of 0.24000 µm;
- an angle of 0.502°;
- an error of 0.032 µm at the far corner of the sensor.

## Performance

Not relevant to this phase: nothing here is on a §5.20 path. `Affine2::fit` is a single pass over its points.

## Design notes and deviations from the plan

- **The size, and why the estimate missed.**
  - **The estimate counted logic lines only.** The budget counts every hand-written line.
  - **Doc comments, blank lines and rustfmt's wrapping** are about half the code: every public item is documented, `missing_docs` requires it, and rustfmt puts long derive lists and assertions over several lines.
  - **The tests** came to 527 against ~220. Every module has its own file, and the property tests set up cases at real scale.
  - **The owner was asked before the pull request.** They chose one PR over a split into two of about 800 and 850 lines.
  - **Future estimates** will count from code as it is written here: about 2.2 lines per line of logic in documented code, and 1.5 in tests.
- **Geometry types are plain `f64`,** not generic over a unit.
  - The same `Point2` holds pixels or micrometres, as the API that uses it says.
  - `Xyz` uses `Um`, because a stage position is always in micrometres.
  - Integer pixel ROIs belong to the frame model (P03).
- **UTC is a `chrono::DateTime<Utc>`.**
  - `chrono` is already in the tree through `teta-wot`.
  - It serialises as RFC 3339, and schemars gives it `"format": "date-time"`.
  - `SystemTime` would have needed its own formatting and schema.
- **IDs are UUID version 7** (RFC 9562), not version 4.
  - They sort by creation time, which suits the SQLite index (P28) and lists of jobs.
  - Within one process `uuid` guarantees their order, and a test checks 10,000 in a row.
  - `generate()` rather than `new()`, which clippy would want paired with a `Default`.
- **The `Clock` trait has no sleep or timeout.**
  - Waiting on a scaled or stepped clock needs async, which `oscope-core` doesn't have.
  - P11 adds it, as ADR-0007 says.
- **Versioned documents keep their fields at the top level,** beside `schema` and `version`, so that files are easy to read and edit by hand.
  - The price is that a type may not have a field named `schema` or `version` itself. `to_document` refuses one that does.
  - Upgrades work on the JSON fields, before they are deserialised, so each version's code needs only the previous version's shape.
- **The property tests use seeded random cases,** as the owner chose: 500 seeds per property, and a failure names its seed. `rand` is already in the lock; `proptest` would have added about 10 crates.
- **The fit test's bounds.**
  - **The first bound was wrong.** It asked that every corner of the sensor be within the noise of the truth, and the worst corner of the 2,000 came to 0.080 µm.
  - **Why.** The corners are extrapolated from 25 random points, so they are the fit's least certain places.
  - **The mean error measured** 0.021 µm, as expected for that noise and those points.
  - **The test now checks both:** the mean below 0.03 µm, and the worst below 0.1 µm (a third of a pixel at 10×).
- **The smoke check caught a mistake in the example while it was written.** It expected a rotation of −0.5°. With y flipped first, the fit's angle reads +0.5°, which is correct.
- **`Ms` has no conversions beyond seconds and `Duration`.** Others, such as microseconds for a camera SDK, belong in the driver, at the edge (ADR-0007).

## Tests

- **`tests/geometry.rs`,** 7 tests, 5 of them over 500 seeded random maps like a calibration's: 0.05–2.4 µm per pixel, any angle, perhaps flipped, a little shear, anywhere on 100 mm of travel, over a 5472 × 3648 sensor.
  - A map then its inverse returns every point, to 1 nm.
  - `then` equals applying one map and then the other.
  - **A fit recovers a known map** from 25 pairs with ±0.05 µm of noise: the mean corner error is below 0.03 µm and the worst below 0.1 µm, and the residual is below the noise.
  - A fit without noise is exact.
  - Too few points, collinear points and a singular map are errors; a small map isn't singular.
  - Rectangles: the half-open edges, the centre, intersections, and touching that isn't overlapping.
  - The JSON of `Xyz`, `Affine2` and `Rect`, and their round trips.
- **`tests/clock.rs`,** 4 tests:
  - the system clock never goes back, on four threads reading it 100,000 times each;
  - it moves, which the test waits for rather than sleeping, and maps to UTC from its start;
  - the fake clock moves only forwards and when told, and saturates;
  - timestamp arithmetic, midpoints (also at the top of the range), text and JSON.
- **`tests/units_and_ids.rs`,** 7 tests:
  - conversions and arithmetic;
  - `Duration` both ways, including negative and NaN;
  - text with a precision, JSON, and the schema;
  - 10,000 IDs are unique and in creation order;
  - IDs round-trip through text and JSON, with `"format": "uuid"`;
  - a bad ID's error names its kind;
  - frame numbers.
- **`tests/version.rs`,** 5 tests:
  - a document carries its schema and version, and reads back;
  - versions 1 and 2 are upgraded step by step;
  - a newer version, another schema, a missing or bad version, and a non-object are refused;
  - a type mismatch keeps serde's error as its source;
  - by default an older version can't be upgraded;
  - a type with its own `version` field can't be written.
- **Mutation checks,** each restored afterwards. Every one made the tests named here fail:

  | Broken on purpose | Failed |
  |---|---|
  | A sign in the fit's matrix | `a_fit_recovers_a_known_map_from_noisy_pairs` |
  | Collinear points not detected | `degenerate_fits_and_inverses_are_errors` |
  | A singular map not detected | `degenerate_fits_and_inverses_are_errors` |
  | A newer document accepted | `a_newer_or_foreign_document_is_refused` |
  | An old document not upgraded | `an_old_document_is_upgraded_a_version_at_a_time`, `by_default_an_older_version_cant_be_upgraded` |
  | The fake clock allowed to go back | `a_fake_clock_moves_only_forwards_and_when_told` |

- **Windows** (the working tree): all 8 CI steps passed on `6cadaaa`, the examples' `--smoke` among them, with 42 suites and 33 tests.
- **Ubuntu 24.04.5** (WSL): a fresh clone of `6cadaaa` in the Linux file system, with 86 files and no CR in any. All 8 steps passed, with the same 42 suites and 33 tests. The target folder was its own, kept from P01's run.
- **GitHub CI** passed on the pull request: Ubuntu in 1 min 11 s, Windows in 2 min 20 s. Every step succeeded, and `ex-core-tour --smoke` printed `smoke: ok` on both.

## Follow-ups and known gaps

- **Waiting on the clock** (sleeps and timeouts at a simulated rate) is P11's, as ADR-0007 says.
- **The stage's axis directions** are recorded in P02b, for ADR-0007's stage coordinates.
- **`Affine2` doesn't decompose itself** into scale, angle, shear and flip; the example does it by hand. The calibration (P32) can add that when it needs it.
- **U06's 24-hour soak** ends about 2026-11-12 16:45 PST. Its result goes into the U06 notes, in this pull request or the next.

## Clean room

Everything here was written for this phase. Nothing was copied from `microscope` or any other codebase.

- **The affine fit** is ordinary linear least squares, with the points centred on their means first, so that the translation maps one mean to the other. Späth sets out the same problem, fitting an affine transformation and a translation between two point sets in the least-squares sense: H. Späth, "Fitting affine and orthogonal transformations between two sets of points", *Mathematical Communications* 9 (2004), 27–34 ([Hrčak](https://hrcak.srce.hr/712)).
- **The IDs** are UUID version 7, from RFC 9562, made by the `uuid` crate.
