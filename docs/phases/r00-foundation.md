# R00 Clean migration foundation

- Status: Complete
- Date: 2026-11-22
- Branch: `codex/linux-container-rebuild`
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a` on `main`

The owner approved the Linux Docker/WSL2 architecture, factory flow, and a clean
branch with gradual ports. The branch retains source ancestry and removes the
legacy tracked implementation from its working tree. `main` is unchanged.

Saved the architecture, migration phases, factory/service workflow, ADR 0062 and
branch-specific working rules. Kept the MIT license, LF policy and pinned Rust
toolchain. Application code and frontend changes begin only in their port phases.

Validation: verify main's pinned commit, document links, clean branch contents,
and `git diff --check` before committing. R01 environment verification is recorded
separately and must not be inferred from the planning commit.
