# ADR-0001: Record architecture decisions, and the delivery process

- Status: Accepted
- Date: 2026-11-11
- Phase: P00

## Context

OScope is large. Three milestones (backend, frontend, real hardware) are built in about a hundred small phases, by an implementer working across many sessions, and reviewed by one owner.

- **Early decisions shape everything after them.** The frame model, the hardware abstraction, the extension ABI and the data format are examples. Later phases need to read what was decided and why, without re-deriving it from code or chat history.
- **The plan is a living document,** edited as phases land, so it is a poor place to keep the reasons behind decisions.
- **Every change must be one the owner can review in a sitting,** and `main` must always work.

## Decision

**Architecture decision records**

- Significant decisions are recorded as ADRs in `docs/adr/`, as the [ADR index](README.md) describes:
  - numbered across the project and never reused;
  - written in the phase that makes the decision;
  - accepted when that phase's pull request is approved;
  - never edited afterwards except for their status;
  - superseded by a new ADR when a decision changes.
- The plan's §10 reserves numbers for the ADRs it expects.

**The delivery process** is the one in [plan §6](../implementation-plan.md#6-how-phases-work):

- **One phase at a time:** one branch and one pull request each.
- **The owner approves every pull request** before it is merged. The next phase starts from the updated `main`.
- **Every pull request carries** the phase's code, tests, examples, notes and ADRs, and updates the phase's status in the plan and the notes index.
- **Size budgets** in hand-written changed lines: S ≤ 300, M ≤ 600, L ≤ 900. Phases are estimated in four parts (code, tests, examples, glue) and split before they start if they would exceed the budget.
- **The Definition of Done** is the pull request template, `.github/pull_request_template.md`.
- **Spec-first phases** write a behaviour spec, with its literature sources, before the code.
- **Checkpoints** close their stage with a demo script.
- **Upstream `teta-wot` phases** follow the same rules in that repository. Their notes are kept here ([ADR-0004](0004-teta-wot-v0-2-0-extended-upstream-first.md)).

## Consequences

- **Gains:**
  - Decisions can be traced to the phase that made them, and their reasons survive changes to the plan and new sessions.
  - Pull requests stay small enough to review properly, and `main` always works.
- **Costs:**
  - Every phase spends some effort on notes and ADRs.
  - Strict sequencing means waiting for each review.
- **Budgets need honest estimates.** Earlier experience with a similar process showed that tests (about a third of the lines), styling and glue are the parts most often under-counted.

## Alternatives considered

- **Decisions recorded only in the plan:** edits to a living document lose the reasons, and the plan grows too long to read.
- **ADR numbers per milestone:** cross-references become ambiguous ("ADR-0003 of which milestone?").
- **One pull request per stage:** fewer reviews, but each is too large to read in one sitting. The owner chose phase-sized pull requests while planning (D3).
