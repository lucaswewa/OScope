# Releasing

A release is a tag, `vX.Y.Z`, on `main`, which only the owner pushes. Pushing it runs [`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds, checks and publishes the packages ([ADR-0033](adr/0033-packaging-and-release.md)). The checklist below comes first: the workflow can't tell whether it was followed.

## 1. Choose the version

- **Milestone 1 is `0.1.0`,** the workspace's version since P01.
- **Before 1.0,** a milestone takes the next minor version (`0.2.0` for M2), and fixes between milestones a patch (`0.1.1`).

## 2. Prepare `main`

1. **Every phase is merged,** and plan §7 marks it *Done*.
2. **The workspace's version,** `version` in the root `Cargo.toml`'s `[workspace.package]`, is `X.Y.Z`, and `Cargo.lock` is up to date with it.
3. **Both local replays pass** on `main`'s commit, each run alone ([ADR-0038](adr/0038-local-ci-replays-side-by-side.md)): `scripts/ci-local.sh` on Windows, and `scripts/ci-clone.sh` under WSL.

## 3. Measure and soak the release build

1. **The budgets:** `cargo xtask perf --cuda`, on the owner's machine. Every budget it measures must be met. Keep `target/perf/report.html` for the release ([`xtask/README.md`](../xtask/README.md)).
2. **The soak:** `cargo xtask soak --minutes 240`, or the Soak workflow, started by hand. It must pass. Keep `target/soak/soak.json`.

## 4. Review what is distributed

1. **The notices:** `bash packaging/notices/make.sh`. It fails on a licence that `deny.toml` doesn't allow. A file copied into the repository since the last release, such as a font or data that the server embeds, gets its notice beside the others ([`packaging/README.md`](../packaging/README.md#the-notices)).
2. **The packages, tried:**
   - `packaging/linux/test-deb.sh`, in clean Ubuntu 22.04 and 24.04 containers;
   - `packaging/windows/test-zip.ps1`;
   - the zip's `install.ps1` and `uninstall.ps1`, from an administrator's PowerShell, on a Windows computer.

## 5. Tag

```sh
git switch main && git pull
git tag -a vX.Y.Z -m "OScope X.Y.Z"
git push origin vX.Y.Z
```

The workflow then:

1. checks that the tag is the workspace's version;
2. builds the server for Windows, packages the zip, and checks it;
3. builds the server on Ubuntu 22.04, packages the .deb, and installs it in clean Ubuntu 22.04 and 24.04 containers;
4. publishes both, with `SHA256SUMS`, as the tag's GitHub release.

Afterwards, edit the release to attach the performance report and the soak's report.

## If something goes wrong

- **The workflow fails before it publishes:** delete the tag (`git push origin :refs/tags/vX.Y.Z` and `git tag -d vX.Y.Z`), fix, and tag again.
- **A published release is broken:** don't move its tag. Fix it, and release a patch version.
- **GitHub Actions doesn't run,** as when its billing failed from 2026-11-14: the same scripts build and check the packages on the owner's machine (`packaging/README.md`). The .deb must then be built on Ubuntu 22.04, in a container or a computer of its own, to install on both.
