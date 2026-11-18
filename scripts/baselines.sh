#!/usr/bin/env bash
# Makes the visual tests' baselines again, for the system it runs on: after a
# change that is meant to change how the app looks (ADR-0041). Look at what
# it wrote before committing it. See web/README.md.
#
#   scripts/baselines.sh [PLAYWRIGHT'S ARGUMENTS...]
#
# It needs what the end-to-end tests need: the packages, the built app, the
# browsers and a server. On the owner's Windows PC, Linux's baselines are made
# under WSL, from the same checkout:
#
#   wsl -d Ubuntu-24.04 -- bash /mnt/c/repos/OScope/scripts/baselines.sh
#
# There, the server is the one the Linux replay built, unless
# OSCOPE_SERVER_BIN names another: the checkout's own is Windows'.
set -euo pipefail

cd "$(dirname "$0")/../web"
replayed=$HOME/target/oscope-ci/debug/oscope-server
if [ -z "${OSCOPE_SERVER_BIN:-}" ] && [ -z "${CARGO_TARGET_DIR:-}" ] \
  && grep -qi microsoft /proc/version 2> /dev/null && [ -x "$replayed" ]; then
  export OSCOPE_SERVER_BIN=$replayed
fi
OSCOPE_VISUAL=1 npx --no-install playwright test --project chromium --grep @visual --update-snapshots "$@"
git status --short tests/e2e/baselines
