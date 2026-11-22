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

Verified main's pinned commit, local document links and `git diff --check` before
committing the clean foundation as `f0de2c4`. The tracked tree contains plans,
working rules, the license, LF policy and the Rust toolchain pin. Ignored legacy
build caches and recordings remain local and are not included in the new tree.
R01 environment verification is recorded separately.

## Manual test

From the WSL checkout, run:

```sh
git branch --show-current
git show --no-patch --oneline 44bc08a53f98dc29db1fa6b488a4f6269732a71a
git merge-base --is-ancestor 44bc08a53f98dc29db1fa6b488a4f6269732a71a HEAD
echo $?
python3 scripts/check-docs.py
```

Expect the rebuild branch, the pinned W6.4 source commit, ancestor-check exit code
`0`, and a passing document check. Read the architecture, migration plan, factory
flow and ADR linked from the root README and confirm their platform, fallback and
production boundaries match the approved plan. Later phases add files, so inspect
the R00 tree itself with `git ls-tree -r --name-only f0de2c4` when checking the clean
foundation. It contains plans and tooling pins rather than the legacy source.

Any missing reference, failed link, incorrect branch or inconsistent decision
fails this check. No process or data cleanup is needed.
