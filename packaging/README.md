# Packaging

How OScope's server runs as a service (P19, [ADR-0015](../docs/adr/0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md)). The desktop app's installers come with it in M2 (W9).

## Linux: systemd

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

## Windows: a service

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
