#!/usr/bin/env bash
# Runs test suites and examples' smoke runs several times each, side by
# side, each run on two CPUs of its own, as on CI's two-core runners: the
# check, before a pull request, that the suites a change touches don't lean
# on a fast PC (CLAUDE.md). See scripts/README.md.
#
#   scripts/two-core.sh [--repeat N] [--jobs N] [--logs DIR] TARGET...
#
#   TARGET      a test suite, as PACKAGE:TEST (oscope-workflows:stitch, for
#               its tests/stitch.rs), or PACKAGE:lib for its unit tests; or
#               an example, by its name (ex-stitch), for its --smoke; or the
#               app's tests, as web:unit or web:e2e
#   --repeat N  how many times each runs [default: 5]
#   --jobs N    how many runs at once, each on a pair of CPUs
#               [default: 8, or as many pairs as there are]
#   --logs DIR  where to write each run's log
#               [default: target/two-core/<date-time>]
#
# It prints each run as it ends, then each target's passes and times, and
# exits with 0 only if every run passed.
set -uo pipefail

repeat=5
jobs=8
logs=""
targets=()
while [ $# -gt 0 ]; do
  case "$1" in
    --repeat) repeat=${2:?--repeat needs a number}; shift ;;
    --jobs) jobs=${2:?--jobs needs a number}; shift ;;
    --logs) logs=${2:?--logs needs a folder}; shift ;;
    -h | --help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
    *) targets+=("$1") ;;
  esac
  shift
done
if [ ${#targets[@]} -eq 0 ]; then
  echo "no targets (try --help)" >&2
  exit 2
fi

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=pinned.sh
. "$here/pinned.sh"
root=$(git rev-parse --show-toplevel) || exit 2
cd "$root" || exit 2
if [ -f "$HOME/.cargo/env" ]; then
  . "$HOME/.cargo/env"
fi
target=${CARGO_TARGET_DIR:-$root/target}
logs=${logs:-$target/two-core/$(date +%Y%m%d-%H%M%S)}
mkdir -p "$logs" || exit 2
slots=$(pinned_slots "$jobs")

# Each target built, and its runs listed, its copies together so that they
# run side by side.
: > "$logs/runs.tsv"
for name in "${targets[@]}"; do
  case "$name" in
    web:unit)
      folder=$root/web
      command="npm run -s test:unit"
      ;;
    web:e2e)
      # Against this build's server and app. One worker, as Playwright
      # chooses on two cores: it counts the computer's CPUs, not the pair.
      if ! { cargo build --locked --package oscope-server && (cd web && npm run -s build); }         > "$logs/build.log" 2>&1; then
        echo "FAIL  can't build $name: $logs/build.log"
        tail -n 20 "$logs/build.log" | sed 's/^/      /'
        exit 1
      fi
      folder=$root/web
      command="npm run -s test:e2e -- --workers=1"
      ;;
    web:*)
      echo "unknown web suite: $name (web:unit or web:e2e)" >&2
      exit 2
      ;;
    *:*)
      package=${name%%:*}
      suite=${name#*:}
      if [ "$suite" = lib ]; then
        selection=(--lib)
      else
        selection=(--test "$suite")
      fi
      if ! cargo test --locked --no-run --message-format=json --package "$package" \
        "${selection[@]}" > "$logs/build.json" 2> "$logs/build.log"; then
        echo "FAIL  can't build $name: $logs/build.log"
        tail -n 20 "$logs/build.log" | sed 's/^/      /'
        exit 1
      fi
      run=$(test_runs "$logs/build.json" | head -1)
      [ -n "$run" ] || { echo "FAIL  no test binary for $name" >&2; exit 1; }
      folder=$(cut -f2 <<< "$run")
      command=$(cut -f3 <<< "$run")
      ;;
    *)
      if ! cargo build --locked --package "$name" > "$logs/build.log" 2>&1; then
        echo "FAIL  can't build $name: $logs/build.log"
        tail -n 20 "$logs/build.log" | sed 's/^/      /'
        exit 1
      fi
      binary="$target/debug/$name"
      [ ! -f "$binary.exe" ] || binary="$binary.exe"
      folder=$root
      command="$binary --smoke"
      ;;
  esac
  for ((k = 1; k <= repeat; k++)); do
    printf '%s #%d\t%s\t%s\n' "$name" "$k" "$folder" "$command" >> "$logs/runs.tsv"
  done
done

echo "$((${#targets[@]} * repeat)) runs, $slots at once, each on two CPUs; logs in $logs"
PINNED_RESULTS=()
run_pinned "$slots" "$logs" "$logs/runs.tsv"
echo
for name in "${targets[@]}"; do
  printf '%s\n' "${PINNED_RESULTS[@]}" | awk -F'\t' -v name="$name" '
    { split($1, part, " #") }
    part[1] == name {
      runs++; if ($2 == 0) passed++
      s = $3 / 1000; if (runs == 1 || s < low) low = s; if (runs == 1 || s > high) high = s
    }
    END { printf "%-40s %d of %d passed, %.1f-%.1f s\n", name, passed, runs, low, high }'
done
[ "$PINNED_FAILED" -eq 0 ]
