#!/usr/bin/env bash
# Builds OScope's .deb from this source tree, on Ubuntu, as its operator does
# in docs/deploying-on-ubuntu.md:
#
# - the server's release build, and the web app's (W10.4);
# - the tests, `cargo test --workspace`;
# - the third-party notices, installing cargo-about 0.9.3 if it isn't;
# - the .deb, copied to /var/tmp for the administrator to install.
#
#   bash packaging/linux/build-from-source.sh [--no-tests]
#
# --no-tests skips the tests, which build the whole workspace a second time,
# for debugging. It ends with the line that installs the .deb.
set -euo pipefail

tests=yes
case "${1:-}" in
    "") ;;
    --no-tests) tests=no ;;
    *) echo "usage: bash packaging/linux/build-from-source.sh [--no-tests]" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/../.."

# What it needs, and how to get what is missing.
missing=()
for tool in cc pkg-config fakeroot dpkg-deb git; do
    command -v "$tool" >/dev/null || missing+=("$tool")
done
if [ ${#missing[@]} -gt 0 ]; then
    echo "build-from-source: no ${missing[*]}. The administrator installs them with:" >&2
    echo "  sudo apt install build-essential pkg-config fakeroot git" >&2
    exit 1
fi
if ! command -v npm >/dev/null; then
    echo "build-from-source: no npm, for the web app. Install Node.js 24 with nvm, for this account:" >&2
    echo "  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash" >&2
    echo "  . \"\$HOME/.nvm/nvm.sh\" && nvm install 24" >&2
    exit 1
fi
if ! command -v cargo >/dev/null; then
    echo "build-from-source: no cargo. Install Rust for this account with:" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal" >&2
    echo "  . \"\$HOME/.cargo/env\"" >&2
    exit 1
fi

step() { echo "== build-from-source: $*"; }
step "$(git log -1 --format='%h %s')"
step "the server's release build"
cargo build --release --locked -p oscope-server
step "the web app's build"
npm --prefix web ci --no-fund --no-audit
npm --prefix web run build
if [ "$tests" = yes ]; then
    step "the tests"
    # Without debug information and incremental caches, which took nine
    # tenths of their 75 GB on disk. They run as CI runs them otherwise.
    CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --workspace --locked
fi
step "the third-party notices"
if [ "$(cargo about --version 2>/dev/null || true)" != "cargo-about 0.9.3" ]; then
    cargo install cargo-about --version 0.9.3 --locked --features cli
fi
notices=$(bash packaging/notices/make.sh | tail -1)
step "the .deb"
deb=$(bash packaging/linux/build-deb.sh target/release/oscope-server "$notices" | tail -1)
# /var/tmp, so that apt's own account can read it too; unlike /tmp, it
# survives a restart. A file of that name that another account left there is
# kept: then install it from target/deb.
if cp -f "$deb" /var/tmp/ 2>/dev/null; then
    deb="/var/tmp/$(basename "$deb")"
else
    deb="$(pwd)/$deb"
fi
echo
echo "build-from-source: ok. The administrator installs it with:"
echo "  sudo apt install $deb"
