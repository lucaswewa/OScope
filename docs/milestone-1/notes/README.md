# Milestone 1 implementation notes

One file per phase, written for the reviewer and for whoever works on the code next: what was built, how to try it, deviations from the plan, tests, follow-ups and the clean-room check.

- **OScope phases** write `PNN-short-name.md` in the phase's own pull request.
- **Upstream `teta-wot` phases** write `UNN-short-name.md` in a small OScope pull request of their own.
- **Every file starts from [the template](template.md).**
- **Status runs** In review → Done. The next phase's pull request marks the previous one *Done*, both here and on its notes' Status line.

| Phase | Title | Status | Notes |
|---|---|---|---|
| P00 | Docs scaffolding, working rules, foundational ADRs | Done | [P00-docs-scaffolding.md](P00-docs-scaffolding.md) |
| U01 | Linux support, Ubuntu CI, licence metadata (`teta-wot`) | Done | [U01-linux-ci.md](U01-linux-ci.md) |
| U02 | Path parameters for endpoints (`teta-wot`) | Done | [U02-path-params.md](U02-path-params.md) |
| U03 | Router composition, custom listeners, HTTP/2 (`teta-wot`) | Done | [U03-router-composition.md](U03-router-composition.md) |
| U04 | Binary WebSocket streams (`teta-wot`) | In review | [U04-binary-streams.md](U04-binary-streams.md) |
