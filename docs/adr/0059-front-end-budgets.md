# ADR-0059: Front-end budgets

- Status: Accepted
- Date: 2026-11-25
- Phase: W10.3

## Context

- **The server has had its budgets since P53:** plan §5.20's, each measured by the benchmark of the phase that set it, which prints `perf: KEY VALUE` lines. `cargo xtask perf` runs them in release builds and judges each against its bound, in `target/perf/report.html`. The nightly soak, `cargo xtask soak`, finds a leak by Theil–Sen's slope of the server's memory.
- **The app had measures, but no budgets:** W1.5 measured the live view at 14.5 frames a second, 40 ms from exposure to page; #113 kept it there; W6.4 measured a full-screen deep-zoom view never seen at 88 ms, against §5.20's 150. None of them was run again, and a change could have undone each.
- **What a person feels of the app** is its live view's rate and delay, how soon it shows a frame once opened, how quickly a slide deep-zoomed comes sharp, how much it downloads, and whether it keeps growing in a long session.

## Decision

- **The app's budgets are `cargo xtask perf`'s, with `--web`:** measured in Chromium by `web/tests/perf/`, against the release server xtask builds, with CUDA if asked, printing `perf:` lines as the server's benchmarks do. One report holds both.

  | The measure | Its budget | Set from |
  |---|---|---|
  | The live view's rate, 1920 × 1272 in mode `design` | at least 12 fps | W1.5, #113: 14.5 |
  | A frame on the page after its exposure | at most 100 ms | W1.5: 40 ms, #113: 29 to 42 |
  | The first frame after opening the app | at most 2,000 ms | W10.3 |
  | A full-screen view never seen, deep-zoomed, median of 20 | at most 150 ms | §5.20, W6.4: 88 |
  | The bundle's entry, gzipped | at most 100 kB | W10.3: 67 |
  | Its largest chunk, gzipped | at most 300 kB | W10.3: 260, VTK.js's |
  | All of it, gzipped | at most 1,200 kB | W10.3: 915 |
  | The memory's growth over a long live view | at most 10 MB an hour | W10.3: −0.3, a heap of 11.5 MB |

- **The memory's growth is Theil–Sen's slope** of the JavaScript heap after a collection, sampled every 15 s, after the run's first quarter: as P53's soak reads the server's. It needs a whole run, 10 minutes in `perf`, and isn't judged in a quick one.
- **The soak keeps the app open:** `cargo xtask soak --web` runs the memory's test as long as the soak, on a server of its own, and fails beside it if the growth is over its budget.
- **The rest stays as it was:** the end-to-end tests check what the app does, not how fast, and none of them measures.

## Consequences

- **Easier:** a change that slows the live view, delays the first frame, deep-zooms slower, grows the bundle or leaks in the page fails `perf --web`, which says by how much.
- **Harder:** `--web` needs Node and Chromium beside the Rust toolchain, and the soak's runner them too. Its runs take some minutes more. The release build it makes is xtask's own: run it with `CARGO_TARGET_DIR` elsewhere where a release server runs from `target/release`.
- **The budgets are this computer's:** as the server's are, they are measured on the owner's PC and its GPU. Elsewhere they say less.

## Alternatives considered

- **Lighthouse's scores:** they judge a page loaded once over a network, not a live view kept open, and would add a dependency of their own.
- **The budgets in the end-to-end tests:** they run on two cores, side by side, where timings say little, and each would fail at random under the gate's load.
- **A bundle budget in Vite's build:** it warns of a chunk's size, but judges no rate, delay or memory, and would stand apart from the report.
