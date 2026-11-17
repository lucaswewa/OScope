# P54: Packaging, release workflow, M1 retrospective

- Status: In review
- Branch / PR: `m1/p54-packaging` / [#70](https://github.com/lucaswewa/OScope/pull/70)
- Size: M · estimated ~720 before it started (the .deb ~200; the zip ~145; the notices ~70; the release workflow ~110; the example and the zip's check ~105; tests ~15; glue ~10; a tenth on top; plan's change log); one pull request, at the owner's standing choice · measured SIZE

## Summary

Installable M1 builds, and a look back ([spec](../phases.md#p54); [ADR-0033](../../adr/0033-packaging-and-release.md)).

- **For Ubuntu 22.04 and 24.04, a .deb:**
  - the server, its configuration, and the systemd unit;
  - the polkit rule for P51's power actions, in both of Ubuntu's forms;
  - the account and group `oscope`, and its folders;
  - the notices.
- **For Windows, a zip:** the server, its configuration, and scripts that install and remove it as a service.
- **The third-party notices,** made by cargo-about, with the licences of the fonts and data the server embeds beside them.
- **A release workflow on tags** that builds, checks and publishes both. The version it releases is the workspace's, which `--version` and the `system` Thing already give, with the commit.
- **[The retrospective](../retrospective.md),** and [`docs/releasing.md`](../../releasing.md), the checklist before a release.
- **Not done here:** the tagged release that the spec's "done when" names. It waits for the owner's word, and for GitHub Actions, which hasn't started jobs since its billing failed on 2026-11-14.

## What was built

- **`packaging/linux/`:**
  - `build-deb.sh`: the .deb, with `dpkg-deb`;
  - `debian/postinst`, `prerm` and `postrm`;
  - `oscope.rules` and `oscope.pkla`: the polkit rule for 24.04 and 22.04;
  - `test-deb.sh`: the example;
  - the unit gains `ReadWritePaths=/etc/oscope`, for the `settings` Thing.
- **`packaging/windows/`:** `package.ps1`, `install.ps1`, `uninstall.ps1`, `oscope.toml`, `README.txt`, and `test-zip.ps1`.
- **`packaging/notices/`:** cargo-about's configuration and template, `make.sh`, and typst-assets' NOTICE, copied unchanged.
- **`.github/workflows/release.yml`**, on tags `v*`.
- **Docs:** ADR-0033; `docs/releasing.md`; `docs/milestone-1/retrospective.md`; `packaging/README.md`, which says how to install, build and try each package.
- **A test:** the packaged configurations load.

## How to try it

The .deb, on Ubuntu, with Docker:

```sh
cargo build --release --locked -p oscope-server
bash packaging/notices/make.sh
bash packaging/linux/test-deb.sh "$(bash packaging/linux/build-deb.sh target/release/oscope-server target/notices)"
```

Built under WSL's Ubuntu 24.04 and tried in a clean `ubuntu:24.04`, it printed:

```text
test-deb: ubuntu:24.04
uid=100(oscope) gid=101(oscope) groups=101(oscope)
oscope-server 0.1.0
  Ubuntu 24.04.5 LTS: ok
test-deb: ok
```

The zip, on Windows:

```powershell
bash packaging/notices/make.sh
$zip = powershell -File packaging\windows\package.ps1 -Server target\release\oscope-server.exe -Notices target\notices
powershell -ExecutionPolicy Bypass -File packaging\windows\test-zip.ps1 $zip
```

```text
oscope-server 0.1.0
… INFO teta_wot_server: serving 21 Things on http://127.0.0.1:61450
test-zip: ok
```

## Performance

None is relevant. The packages: the zip is 27 MB, and the .deb 23 MB. The servers in them, 80 MB on Windows and 104 MB on Linux, keep their symbols.

## Design notes and deviations from the plan

- **The zip isn't an installer:** M2's Tauri installer replaces it, as the spec says. `install.ps1` copies the program to `C:\Program Files\OScope` before installing the service, so that the service doesn't run from wherever the zip was unpacked.
- **The .deb is built by a script with `dpkg-deb`,** not cargo-deb or debhelper: its control file is a dozen lines, and Ubuntu has the tools.
- **Built on Ubuntu 22.04,** the oldest supported, it installs on both. Built under WSL's 24.04 here, it needs 24.04's C library, so it was tried on 24.04 alone; the workflow tries both.
- **Purging keeps the data and the account:** projects are the instrument's results. Only `/etc/oscope` goes.
- **`/etc/oscope` is the group `oscope`'s to write,** with `ReadWritePaths` in the unit: P52's `settings` Thing rewrites the configuration and keeps backups beside it. P52's notes had this as a follow-up.
- **The polkit rule in two forms:** Ubuntu 24.04's polkit reads JavaScript rules, and 22.04's reads `.pkla` files. Each ignores the other's.
- **The notices list OScope's own crates too.**
  - cargo-about can leave out unpublished crates, but teta-wot isn't published either, and its licence must be there.
  - So every crate is listed, each linked to its repository, or else to crates.io.
- **cargo-about 0.9.3** needs its `cli` feature to install: `cargo install cargo-about --version 0.9.3 --locked --features cli`. It is a tool, and isn't shipped.
- **The packages are built without CUDA** (ADR-0033 §6).

### What the mutants' reading found

MUTANTS

### Two cores

TWOCORE

## Tests

| Suite | Tests | What they show |
|---|---|---|
| `oscope-server` `config` | 11 | The packaged configurations load: the .deb's, its folders in `/var/lib/oscope`, and the zip's, its folders beside it |
| `packaging/linux/test-deb.sh` | example | Installed in a clean Ubuntu: the account, its folders and whose they are, the unit, the polkit rule in both forms; its server answers as `oscope`; purged, the program and configuration go and the data stays |
| `packaging/windows/test-zip.ps1` | example | Unpacked: its files; the server's version; it serves with the zip's configuration, on a port the system picks, keeping its data beside it |

REPLAYS

## Follow-ups and known gaps

- **The tagged release,** which P54's "done when" names, waits for the owner's word. GitHub Actions must start jobs again for the workflow to publish, or the packages must be built by hand: [`docs/releasing.md`](../../releasing.md#if-something-goes-wrong).
- **Not tried here:**
  - the zip's `install.ps1` and `uninstall.ps1`, which need an administrator;
  - the .deb under a running systemd, and the polkit rule letting the service reboot, which needs a machine that may be rebooted.

  Both are in P44–P54's manual checklist.
- **lintian** isn't run on the .deb.
- **CUDA packages,** and stripping the binaries, are for M2.

## Clean room

Nothing was copied from `microscope` or any other codebase. The maintainer scripts and the polkit rules are written from Debian's and polkit's documented formats. typst-assets' NOTICE is copied unchanged, as its licence asks.
