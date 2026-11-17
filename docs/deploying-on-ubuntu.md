# Deploying OScope on Ubuntu 24.04

How to build, test and run OScope's server as a service on an Ubuntu 24.04 computer, line by line. When you're done:

- **The service `oscope`** runs OScope's server as the system account `oscoped`. It serves this computer alone, on `http://127.0.0.1:8210/`.
- **The operator `labuser`** builds and tests new versions, starts, stops and restarts the service, reads its log and its data, changes its configuration, and uses the serial ports, all without sudo.
- **The administrator,** you, with sudo, installs each version that `labuser` builds: installing a package runs as root.

The service is the .deb's ([`packaging/README.md`](../packaging/README.md)): the program is `/usr/bin/oscope-server`, its configuration `/etc/oscope/oscope.toml`, and its data `/var/lib/oscope`. Its devices are simulated until M3 brings the instrument's drivers.

## What you need

- **Ubuntu 24.04 LTS on x86_64,** with access to the internet: Ubuntu's packages, Rust, crates.io and GitHub.
- **An administrator's account,** with sudo.
- **Room and time** for the first build with its tests: ⟨measured⟩ of disk, and ⟨measured⟩ on a 24-thread computer. Fewer cores take longer.
- **Your GitHub account,** to give the computer a deploy key: OScope's repository is private.

Each block of commands runs as one account, in order. `sudo` asks for the administrator's password.

## 1. The packages and the accounts

As the administrator:

```sh
sudo apt update
sudo apt install -y build-essential pkg-config fakeroot git curl jq
sudo adduser labuser
sudo adduser --system --group --home /var/lib/oscope --no-create-home --shell /usr/sbin/nologin oscoped
sudo usermod -aG oscoped,systemd-journal,dialout labuser
```

- **The packages:** `build-essential`, `pkg-config` and `fakeroot` build the server and its package, `curl` installs Rust, and `jq` reads the server's answers in the smoke test.
- **`adduser labuser`** asks for its password, then for its name and the like, which may stay empty.
- **`oscoped`** is the service's account. The .deb would make it; making it first lets `labuser` join its group now. `adduser` notes that `/var/lib/oscope` doesn't exist yet: the .deb makes it.
- **`labuser`'s groups:**
  - `oscoped` makes it an operator, who may start, stop and restart the service, change its configuration and read its data;
  - `systemd-journal` lets it read the service's log;
  - `dialout` lets it use the serial ports, where the stage's controller, grblHAL, comes in M3: to look after the controller itself, as with grblHAL's `$$` settings. Stop the service first, since one program at a time can use a serial port.
- **The service** may use the serial ports too: its unit adds the group `dialout` to its own, without changing the account `oscoped`.

## 2. The source

Log in as `labuser`: over SSH, `ssh labuser@<this computer>`, or from the administrator's session, `sudo -iu labuser`. A new login takes the groups of step 1.

The computer reads OScope's repository with a deploy key: a key of its own, which reads that repository and nothing else. As `labuser`:

```sh
mkdir -p -m 700 ~/.ssh
ssh-keygen -t ed25519 -N "" -C "labuser@$(hostname) OScope deploy key" -f ~/.ssh/oscope_deploy
cat ~/.ssh/oscope_deploy.pub
```

Then, in a browser, as yourself: in the repository's **Settings → Deploy keys → Add deploy key** on GitHub, paste the line that `cat` printed. Name it after the computer, and leave **Allow write access** off.

As `labuser` again:

```sh
cat >> ~/.ssh/config <<'EOF'
Host github-oscope
    HostName github.com
    User git
    IdentityFile ~/.ssh/oscope_deploy
    IdentitiesOnly yes
EOF
chmod 600 ~/.ssh/config
ssh -T github-oscope
git clone github-oscope:lucaswewa/OScope.git ~/OScope
cd ~/OScope
git switch --detach v0.1.1
```

- **`ssh -T`** asks, the first time, whether to trust GitHub's host key. Compare its fingerprint with [those GitHub publishes](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints), then answer `yes`. It then says *You've successfully authenticated*: the key works.
- **`git switch --detach v0.1.1`** takes the release, by its tag. `main` has what is newest, released or not.

## 3. Rust

As `labuser`:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
```

This installs rustup in `labuser`'s home, and later logins find `cargo` on their own. The first build installs the Rust version that the repository names, in `rust-toolchain.toml`.

## 4. Build, test and package

As `labuser`:

```sh
cd ~/OScope
bash packaging/linux/build-from-source.sh
```

[`build-from-source.sh`](../packaging/linux/build-from-source.sh):

1. builds the server for release;
2. runs the tests, `cargo test --workspace`: ⟨measured⟩. `--no-tests` skips them;
3. makes the third-party notices, installing cargo-about the first time;
4. packages the .deb, and copies it to `/var/tmp`, which a restart keeps.

It ends with the line that installs the package:

```text
build-from-source: ok. The administrator installs it with:
  sudo apt install /var/tmp/oscope_0.1.1_amd64.deb
```

## 5. Install

As the administrator:

```sh
sudo apt install /var/tmp/oscope_0.1.1_amd64.deb
systemctl status oscope --no-pager
```

The package installs the server, its configuration, its service and its polkit rules, then enables and starts the service: `systemctl status` says *active (running)*.

**Over OScope 0.1.0's package,** whose account was `oscope`, it upgrades it, and what that account wrote becomes `oscoped`'s. The old account can then go: `sudo deluser --system oscope`.

## 6. The smoke test

As `labuser`, while no one uses the instrument, since it restarts the service twice:

```sh
cd ~/OScope
bash packaging/linux/smoke-test.sh
```

[`smoke-test.sh`](../packaging/linux/smoke-test.sh) checks the service as installed:

```text
⟨measured⟩
```

- **The service:** enabled, running as `oscoped` with the group `dialout`, and answering as the version installed.
- **The instrument:** each simulated device, the disks and the jobs say `ok`, but the stage may wait to be homed, as after each start: the test moves nothing. The GPU's state is shown, not checked: the package is built without CUDA.
- **A capture:** a frame from the simulated camera, into a new project, which is then deleted.
- **Restarts:** asked to through its API, the server exits with code 75 and systemd starts it again. Then `labuser` restarts it with `systemctl`, without sudo.
- **The operator's rights:** `labuser` reads the service's log and its data, and may change its configuration and use the serial ports.

A check that fails says `FAIL`, why, and where the service's log is.

## Reaching it from another computer

The service serves this computer alone. From another, tunnel its port over SSH:

```sh
ssh -N -L 8210:127.0.0.1:8210 labuser@<this computer>
```

While that runs, `http://127.0.0.1:8210/docs` on the other computer shows the service's API. If that computer serves OScope itself, use another local port, such as `-L 8310:127.0.0.1:8210`.

## Day to day, as `labuser`

| To | Run |
|---|---|
| See whether it runs | `systemctl status oscope --no-pager` |
| Follow its log | `journalctl -u oscope -f` |
| Restart, stop or start it | `systemctl restart oscope`, `systemctl stop oscope`, `systemctl start oscope` |
| Change its configuration | `nano /etc/oscope/oscope.toml`, then `systemctl restart oscope` |
| See its projects and the free space | `ls /var/lib/oscope/data/projects`, `df -h /var/lib/oscope` |

- **The configuration's keys** are in [the server's README](../crates/oscope-server/README.md#the-configuration-file). The `settings` Thing changes the file too, keeping the one before as `oscope.toml.bak`. Edit it with an editor that writes the file in place, as `nano` and `vim` do, so that it stays the group's.
- **The projects** are the instrument's results. The service writes them, and its operators may read and copy them. Deleting them goes through the API.

## A new version

As `labuser`, with the new release's tag:

```sh
cd ~/OScope
git fetch --tags
git switch --detach v0.1.2
bash packaging/linux/build-from-source.sh
```

Then, as the administrator, the line it ends with, such as `sudo apt install /var/tmp/oscope_0.1.2_amd64.deb`. Then, as `labuser`, the smoke test.

If you changed the configuration and the new version's differs, apt asks which to keep. Keep yours: `N`, the default.

## Removing it

As the administrator:

```sh
sudo apt remove oscope
sudo apt purge oscope
```

- **`remove`** stops the service, and removes it and the program.
- **`purge`** removes its configuration too.
- **The data,** `/var/lib/oscope`, and the accounts stay: the projects are the instrument's results. To remove them too, once you have copied what you need: `sudo rm -rf /var/lib/oscope`, `sudo deluser --system oscoped` and `sudo deluser --remove-home labuser`.

## If something goes wrong

- **`build-from-source: no cc`,** or another tool: step 1's packages are missing. **No `cargo`:** step 3, or a new login.
- **`git clone` says *Permission denied (publickey)*:** the deploy key isn't on GitHub yet, or `~/.ssh/config` names another file.
- **The smoke test says an operator can't restart the service or read its log:** `labuser`'s groups take effect at its next login. `id -nG` lists `oscoped` and `systemd-journal` once they have.
- **The service doesn't start:** `journalctl -u oscope -n 50` says why. If another program has its port, change `port` in `/etc/oscope/oscope.toml`, then `systemctl restart oscope`.
