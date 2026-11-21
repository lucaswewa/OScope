#!/usr/bin/env bash
# Runs scripts/ci-local.sh on a fresh clone of a commit, checked out with LF
# line endings in a folder of its own, as CI sees the code. On Windows, run
# it under WSL, so that the clone and its build are on the Linux side.
#
#   scripts/ci-clone.sh [COMMIT] [--cuda] [--jobs N] [--changed BASE]
#
#   COMMIT    what to check [default: HEAD]; it must be committed
#   --cuda    also run the CUDA tests
#   --jobs N  how many suites or smoke runs at once, as ci-local.sh's
#   --changed BASE
#             only what the changes since BASE reach, as ci-local.sh's
#
# The clone goes to $CI_CLONE_DIR/oscope-<commit> [default: ~/ci], replacing
# an earlier clone of the same commit, and builds in $CARGO_TARGET_DIR
# [default: ~/target/oscope-ci]. A file holding a CR fails the run.
# See scripts/README.md.
set -uo pipefail

commit=HEAD
base=""
options=()
while [ $# -gt 0 ]; do
  case "$1" in
    --cuda) options+=(--cuda) ;;
    --jobs) options+=(--jobs "${2:?--jobs needs a number}"); shift ;;
    --changed) base=${2:?--changed needs a commit}; shift ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
    *) commit=$1 ;;
  esac
  shift
done

if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
here=$(cd "$(dirname "$0")" && pwd)
source=$(git -C "$here" rev-parse --show-toplevel) || exit 2
sha=$(git -C "$source" rev-parse --verify "$commit^{commit}") || exit 2
# The base as a commit, which the clone has: a branch's name may not be its.
if [ -n "$base" ]; then
  resolved=$(git -C "$source" rev-parse --verify "$base^{commit}") || exit 2
  options+=(--changed "$resolved")
fi
clone=${CI_CLONE_DIR:-$HOME/ci}/oscope-${sha:0:7}

rm -rf "$clone"
mkdir -p "$(dirname "$clone")" || exit 2
git -c core.autocrlf=false clone -q --no-local --no-checkout "$source" "$clone" || exit 2
git -C "$clone" -c core.autocrlf=false checkout -q --detach "$sha" || exit 2
files=$(git -C "$clone" ls-files | wc -l)
with_cr=$(cd "$clone" && git ls-files -z | xargs -0 grep -lI $'\r' 2> /dev/null | wc -l)
echo "a fresh clone of ${sha:0:7}: $files files, $with_cr with a CR"

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/target/oscope-ci}
echo "building in $CARGO_TARGET_DIR"
# This checkout's script, so that older commits can be checked too.
(cd "$clone" && bash "$here/ci-local.sh" ${options[@]+"${options[@]}"})
code=$?
if [ "$with_cr" -ne 0 ]; then
  echo "FAIL  $with_cr files hold a CR, though .gitattributes asks for LF"
  code=1
fi
exit "$code"
