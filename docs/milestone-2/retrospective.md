# Milestone 2: a look back

Milestone 2 ran from its specs on 2026-11-17 to W10.4 on 2026-11-25. It is the app: one Vue 3 codebase in `web/`, served by the server at `/`, run against the simulated instrument throughout. This is its look back ([the plan](../implementation-plan.md#8-milestone-2-phase-overview), [the specs](phases.md), [the notes](notes/README.md)).

## What it delivered

- **The workspace and its parts (W0, W1):** the app's checks, tokens and themes, its components, forms from schemas, charts; the API's types from the server's own description; live properties, the connection and the login; the host adapter; and the preview's client.
- **The instrument from the app (W2 to W4):** the shell and its status; the live view, the camera, the stage, the lights, the overlays and the values; autofocus, capture, a live recipe and the mosaic built by moving; the jobs, the log, About and Power.
- **Scans (W5):** plans of every pattern, a run followed to its end, the histo scan from an overview to its regions and focus map, z-scans, stacks and positions.
- **The Gallery (W6):** projects, records, files imported and exported, and a deep-zoomed viewer.
- **The Workspace (W7):** the recipes over the API and a graph editor for them; analyses and their results; annotations measured; cells and stains; surfaces and volumes in 3D; reports.
- **Settings (W8):** the system, the display and the devices; calibration sets and their wizard; simulated samples, one's own among them; extensions and their panels; accounts.
- **Quality (W10):** the checkpoints G to L as workflows that drive the app; baselines of every tab and dialog, axe and a name on every control, the main workflows by the keyboard alone; the app's budgets beside the server's; and the app in the packages, with a user's guide.
- **The desktop app (W9) was dropped** after W7, at the owner's choice (ADR-0063): the browser is OScope's client.

## Its numbers

- **Phases:** 52 with notes, W0.1 to W10.4, of the 57 specified; W9's five were dropped.
- **Size:** about 78,000 hand-written lines measured over the first 51, against first estimates of 75,200 and second ones of 91,900. The median phase measured about 1,380.
- **The estimates:**
  - **The specs' first estimates were right on the whole,** a median of 0.98 of what was measured, but scattered: 20 of 51 within 20 %.
  - **The second estimates, made before each phase's code, ran high:** a median of 0.79, 21 within 20 %.
    - Most of the overestimates came after W5. Each phase reused more of what came before it than its estimate allowed for: the viewer, the forms, the charts, the tests' helpers.
    - The furthest below: W5.4, ×0.41; W7.7, ×0.49; W8.3, ×0.53; W6.4, ×0.54.
    - The furthest above, all early: W0.3, ×1.31; W0.6, ×1.29; W1.5, ×1.22.
  - **Four second estimates were over 2,500 lines,** all in W0 and W1, and the owner was asked before each.
- **Tests at its end:** 686 Rust tests; 857 unit tests of the app; 56 end-to-end specs, some 480 tests across the three browsers; 20 workflow tests; 316 baselines, each tab and dialog in both themes on Windows and on Linux.
- **Every phase passed its gate:** its touched suites five times on two cores, and both local replays, in full at each stage's checkpoint.

## What went well

- **The app in the browser, against the simulator,** from its first phase. Every check, from the units to the workflows, ran without the instrument.
- **The second estimate, before the code,** with its parts, in the plan's change log. It made each phase's size a decision, not a surprise.
- **The end-to-end tests, each with its own server,** on a port of its own. They ran side by side, eight at a time, and their `// Covers:` lines let a gate run only what a change reached (ADR-0061).
- **The workflows found what the phases' own tests didn't.** Driving whole checkpoints, W10.1 to W10.3 found:
  - the scan tab's plan stopping once the tab had been left;
  - a histo scan's marks lost as its overview was read again;
  - the Workspace's lists missing a project made since it opened;
  - a project made but not chosen, its list answered out of order;
  - a Gallery card that Safari's users couldn't open from the keyboard.
- **Mutation checks, a list a phase,** read against the tests first, then run. Nearly every phase found a test that passed for the wrong reason.

## What didn't

- **Load, on one computer.** The gates ran eight suites at once, each on two CPUs, and the tests that wait for the instrument met the debug server rendering on the CPU. Some failed now and then under that load, passing five times of five alone:
  - W3.6's values;
  - W7.2's editor;
  - W7.3's recipes in WebKit;
  - W7.4's analyses;
  - W7.5's annotations;
  - W8.6's first account in Firefox.

  Each was named and merged past, at the owner's choice; each costs a gate's rerun.
- **Windows' own limits.** Late in W10, Windows ran short of what sockets need, `net::ERR_NO_BUFFER_SPACE`, with its non-paged pool above 2 GB, and a page failed to load. The disk filled with build output again, and WSL's folder was cleaned before most Linux replays.
- **The browsers differ more than the code does.** WebKit on Windows draws a resized WebGL canvas black; Safari passes links by Tab; Firefox moves the focus out of the page past its last control; WebKit's screenshots are twice the CSS pixels and take seconds on two cores. Each cost a phase a round.
- **The build, once in a while:** twice, a first `vite build` after an edit came out broken on this PC, and the next one was sound. Its cause isn't found.
- **GitHub Actions stayed stopped** for its billing, from 2026-11-14. The local replays carried every merge (ADR-0038), but the release workflow hasn't run since 0.1.

## Lessons for M3

1. **Gate on the instrument as on the simulator:** M3's hardware tests run alone, one at a time, never side by side; keep the replays' load where it was, and the hardware's tests apart.
2. **A test that waits for the server waits as long as the server's own limit,** and the app's limits stay the user's: where a debug build can't meet one under load, the test runs the release build or less at once, not a longer limit in the app.
3. **Name the cause of a flake before naming it a flake:** the race in the projects' list was one, found only by reading the trace.
4. **Keep the second estimate,** and lower it as the app grows: the more a phase reuses, the more it is a fraction of the spec's.

## What M3 inherits

- **0.2.0,** tagged at the owner's word, by [the release checklist](../releasing.md).
- **The checklists for trying the stages by hand:** W8's (#136) and W10's, for the owner's runs.
- **The follow-ups the notes list,** among them:
  - a live recipe's outlines over the live image, which the preview can't show (W10.1);
  - one to one on a HiDPI screen through a narrow region (W10.1);
  - argon2 built optimised in the dev profile, for the tests' server (W10.1);
  - roles for accounts (W8.6);
  - moving off `chrono`, teta-wot first (P53);
  - the soak's open finding, some 10 KB a scan and stitch (P53).
- **The packages carry the app,** without CUDA, from the browser alone: no desktop app (ADR-0063).
- **M3 itself:** the real instrument, the MIChrome 20 and the grblHAL stage, brought up against the code that has run for two milestones against their twins ([plan §9](../implementation-plan.md#9-milestone-3-outline-real-hardware-and-performance)).
