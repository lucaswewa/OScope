# Milestone 2 implementation notes

One file per phase, written for the reviewer and for whoever works on the code next: what was built, how to try it, screenshots, deviations from the plan, tests, follow-ups and the clean-room check.

- **Each phase** writes `W3.2-short-name.md` in its own pull request, from [the template](template.md).
- **Its screenshots,** in both themes, go in `shots/`, named after the phase.
- **Status runs** Planned → In review → Done. The next phase's pull request marks the previous one *Done*, both here and on its notes' Status line.
- **The specs** are in [phases.md](../phases.md), and the overview in [the plan's §8](../../implementation-plan.md#8-milestone-2-phase-overview).

| Phase | Title | Status | Notes |
|---|---|---|---|
| W0.1 | The web workspace and its checks | Done | [W0.1-web-workspace.md](W0.1-web-workspace.md) |
| W0.2 | Design tokens, themes and icons | Done | [W0.2-tokens-themes-icons.md](W0.2-tokens-themes-icons.md) |
| W0.3 | Components I: actions and inputs | Done | [W0.3-components-inputs.md](W0.3-components-inputs.md) |
| W0.4 | Components II: overlays and feedback | Done | [W0.4-components-overlays.md](W0.4-components-overlays.md) |
| W0.5 | Components III: layout and data | Done | [W0.5-components-data.md](W0.5-components-data.md) |
| W0.6 | Forms from JSON Schema | Done | [W0.6-schema-forms.md](W0.6-schema-forms.md) |
| W0.7 | Charts | Done | [W0.7-charts.md](W0.7-charts.md) |
| W1.1 | Generated API types and the typed client | Done | [W1.1-api-types.md](W1.1-api-types.md) |
| W1.2 | Things, live properties and events | Done | [W1.2-live-things.md](W1.2-live-things.md) |
| W1.3 | Connection, login and the control lease | Done | [W1.3-connection.md](W1.3-connection.md) |
| W1.4 | The host adapter | Done | [W1.4-host-adapter.md](W1.4-host-adapter.md) |
| W1.5 | The preview client | Done | [W1.5-preview-client.md](W1.5-preview-client.md) |
| W2.1 | The shell and navigation | Done | [W2.1-shell.md](W2.1-shell.md) |
| W2.2 | Status, connection and lease in the shell | Done | [W2.2-status.md](W2.2-status.md) |
| W3.1 | The live viewer | Done | [W3.1-live-viewer.md](W3.1-live-viewer.md) |
| W3.2 | Camera controls | Done | [W3.2-camera-controls.md](W3.2-camera-controls.md) |
| W3.3 | Stage controls | Done | [W3.3-stage-controls.md](W3.3-stage-controls.md) |
| W3.4 | Illumination and the objective | Done | [W3.4-illumination.md](W3.4-illumination.md) |
| W3.5 | Overlays I: crosshair, ROI, grid and profile | Done | [W3.5-overlays.md](W3.5-overlays.md) |
| W3.6 | Overlays II: histogram, saturation, false colour and the look | Done | [W3.6-overlays-ii.md](W3.6-overlays-ii.md) |
| W3.7 | Autofocus, capture and the live recipe | Done | [W3.7-autofocus-capture-recipe.md](W3.7-autofocus-capture-recipe.md) |
| W3.8 | The free-roam mosaic and the tile viewer | Done | [W3.8-free-roam-mosaic.md](W3.8-free-roam-mosaic.md) |
| W4.1 | Jobs | Done | [W4.1-jobs.md](W4.1-jobs.md) |
| W4.2 | Logging | Done | [W4.2-logging.md](W4.2-logging.md) |
| W4.3 | About, Power and the status in full | Done | [W4.3-about-power.md](W4.3-about-power.md) |
| W5.1 | Scan plans | Done | [W5.1-scan-plans.md](W5.1-scan-plans.md) |
| W5.2 | Running a scan | Done | [W5.2-running-a-scan.md](W5.2-running-a-scan.md) |
| W5.3 | The histo scan | Done | [W5.3-histo-scan.md](W5.3-histo-scan.md) |
| W5.4 | Z-scans, stacks and positions | Done | [W5.4-zscans-stacks-positions.md](W5.4-zscans-stacks-positions.md) |
| W6.1 | Projects | Done | [W6.1-projects.md](W6.1-projects.md) |
| W6.2 | A project's records | Done | [W6.2-records.md](W6.2-records.md) |
| W6.3 | Imports, exports and uploads | Done | [W6.3-files.md](W6.3-files.md) |
| W6.4 | The deep-zoom viewer | Done | [W6.4-deep-zoom.md](W6.4-deep-zoom.md) |
| W7.1 | Recipes over the API | Done | [W7.1-recipes-api.md](W7.1-recipes-api.md) |
| W7.2 | The graph editor: the canvas | In review | [W7.2-graph-canvas.md](W7.2-graph-canvas.md) |
| W7.3 | The graph editor: parameters and the library | Planned | — |
| W7.4 | Analyses and results | Planned | — |
| W7.5 | Annotations and measurements | Planned | — |
| W7.6 | Cells and stains | Planned | — |
| W7.7 | 3D: surfaces | Planned | — |
| W7.8 | 3D: volumes and slices | Planned | — |
| W7.9 | Reports | Planned | — |
| W8.1 | System, display and devices | Planned | — |
| W8.2 | Calibration sets | Planned | — |
| W8.3 | The calibration wizard | Planned | — |
| W8.4 | Simulated samples | Planned | — |
| W8.5 | Extensions and their panels | Planned | — |
| W8.6 | Accounts | Planned | — |
| W9.1 | The Tauri shell | Planned | — |
| W9.2 | The sidecar | Planned | — |
| W9.3 | The shared-memory preview | Planned | — |
| W9.4 | Tiles from disk | Planned | — |
| W9.5 | Installers | Planned | — |
| W10.1 | Workflows end to end | Planned | — |
| W10.2 | Visual baselines and accessibility | Planned | — |
| W10.3 | Front-end budgets | Planned | — |
| W10.4 | The app in the packages, and the look back | Planned | — |
