# P00: Docs scaffolding, working rules, foundational ADRs

- Status: In review
- Branch / PR: `m1/p00-docs-scaffolding` / [#1](https://github.com/lucaswewa/OScope/pull/1)
- Size: estimated S (config 30, docs not counted) · measured 35 hand-written lines (`.gitattributes` 11, the pull request template 24). The rest is documentation

## Summary

This phase sets up the documentation system and working rules that every later phase uses, and records the four decisions that frame the project ([spec](../phases.md#p00)). There is no code.

## What was built

| File | What |
|---|---|
| `CLAUDE.md` | The working rules: how a phase is delivered, the clean room, dependencies, unsafe code, platforms, line endings, GPU and hardware tests, test hygiene, ports, and the checks before a PR |
| `README.md` | Rewritten: what OScope is, its status, the instrument, how it's built, the milestones, links to the docs, the licence |
| `.github/pull_request_template.md` | The Definition of Done from plan §6 |
| `.gitattributes` | LF line endings for text files; binary types marked |
| `docs/README.md` | The index now links the ADRs, the notes and the working rules |
| `docs/adr/README.md`, `template.md` | The ADR index and process, and the template (plan Appendix A) |
| `docs/adr/0001` … `0004` | The foundational ADRs, *Accepted* on approval |
| `docs/milestone-1/notes/README.md`, `template.md` | The notes index and the template (plan Appendix B, plus a Hardware section) |
| `docs/implementation-plan.md` | P00 marked *In review*; the `teta-wot` path in §6 made machine-neutral; a change-log line |

The ADRs:

- **[ADR-0001](../../adr/0001-record-architecture-decisions-and-the-delivery-process.md)** records architecture decisions and the delivery process.
- **[ADR-0002](../../adr/0002-clean-room-and-dependency-licences.md)** sets out the clean room (what OScope may take from `microscope`) and the dependency licence policy.
- **[ADR-0003](../../adr/0003-platforms-windows-and-ubuntu-x86-64.md)** fixes the platforms: Windows 11 and Ubuntu LTS on x86_64.
- **[ADR-0004](../../adr/0004-teta-wot-v0-2-0-extended-upstream-first.md)** makes OScope depend on `teta-wot` v0.2.0, extended upstream first.

## How to try it

1. Open the branch on GitHub and follow the links from `README.md` → `docs/README.md` → the plan, the phase specs, the ADR index and each ADR, and the notes index. Every link should resolve, anchors included.
2. After merging, start a new pull request: its description should be prefilled with the Definition of Done.
3. `git ls-files --eol` shows `attr/text=auto eol=lf` on every text file.

## Design notes and deviations from the plan

- **`.gitattributes` is added,** though the spec doesn't list it, because `CLAUDE.md`'s LF rule needs enforcing. This machine has `core.autocrlf=true`. The repository already stores LF, so nothing is renormalised; checkouts now get LF too.
- **ADR-0003 adds two details the plan doesn't spell out:**
  - Ubuntu release builds are made on 22.04, so one `.deb` runs on both LTS versions;
  - WSL needs extra tooling (usbipd-win) to see USB devices, which matters for M3's hardware tests on Linux.
- **ADR-0004 adds two rules the plan implies:**
  - `oscope-api` uses `teta-wot`'s axum re-export rather than its own axum dependency;
  - OScope's `main` pins only released tags.
- **The notes template has a *Hardware* section,** for P02b and M3's driver phases.
- **The ADR index states a numbering rule:** ADRs the plan didn't foresee take the next number above those it reserves.
- **`goal.md` stays** at `docs/goal.md` (plan §13, point 14).

## Tests

None: docs and configuration only.

- **Links checked locally.** A throwaway script checked every relative link and `#anchor` in the repository's Markdown files, using GitHub's heading-to-anchor rule.
  - Result: 16 files, 147 links, 0 broken.
  - Mutation check: on a sample with a bad anchor and a missing file, it reported both and failed.
- **WSL is ready for U01.** `wsl -l -v` shows `Ubuntu-24.04` on WSL 2 (Ubuntu 24.04.5 LTS). It has no Rust toolchain yet.

## Follow-ups and known gaps

- **Automated link checking.** P01 should add a link check to CI (a small `xtask` or an existing checker), so docs-only changes are checked automatically.
- **Next phase:** U01, in the `teta-wot` repository.

## Clean room

`CLAUDE.md`, the README, the ADRs and the templates are written fresh for OScope. The idea of keeping working rules, ADRs and per-phase notes comes from the owner's earlier projects, which ADR-0002 allows. No text is copied.
