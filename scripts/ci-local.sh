#!/usr/bin/env bash
# Runs CI's checks (.github/workflows/ci.yml) on the checkout you are in, in
# the same order, and says which passed. The test suites and the examples'
# smoke runs go side by side, each on two CPUs of its own, as on CI's
# two-core runners.
#
#   scripts/ci-local.sh [--cuda] [--jobs N] [--logs DIR] [--changed BASE]
#
#   --cuda      also run the CUDA tests (OSCOPE_CUDA=1 and the cuda feature),
#               which need a GPU and the CUDA toolkit
#   --jobs N    how many suites or smoke runs at once, each on a pair of CPUs
#               [default: 8, or as many pairs as there are]
#   --logs DIR  where to write each step's log
#               [default: target/ci-logs/<date-time>]
#   --changed BASE
#               only the tests that the changes since BASE reach (ADR-0061):
#               web/scripts/changed.mjs picks the Rust suites, the doc tests,
#               the examples, the unit tests and the end-to-end specs, and
#               whether the workflows run; the other checks run in full, and
#               a step not reached says skip
#
# A failing step shows the end of its log. It exits with 0 if every step
# run passed, and 1 otherwise. See scripts/README.md.
set -uo pipefail

cuda=false
logs=""
jobs=8
base=""
while [ $# -gt 0 ]; do
  case "$1" in
    --cuda) cuda=true ;;
    --jobs) jobs=${2:?--jobs needs a number}; shift ;;
    --logs) logs=${2:?--logs needs a folder}; shift ;;
    --changed) base=${2:?--changed needs a commit}; shift ;;
    -h | --help) sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=pinned.sh
. "$here/pinned.sh"
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
target=${CARGO_TARGET_DIR:-$root/target}
logs=${logs:-$target/ci-logs/$(date +%Y%m%d-%H%M%S)}
mkdir -p "$logs" || exit 2
slots=$(pinned_slots "$jobs")
changes=$(git status --porcelain | wc -l)
echo "$root at $(git rev-parse --short HEAD), with $changes changed files; logs in $logs"
echo "suites and smoke runs: $slots at once, each on two CPUs"

# What the change reaches (ADR-0061), or, without --changed, everything.
reach_rust=all reach_doc=all reach_examples=all reach_python=yes reach_unit=all reach_e2e=all
reach_workflows=yes
if [ -n "$base" ]; then
  if ! plan=$(node web/scripts/changed.mjs "$base" 2> "$logs/changed.log"); then
    echo "FAIL  can't tell what the changes since $base reach: $logs/changed.log" >&2
    exit 2
  fi
  printf '%s\n' "$plan" > "$logs/changed.txt"
  reached() { sed -n "s/^$1: //p" <<< "$plan"; }
  reach_rust=$(reached rust) reach_doc=$(reached doc) reach_examples=$(reached examples)
  reach_python=$(reached python) reach_unit=$(reached unit) reach_e2e=$(reached e2e)
  reach_workflows=$(reached workflows)
  echo "only what the changes since $(git rev-parse --short "$base") reach; the plan: $logs/changed.txt"
  for kind in rust doc examples python unit e2e workflows; do
    value=$(reached $kind)
    words=$(wc -w <<< "$value")
    case "$value" in
      all | none | yes | no) printf '  %-9s %s\n' "$kind" "$value" ;;
      *) printf '  %-9s %s\n' "$kind" "$words of them" ;;
    esac
  done
  sed -n 's/^why: /  because  /p' <<< "$plan" | cut -c1-160
fi
# packages_of LIST: cargo's selection for LIST, all of them or each named.
packages_of() {
  if [ "$1" = all ]; then
    echo --workspace
  else
    for name in $1; do printf -- '--package %s ' "$name"; done
  fi
}
skipped=0
# skip LABEL: a step that nothing changed reaches.
skip() {
  skipped=$((skipped + 1))
  printf 'skip  %-30s        nothing changed reaches it\n' "$1"
}

# uv, for Python extensions (ADR-0020): on PATH, or where its installers put
# it. Found, it runs the Python SDK's tests, and the tests that need Python.
uv=$(command -v uv || true)
for candidate in "$HOME/.local/bin/uv" "$HOME/.cargo/bin/uv"   "$(cygpath -u "${LOCALAPPDATA:-}" 2>/dev/null)"/Microsoft/WinGet/Packages/astral-sh.uv_*/uv.exe; do
  if [ -z "$uv" ] && [ -x "$candidate" ]; then
    uv=$candidate
  fi
done
if [ -n "$uv" ]; then
  OSCOPE_UV=$(cygpath -w "$uv" 2>/dev/null || echo "$uv")
  export OSCOPE_UV OSCOPE_PYTHON=1
fi

passed=0
failed=0
count=0
last=ok
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
    last=ok
    printf 'ok    %-30s %4d s\n' "$label" $((SECONDS - start))
  else
    failed=$((failed + 1))
    last=failed
    printf 'FAIL  %-30s %4d s  %s\n' "$label" $((SECONDS - start)) "$log"
    tail -n 20 "$log" | sed 's/^/      /'
  fi
}

# The last runs' times, so that the longest go first and none is left to
# run alone at the end.
times="$target/ci-times.tsv"
# longest_first LIST: LIST's lines, those that took longest last time first,
# and those not run before before them.
longest_first() {
  if [ -f "$times" ]; then
    awk -F'\t' 'NR == FNR { t[$1] = $2; next } { print (($1 in t) ? t[$1] : 100000) "\t" $0 }' \
      "$times" "$1" | sort -t$'\t' -k1,1nr | cut -f2-
  else
    cat "$1"
  fi
}
# keep_times: adds PINNED_RESULTS' times to the file of times.
keep_times() {
  {
    [ -f "$times" ] && cat "$times"
    printf '%s\n' "${PINNED_RESULTS[@]}" | awk -F'\t' '{ print $1 "\t" $3 }'
  } | awk -F'\t' '{ t[$1] = $2 } END { for (k in t) print k "\t" t[k] }' > "$times.new" \
    && mv "$times.new" "$times"
}

step "toolchain" rustup toolchain install
step "fmt" cargo fmt --all --check
step "clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
# The tests: their binaries built, then run side by side, each in its
# package's folder, as cargo test runs them; then the doc tests.
if [ "$reach_rust" = none ]; then
  skip "tests"
  last=skipped
else
  # shellcheck disable=SC2046
  step "tests build" cargo test $(packages_of "$reach_rust") --locked --no-run --message-format=json
fi
if [ "$last" = ok ]; then
  mkdir -p "$logs/tests"
  test_runs "$(ls "$logs"/*-tests-build.log)" > "$logs/tests.tsv"
  longest_first "$logs/tests.tsv" > "$logs/tests.order"
  count=$((count + 1))
  start=$SECONDS
  PINNED_RESULTS=()
  run_pinned "$slots" "$logs/tests" "$logs/tests.order" quiet
  keep_times
  if [ "$PINNED_FAILED" -eq 0 ]; then
    passed=$((passed + 1))
    printf 'ok    %-30s %4d s\n' "tests" $((SECONDS - start))
  else
    failed=$((failed + 1))
    printf 'FAIL  %-30s %4d s  %s of %s suites failed\n' "tests" $((SECONDS - start)) \
      "$PINNED_FAILED" "$(wc -l < "$logs/tests.tsv")"
  fi
fi
if [ "$reach_doc" = none ]; then
  skip "doc tests"
else
  # shellcheck disable=SC2046
  step "doc tests" cargo test $(packages_of "$reach_doc") --locked --doc
fi
python_sdk() { (cd python/oscope-ext && "$uv" run --locked pytest -q); }
if [ "$reach_python" = no ]; then
  skip "python sdk tests"
elif [ -n "$uv" ]; then
  step "python sdk tests" python_sdk
else
  failed=$((failed + 1))
  echo "FAIL  python: uv isn't installed, for the Python extensions' tests (CLAUDE.md)"
fi
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
# The web app (W0.1, ADR-0040): its checks, then its end-to-end tests, which
# start the server just built, each test its own.
web() { (cd web && "$@"); }
# in_web LIST: LIST's paths from the repository's root, from web/ instead.
in_web() { for file in $1; do printf '%s ' "${file#web/}"; done; }
if command -v npm > /dev/null; then
  step "web install" web npm ci
  # The API's committed types are the server's description of its API (ADR-0043).
  step "web api types" web npm run -s api:check
  step "web typecheck" web npm run -s typecheck
  step "web lint" web npm run -s lint
  case "$reach_unit" in
    none) skip "web unit tests" ;;
    all) step "web unit tests" web npm run -s test:unit ;;
    *) step "web unit tests" web node scripts/vitest.mjs related --run --passWithNoTests $(in_web "$reach_unit") ;;
  esac
  step "web build" web npm run -s build
  step "web licences" web npm run -s licences
  step "web advisories" web npm run -s advisories
  # The browsers of the Playwright the lock file names, and never another:
  # with the packages missing, a bare npx would fetch the newest from the
  # registry, whose install then removes the builds the tests need.
  step "web browsers" web npx --no-install playwright install chromium firefox webkit
  # With the visual tests, whose baselines are this computer's: OSCOPE_VISUAL=
  # in the environment, empty, leaves them out. On Linux, six workers, not
  # Playwright's half of the CPUs: under WSL its twelve failed two to six
  # load-sensitive tests a run, from 2026-11-21 (the owner's choice).
  workers=()
  [ "$(uname -s)" = Linux ] && workers=(--workers=6)
  case "$reach_e2e" in
    none) skip "web end to end" ;;
    all) step "web end to end" web env OSCOPE_VISUAL="${OSCOPE_VISUAL-1}" npm run -s test:e2e -- ${workers[@]+"${workers[@]}"} ;;
    *) step "web end to end" web env OSCOPE_VISUAL="${OSCOPE_VISUAL-1}" npm run -s test:e2e -- ${workers[@]+"${workers[@]}"} $(in_web "$reach_e2e") ;;
  esac
  # The checkpoints' workflows (W10.1), whole, whenever the change reaches
  # the app or the server: four at a time. Each is a session at the
  # instrument, on a debug server rendering its frames on the CPU; twelve
  # at once answered the app past its limits.
  case "$reach_workflows" in
    no) skip "web workflows" ;;
    *) step "web workflows" web npm run -s test:workflows -- --workers=4 ;;
  esac
else
  failed=$((failed + 1))
  echo "FAIL  web: Node isn't installed, for the app's checks (CLAUDE.md)"
fi
# Every example with a binary, headless (examples/README.md): built
# together, then their smoke runs side by side, from the repository's root.
examples=()
for manifest in examples/*/Cargo.toml; do
  [ -f "$(dirname "$manifest")/src/main.rs" ] || continue
  name=$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -1)
  # Those the change reaches, unless it reaches all.
  [ "$reach_examples" = all ] || [[ " $reach_examples " == *" $name "* ]] || continue
  examples+=("$name")
done
if [ ${#examples[@]} -eq 0 ]; then
  skip "examples"
  last=skipped
else
  step "examples build" cargo build --locked "${examples[@]/#/--package=}"
fi
if [ "$last" = ok ]; then
  for name in "${examples[@]}"; do
    binary="$target/debug/$name"
    [ ! -f "$binary.exe" ] || binary="$binary.exe"
    printf '%s --smoke\t%s\t%s --smoke\n' "$name" "$root" "$binary"
  done > "$logs/smokes.tsv"
  longest_first "$logs/smokes.tsv" > "$logs/smokes.order"
  mkdir -p "$logs/smokes"
  PINNED_RESULTS=()
  run_pinned "$slots" "$logs/smokes" "$logs/smokes.order"
  keep_times
  count=$((count + ${#examples[@]}))
  passed=$((passed + ${#examples[@]} - PINNED_FAILED))
  failed=$((failed + PINNED_FAILED))
fi
if $cuda; then
  step "CUDA tests" env OSCOPE_CUDA=1 cargo test --workspace --locked --features cuda
fi

results=$(cat "$logs"/tests/*.log "$logs"/*-doc-tests.log 2> /dev/null)
suites=$(grep -c '^test result: ok' <<< "$results")
tests=$(grep -o '^test result: ok. [0-9]* passed' <<< "$results" | awk '{s += $4} END {print s + 0}')
echo "tests: $tests passed, in $suites suites"
if [ -n "$base" ]; then
  echo "steps: $passed passed, $failed failed, $skipped not reached by the changes since $(git rev-parse --short "$base")"
else
  echo "steps: $passed passed, $failed failed"
fi
[ "$failed" -eq 0 ]
