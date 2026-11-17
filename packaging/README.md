# Packaging

OScope's server, packaged (P54, [ADR-0033](../docs/adr/0033-packaging-and-release.md)):

- **for Ubuntu 22.04 and 24.04:** a .deb;
- **for Windows:** a zip, with its service's scripts.

A release builds and publishes both ([`docs/releasing.md`](../docs/releasing.md)). How the server runs as a service is P19's ([ADR-0015](../docs/adr/0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md)). The desktop app's installers replace the zip in M2 (W9).

## Ubuntu: the .deb

```sh
sudo apt install ./oscope_0.1.0_amd64.deb
```

- **Its account** is `oscope`, its configuration `/etc/oscope/oscope.toml`, and its data `/var/lib/oscope`.
- **The service `oscope`** is enabled and started, on http://127.0.0.1:8210/.
- **Its polkit rule** lets the service reboot and power off the host: P51's power actions, and nothing else.
- **Removing it:**
  - `sudo apt remove oscope` stops the service;
  - `sudo apt purge oscope` removes the configuration too;
  - the data and the account stay.

**Building it,** on Ubuntu, from a release build:

```sh
cargo build --release --locked -p oscope-server
bash packaging/notices/make.sh
bash packaging/linux/build-deb.sh target/release/oscope-server target/notices
```

- `packaging/notices/make.sh` needs cargo-about 0.9.3: `cargo install cargo-about --version 0.9.3 --locked --features cli`.
- A .deb built on Ubuntu 24.04 needs 24.04's C library. The release workflow builds it on 22.04, so that it installs on both.

### Its example: the .deb in clean Ubuntu containers

[`linux/test-deb.sh`](linux/test-deb.sh) needs Docker:

```sh
bash packaging/linux/test-deb.sh target/deb/oscope_0.1.0_amd64.deb ubuntu:24.04
```

For each image, it installs the .deb in a clean container, and checks:

- its account, its folders and whose they are, the unit, and the polkit rule in both its forms;
- that its server, run as `oscope` with its configuration, answers on `/health`;
- that purging it removes the program and the configuration, and keeps the data.

It ends with `test-deb: ok`.

## Ubuntu by hand: systemd

[`linux/oscope.service`](linux/oscope.service) runs `oscope-server` as the `oscope` account, with [`linux/oscope.toml`](linux/oscope.toml), whose folders are in `/var/lib/oscope`, the one place the unit lets it write.

- **SIGTERM stops it gracefully:** invocations cancelled, requests finished, the Things stopped and their settings saved.
- **Exit code 75 starts it again,** as a failure does, 2 s later.

To install, as root:

```sh
useradd --system --home-dir /var/lib/oscope --shell /usr/sbin/nologin oscope
install -m 755 oscope-server /usr/bin/
install -D -m 644 linux/oscope.toml /etc/oscope/oscope.toml
install -m 644 linux/oscope.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now oscope
```

### Trying it without root

[`linux/test-user-unit.sh`](linux/test-user-unit.sh) runs the unit as your own, without the root-only lines (the account, the state folder, the hardening), on port 8228 with the fake devices. Run it from the repository, with a built server:

```sh
cargo build -p oscope-server
packaging/linux/test-user-unit.sh target/debug/oscope-server
```

It checks:

- that the server starts and answers;
- that the `system` Thing's `restart` makes it exit with 75, and systemd start a new process;
- that `systemctl stop` ends it with exit code 0;
- then it removes the unit.

It ends with `test-user-unit: ok`. Under WSL, systemd must be on (`[boot] systemd=true` in `/etc/wsl.conf`).

## Windows: the zip

Unzipped, from an administrator's PowerShell, in its folder:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

- The program goes in `C:\Program Files\OScope`.
- The configuration and the data go in `C:\ProgramData\OScope`. A configuration already there is kept.
- The service `oscope` is installed and started, on http://127.0.0.1:8210/.

`uninstall.ps1` removes the service and the program. The configuration and the data stay.

**Building it,** from a release build:

```powershell
cargo build --release --locked -p oscope-server
bash packaging/notices/make.sh
powershell -File packaging\windows\package.ps1 -Server target\release\oscope-server.exe -Notices target\notices
powershell -ExecutionPolicy Bypass -File packaging\windows\test-zip.ps1 target\zip\oscope-0.1.0-windows-x86_64.zip
```

[`windows/test-zip.ps1`](windows/test-zip.ps1) checks the zip without installing the service, which needs an administrator:

- unpacked in a folder of its own, it has its files;
- its server says its version;
- with the zip's configuration, on a port the system picks, the server answers on `/health`, and keeps its data beside the configuration.

It ends with `test-zip: ok`.

## The notices

[`notices/make.sh`](notices/make.sh) gathers them in `target/notices`, for both packages:

- `THIRD-PARTY-NOTICES.html`: each crate in the server's build, and its licence's text, by cargo-about, with [`notices/about.toml`](notices/about.toml) and the template [`notices/about.hbs`](notices/about.hbs);
- `Roboto-LICENSE.txt`: the fonts the reports embed (P47);
- `typst-assets-0.15.1-NOTICE.txt`: the data that the crate typst-assets bundles. It is the crate's `NOTICE`, copied unchanged (SHA-256 `80ac46fb7d70f1c30bf0c14d3408e6a6f0f9cc9997a00b592aa0d2c8a5243a9d`).

## Windows by hand: a service

`oscope-server service install` registers the server, as described in [the server's README](../crates/oscope-server/README.md#service-mode). It runs as LocalSystem, and the service manager starts it again 2 s after it fails or exits with 75.

### Trying it

[`windows/service-test.ps1`](windows/service-test.ps1), from an administrator's PowerShell, with a built server:

```powershell
cargo build -p oscope-server
powershell -ExecutionPolicy Bypass -File packaging\windows\service-test.ps1
```

It installs the service `oscope-test`, on port 8229, with the fake devices and its folder in `C:\ProgramData\oscope-test`. Then it checks:

- that the service starts and answers;
- that the `system` Thing's `restart` makes it exit with 75, and the service manager start a new process;
- that `Stop-Service` ends it with exit code 0;
- then it removes the service and the folder.

It ends with `service-test: ok`, and leaves a transcript in `%TEMP%\oscope-service-test.log`.

**The local token** of a service running as LocalSystem is readable by LocalSystem alone, so even an administrator has to take the folder over to delete it. The script does.

## Not yet

- **The desktop app and a service:** a service's token and preview ring are its account's alone, and on Windows its ring is in session 0's namespace. The installers will grant the desktop app's users a group, and name the ring in `Global\` (W9).
- **Packages:** `.deb`, MSI and NSIS come with the desktop app (W9).
