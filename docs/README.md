# Documentation

| Document | What it is |
|---|---|
| [Goals](goal.md) | What OScope sets out to do, in the project owner's words |
| [Implementation plan](implementation-plan.md) | Scope, the decisions made while planning, the architecture for all three milestones, the phase overviews of Milestones 1 and 2, an outline of Milestone 3, risks and points for review |
| [Milestone 1 phase specifications](milestone-1/phases.md) | The goal, scope, example, "done when" and tests for each phase: U01–U06 (upstream `teta-wot`), and P00–P54 with P02b |
| [Architecture decision records](adr/README.md) | One record per significant decision, numbered across the project |
| [Milestone 1 implementation notes](milestone-1/notes/README.md) | One file per phase: what was built, how to try it, deviations and follow-ups |
| [Milestone 2 phase specifications](milestone-2/phases.md) | The goal, scope, try-it, "done when" and tests for each phase of the app: W0.1–W10.4 |
| [Milestone 2 implementation notes](milestone-2/notes/README.md) | One file per phase, with its screenshots |
| [Manual tests of W2–W4](milestone-2/manual-tests-W2-W4.md) | A checklist for trying each phase of the app's stages W2 to W4 by hand, in the app itself, with Checkpoints G and H |
| [Manual tests of P16–P23](milestone-1/manual-tests-P16-P23.md) | A checklist for trying each phase from P16 to P23 by hand, from its example |
| [Manual tests of P24–P34](milestone-1/manual-tests-P24-P34.md) | A checklist for trying each phase from P24 to P34 by hand, from its example, and Checkpoints B and C |
| [Manual tests of P35–P43](milestone-1/manual-tests-P35-P43.md) | A checklist for trying each phase from P35 to P43 by hand, from its example, and Checkpoints D and E |
| [Manual tests of P44–P54](milestone-1/manual-tests-P44-P54.md) | A checklist for trying each phase from P44 to P54 by hand, from its example, with Checkpoint F, the packages, and what needs an administrator or a computer that may be rebooted |
| [Manual tests of W0–W1](milestone-2/manual-tests-W0-W1.md) | A checklist for trying each phase of the app's stages W0 and W1 by hand, in the app itself, in both themes |
| [Milestone 1 retrospective](milestone-1/retrospective.md) | A look back at Milestone 1: what it delivered, what its numbers say, what went well and what didn't, and what M2 inherits |
| [Hardware](hardware.md) | The real instrument: camera, SDK, stage controller, lights and host. Written in P02b; M3's bring-up keeps it up to date |
| [The live view's baseline](performance/live-view-baseline.md) | How a frame gets from the simulated camera to the page, how the camera makes its image, and each step's time on the owner's laptop, on the CPU and with CUDA, before optimising. With what to optimise first |
| [The simulated camera's image on the GPU](performance/gpu-simulator.md) | The phase that moves the simulated camera's image to the GPU: its spec, the owner's decisions, how it agrees with the CPU, and its measurements against the baseline |
| [The live view at 14.5 fps](performance/live-view-fps.md) | Why the live view ran at 7.2 fps with the camera on the GPU, the camera's mode that lets it run at 14.5, the stage's lock taken out of each frame, and the measurements |
| [Releasing](releasing.md) | The checklist before a release's tag, and what the release workflow does with it |
| [Deploying on Ubuntu 24.04](deploying-on-ubuntu.md) | Line by line: the accounts, the build and its tests, the package, the service, and its smoke test, on the computer it serves |
| [Working rules](../CLAUDE.md) | How phases are delivered, and the rules every change follows |

See the plan's [documentation conventions](implementation-plan.md#11-documentation-conventions) for how these documents are organised.
