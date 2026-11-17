# Milestone 1: a look back

Milestone 1 ran from the plan's approval on 2026-11-10 to P54 on 2026-11-17. It is the server, with a simulator good enough to develop and test against without the instrument. This is its look back ([the plan](../implementation-plan.md), [the notes](notes/README.md)).

## What it delivered

- **Upstream, in `teta-wot` (U01–U06):** what OScope needed of its Thing framework, released as v0.2.0, including binary streams, HTTP/2 over TLS, and authentication hooks.
- **The foundations (P00–P11):** the layered workspace and its checks; the domain types, logging and errors; the device traits and their fakes; colour, demosaicing and the image pipeline, on the CPU and with CUDA; the simulation clock and a grblHAL-like stage.
- **The simulator (P12–P15):** a MIChrome 20 at full resolution over procedural tissue, surfaces, cells, and imported samples, in real time.
- **The live instrument (P16–P23):**
  - the device Things;
  - the live preview, with its statistics;
  - the desktop app's sidecar contract and shared-memory preview;
  - the job service;
  - recipes on the live stream, with filters on the GPU.
- **Extensions (P24–P27):** packages, native extensions behind a versioned C ABI, Python extensions in workers of their own, and extension drivers.
- **Data (P28–P30):** projects and records in OME-Zarr, the scratch recorder at the design rate, capture, import and export.
- **Calibration and acquisition (P31–P40):**
  - registration, the stage's calibration, and colour;
  - autofocus and exposure;
  - plans, scans and the overview;
  - the live mosaic, post-stitching, and deep zoom on demand.
- **Volumes and analysis (P41–P48):**
  - z-stacks, depth from focus and deconvolution;
  - annotations and measurement, stain separation, cell counting;
  - reports in Typst;
  - a workspace of analyses with their provenance.
- **Running an instrument (P49–P54):**
  - accounts, sessions and the control lease;
  - status and health, power actions, settings with configuration migration;
  - performance budgets and a soak;
  - packages for Windows and Ubuntu.

## Its numbers

- **Phases:** 65 with notes: P00 to P54, with P02b, and P08, P09 split into P08b, P08c and P09b; and U01 to U06.
- **Size:**
  - About 134,000 hand-written lines added, before P54, by the plan's count: source, tests, examples and comments, not docs or lock files.
  - The median phase added about 2,250.
- **The budgets didn't fit the phases.**
  - 54 of the 63 phases measured were over their S, M or L budget (300, 600 or 900 lines). The phases' specs held more than those budgets allow.
  - From P03, the owner chose one pull request a phase, with the overrun estimated before the work and explained after it, over splitting each phase in three.
- **The estimates did fit,** once they counted what each phase is made of:
  - **58 phases** were estimated before they started. Measured over estimated, the median was 1.04, and 36 of the 58 came within 20 %.
  - **The furthest above:**
    - P02, ×2.15: doc comments and blank lines were half its Rust, unforeseen;
    - P09, ×1.9: its estimate came before its split;
    - P21, ×1.65: the recipes' validator and plans;
    - P38, ×1.5: the live mosaic's canvas.
  - **The furthest below:** P36, ×0.64, and P50, ×0.71.
  - P51 wasn't estimated, an oversight its notes record.
- **Every phase passed both local replays** before it merged: Windows, and Linux on a fresh clone.

## What went well

- **Writing the spec and the estimate first.**
  - Spec-first phases opened their notes with the behaviour and its literature before any code.
  - From P52, the estimate went into the plan's change log in a commit of its own, before the code.
- **The simulator.** Every phase from P12 on was built, tested and shown against it: tissue, cells, stage errors, faults on demand. M3's bring-up starts from code that has run for a week against a twin of the instrument.
- **Mutation checks, read first.**
  - Writing the list of mutants before running them, and reading the tests against it, found gaps in nearly every phase.
  - The runs then found weak tests that passed for the wrong reason, as in P53, where a mean of the slopes passed the spike test that was meant to catch it.
- **Two cores, five times.**
  - Running the touched suites side by side on two CPUs each found faults that 24 threads hid: four in P17–P18, three in P38.
  - Since ADR-0038, it is a step of every phase.
- **The local replays as the merge gate (ADR-0038).** GitHub Actions stopped starting jobs on 2026-11-14, a billing matter. Nothing stopped: each phase merged on its two replays, which run all CI's steps and every example besides.
- **The soak.** Its first runs found two real faults: finished jobs kept in memory for ever, and a project that couldn't always be deleted on Windows.

## What didn't

- **The budgets,** above. They measured the phases against a size the phases never had, so "over budget" said nothing. Estimates, against what was then measured, said more.
- **The build's appetite for the one disk.**
  - Build output filled C: three times, once to its last byte during a replay, which broke WSL until it was cleaned.
  - A phase's builds take 60 to 70 GB, and cleaning cold-starts the next ones.
  - Twice, the replays' examples build ran out of memory linking some 70 examples at once: cold after a clean (P51), and after a low-level crate changed (P53).
- **Timing in tests, under load.** A smoke run's budget overran by 4 ms in a replay (P49), and a mosaic was read once as a scan ended (P45). Each was fixed to wait for its condition, but each first failed a replay.
- **Tooling on Windows.** Bash heredocs that turned escapes into line breaks; scripts that wrote CRLF; a long soak (U06's 24 hours) cut at 19 h 26 min by an editor's restart.
- **Some numbers in the plan were assumptions.** The MIChrome 20 delivers 289 MB/s in 16 bits, measured in P02b, not the 580 MB/s §5.5 had assumed. The owner kept 580 MB/s as the design budget, for the camera after it.

## Lessons for M2

1. **Size phases by an estimate in four parts** (code, tests, examples, glue), with doc comments and blank lines counted. Keep the budgets only as a prompt to split.
2. **Commit the estimate before the code,** in the plan's change log.
3. **Read the tests against the mutant list first,** then run it. Break anything that detects or supervises on purpose, once.
4. **Run the touched suites on two cores before every pull request,** and make every test wait for a condition.
5. **Watch the disk and the memory:** `df` before every replay and release build; a clean between phases; cap the examples' build when a low-level crate changes.
6. **Ask the owner** before a cleanup, a merge outside a phase, a new class of dependency or advisory, a schedule on CI, a tag, or a push to `main`. Each was asked, and each answer is in the notes.

## What M2 inherits

- **The manual tests:**
  - P24–P34's checklist, on its branch, not yet pushed;
  - P44–P54's, written after this phase;
  - the owner's runs of P35–P43's.
- **The soak's open finding:** memory grows by some 10 KB a scan and stitch once warm, on Windows and on Linux. Its source isn't found. Run the nightly by hand for its four hours before turning its schedule on (P53).
- **Moving off `chrono`,** unmaintained: teta-wot first (RUSTSEC-2026-0543, P53).
- **The packages:**
  - CUDA isn't in them;
  - the Tauri installer replaces the zip;
  - a release needs GitHub Actions back, or the packages built by hand (P54).
- **The follow-ups the notes list,** among them:
  - colour thumbnails of raw images (P47);
  - hashes for every job's inputs (P48);
  - the browser's media under security (P49);
  - `ex-live-recipe`'s smoke budget under load (P49);
  - the job history's pruning (P53).
- **M2 itself:** the web and desktop clients over this API, its OpenAPI turned into TypeScript (plan §8).
