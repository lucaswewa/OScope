# ADR-0033: Packaging and release

- Status: Accepted
- Date: 2026-11-17
- Phase: P54

## Context

- **M1 needs installable builds** for its two platforms (ADR-0003): Windows 11, and Ubuntu 22.04 and 24.04, on x86_64.
- **The server runs as a service** (ADR-0015, P19): under Windows' service manager, through `oscope-server service install`, and under systemd, through `oscope.service`.
- **Two phases need the packaging:**
  - P51's power actions need a polkit rule on Linux (ADR-0032 §6);
  - P52's `settings` Thing writes the configuration file.
- **What is shipped** carries its licences (ADR-0002): each crate's, and those of the fonts and data the server embeds.
- **The Tauri installer** replaces the Windows package in M2 (W9).
- **A release is a tag,** which only the owner pushes.

## Decision

**1. Windows: a zip,** `oscope-X.Y.Z-windows-x86_64.zip`. It holds:
- the server, its configuration, a README, the licence and the notices;
- `install.ps1`, run as an administrator:
  - the program goes in `C:\Program Files\OScope`;
  - the configuration, unless one is already there, and the data go in `C:\ProgramData\OScope`;
  - the service `oscope` is installed and started;
- `uninstall.ps1`, which removes the service and the program. The configuration and the data stay.

**2. Ubuntu: a .deb,** `oscope_X.Y.Z_amd64.deb`, built by `packaging/linux/build-deb.sh` with `dpkg-deb`: no packaging tool beyond Ubuntu's own.
- **Its files:**
  - `/usr/bin/oscope-server`;
  - `/etc/oscope/oscope.toml`, a conffile;
  - the unit, in `/usr/lib/systemd/system`;
  - the polkit rule, in the two forms Ubuntu's two versions read: `/usr/share/polkit-1/rules.d/50-oscope.rules` (24.04) and `/var/lib/polkit-1/localauthority/10-vendor.d/50-oscope.pkla` (22.04);
  - the licence and the notices, in `/usr/share/doc/oscope`.
- **Installed (postinst):**
  - the system account and group `oscope`;
  - `/var/lib/oscope`, theirs;
  - `/etc/oscope` and the configuration, writable by the group, for the `settings` Thing;
  - the service enabled and started, where systemd runs.
- **Removed:**
  - the service is stopped and disabled (prerm);
  - purged, `/etc/oscope` goes, but `/var/lib/oscope` and the account stay: projects are the instrument's results (postrm).
- **Built on Ubuntu 22.04,** the oldest supported, so that it installs on both (`libc6 (>= 2.35)`).
- **The unit** gains `ReadWritePaths=/etc/oscope`.

**3. The notices,** which both packages carry:
- `THIRD-PARTY-NOTICES.html`, made by `cargo-about` 0.9.3: each crate in the server's build for either platform, with its licence's text. It accepts `deny.toml`'s licences.
- Beside it:
  - Roboto's licence, for the fonts the reports embed (P47);
  - typst-assets' NOTICE, for the data it bundles.

**4. The release workflow,** `.github/workflows/release.yml`, on a tag `vX.Y.Z` that matches the workspace's version:
- it builds the zip, and checks it;
- it builds the .deb on Ubuntu 22.04, and installs it in clean 22.04 and 24.04 containers;
- it publishes both, with `SHA256SUMS`, as the tag's GitHub release.

`docs/releasing.md` is the checklist that comes before the tag.

**5. The version** is the workspace's. It is in `--version`, in the packages' names, and in the `system` Thing, beside the commit.

**6. CPU builds:** the packages are built without CUDA. The GPU's paths are built from source, with `--features cuda` (ADR-0011), until M2.

## Consequences

- **Gains:**
  - One command installs the server on each platform, as a service.
  - The polkit rule ships with the server that needs it.
  - The notices are made, not written by hand.
  - A release is made from a tag, and checked before it is published.
- **Costs:**
  - The zip isn't an installer: M2's replaces it.
  - The .deb is built by hand, without lintian, and for amd64 only.
  - The packages have no GPU paths.
- **To watch:**
  - GitHub's Ubuntu 22.04 runners, which the .deb is built on;
  - polkit's two forms, until 22.04 goes.

## Alternatives considered

- **cargo-deb:** a tool to install, for a control file of a dozen lines.
- **Debian source packaging (debhelper):** more machinery than one binary needs.
- **An MSI, with WiX:** M2's Tauri installer builds the Windows one.
- **The configuration in `/var/lib/oscope`:** Debian keeps configuration in `/etc`, and `ReadWritePaths` gives the `settings` Thing its folder there.
