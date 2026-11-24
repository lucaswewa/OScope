# ADR-0063: No desktop app

- Status: Accepted
- Date: 2026-11-23
- Phase: none: the owner's choice, after W7

## Context

- **Milestone 2 planned a desktop app** in its stage W9 (plan §8): the same Vue 3 app in a Tauri 2 shell, starting and watching the server as its sidecar (ADR-0015), with a local fast path for live frames through shared memory and for deep-zoom tiles read from disk, and an installer a platform. Its five phases came to some 6,800 lines by their first estimates, with three ADRs reserved for them, 0056 to 0058, which were never written.
- **The app works in a browser,** on the server's own origin or another's (W1.3). Every tab of goal.md is in it by W7, and the server already runs as a service with M1's packages (ADR-0033).
- **After W7, the owner chose to skip W9 and drop it:** no desktop app is planned.

## Decision

- **OScope's frontend is the web app alone,** served by `oscope-server` and opened in a browser, on the instrument's computer or another. There is no Tauri build, no `desktop/` folder, and no desktop installer.
- **Stage W9 and Checkpoint M are dropped** from Milestone 2, and ADRs 0056, 0057 and 0058 are not written. W10 no longer depends on them:
  - **W10.3's budgets** are set from what W1.5 and W6.4 measured, without W9.3's shared-memory preview;
  - **W10.4** depends on W10.3 alone, and ships the app in Milestone 1's packages, which the service serves at `/`.
- **What M1 built for a desktop app stays in the server,** unused by any app:
  - ADR-0015's sidecar contract, whose exit codes and service mode the packages use, and its shared-memory preview ring, which `ex-shm-reader` reads;
  - P49's per-boot token for a local client, which any local process holding it may use.
- **The app's host adapter stays** (ADR-0046): one interface for storage, files, the clipboard and notifications, with the browser's as its only host. Its `can`, the desktop app's abilities, `sharedMemoryPreview` and `tilesFromDisk`, is false in both: only the developers' host page shows it.
- **WebKit stays among the browsers** that run the main path's tests: it is Safari's engine, as well as the webview the desktop app would have used on Ubuntu.

## Consequences

- **Easier:** Milestone 2 is five phases shorter. No webview's quirks, no installer per platform, no Rust sidecar plumbing in the app, and one build to test.
- **Harder:** no fast path. The live view and deep zoom go through HTTP and WebSockets, on the same computer as elsewhere, so W1.5's and W6.4's measures are the app's budgets.
- **Living documents change:** `CLAUDE.md`, `goal.md`, the plan, the specs, the web app's README and the notes' template no longer promise a desktop app. Notes and ADRs written before this one say what was planned then, and are left as they were; the index's rows of ADR-0015, ADR-0033 and ADR-0046, which planned for one, point here. So does `packaging/README.md`.
- **A desktop app could come back** as a new stage, its specs drawn again from W9's in the history of `phases.md`, with this ADR superseded.

## Alternatives considered

- **Defer W9 to a Milestone 2.5,** its specs kept: offered to the owner, who chose to drop it.
- **Move W9 to Milestone 3,** beside the real instrument's work: offered too, and not chosen.
