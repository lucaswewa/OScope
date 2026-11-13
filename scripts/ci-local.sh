#!/usr/bin/env bash
# Runs CI's checks (.github/workflows/ci.yml) on the checkout you are in, in
# the same order, and says which passed.
#
#   scripts/ci-local.sh [--cuda] [--logs DIR]
#
#   --cuda      also run the CUDA tests (OSCOPE_CUDA=1 and the cuda feature),
#               which need a GPU and the CUDA toolkit
#   --logs DIR  where to write each step's log
#               [default: target/ci-logs/<date-time>]
#
# A failing step shows the end of its log. It exits with 0 if every step
# passed, and 1 otherwise. See scripts/README.md.
set -uo pipefail

cuda=false
logs=""
while [ $# -gt 0 ]; do
  case "$1" in
    --cuda) cuda=true ;;
    --logs) logs=${2:?--logs needs a folder}; shift ;;
    -h | --help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
logs=${logs:-${CARGO_TARGET_DIR:-$root/target}/ci-logs/$(date +%Y%m%d-%H%M%S)}
mkdir -p "$logs" || exit 2
changes=$(git status --porcelain | wc -l)
echo "$root at $(git rev-parse --short HEAD), with $changes changed files; logs in $logs"

passed=0
failed=0
count=0
# step LABEL COMMAND...: runs the command, with its output in a log.
step() {
  local label=$1
  shift
  count=$((count + 1))
  local log start
  log=$(printf '%s/%02d-%s.log' "$logs" "$count" "${label//[^A-Za-z0-9_.-]/-}")
  start=$SECONDS
  if "$@" > "$log" 2>&1; then
    passed=$((passed + 1))
    printf 'ok    %-30s %4d s\n' "$label" $((SECONDS - start))
  else
    failed=$((failed + 1))
    printf 'FAIL  %-30s %4d s  %s\n' "$label" $((SECONDS - start)) "$log"
    tail -n 20 "$log" | sed 's/^/      /'
  fi
}

step "toolchain" rustup toolchain install
step "fmt" cargo fmt --all --check
step "clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
step "tests" cargo test --workspace --locked
step "docs" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
if command -v cargo-deny > /dev/null; then
  version=$(cargo deny --version 2> /dev/null)
  [ "$version" = "cargo-deny 0.20.2" ] || echo "note: CI uses cargo-deny 0.20.2, and this is $version"
  step "deny" cargo deny --locked check
else
  failed=$((failed + 1))
  echo "FAIL  deny: cargo-deny isn't installed; CI's is 0.20.2:"
  echo "      cargo install cargo-deny --version 0.20.2 --locked"
fi
step "server --version" cargo run -q -p oscope-server --locked -- --version
# Every example with a binary, headless (examples/README.md).
for manifest in examples/*/Cargo.toml; do
  [ -f "$(dirname "$manifest")/src/main.rs" ] || continue
  name=$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -1)
  step "$name --smoke" cargo run -q -p "$name" --locked -- --smoke
done
if $cuda; then
  step "CUDA tests" env OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda
fi

tests_log=$(ls "$logs"/*-tests.log 2> /dev/null | head -1)
if [ -n "$tests_log" ]; then
  suites=$(grep -c '^test result: ok' "$tests_log")
  tests=$(grep -o '^test result: ok. [0-9]* passed' "$tests_log" | awk '{s += $4} END {print s + 0}')
  echo "tests: $tests passed, in $suites suites"
fi
echo "steps: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
