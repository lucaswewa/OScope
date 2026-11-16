# Runs commands side by side, each limited to its own pair of CPUs, as CI's
# two-core runners are. Sourced by ci-local.sh and two-core.sh; see
# scripts/README.md.
#
# Slot k gets CPUs 2k and 2k+1. On the owner's PC, an i9-12900, CPUs 0-15
# are the eight performance cores' threads, so slots 0-7 use them all.

# pinned_slots [N]: N, or 8, but no more pairs than there are CPUs.
pinned_slots() {
  local cpus
  cpus=$(nproc 2> /dev/null || echo "${NUMBER_OF_PROCESSORS:-2}")
  local most=$((cpus / 2))
  local slots=${1:-8}
  [ "$most" -ge 1 ] || most=1
  [ "$slots" -le "$most" ] || slots=$most
  echo "$slots"
}

# pin SLOT: limits this (sub)shell, and what it starts, to CPUs 2·SLOT and
# 2·SLOT+1. On Windows, through PowerShell; children inherit the mask.
pin() {
  local first=$((2 * $1)) me=$BASHPID
  if [ -r "/proc/$me/winpid" ]; then
    local winpid
    winpid=$(cat "/proc/$me/winpid")
    powershell.exe -NoProfile -NonInteractive -Command \
      "(Get-Process -Id $winpid).ProcessorAffinity = $((3 << first))" > /dev/null
  else
    taskset -cp "$first,$((first + 1))" "$me" > /dev/null
  fi
}

# run_pinned SLOTS LOGS LIST [quiet]: runs each line of the file LIST,
# "label<TAB>folder<TAB>command and its arguments", SLOTS at a time, each in
# its folder and pinned to a slot of its own, its output in a log in LOGS.
# Prints a line for each as it ends, or with quiet only for those that fail,
# and the end of the log of each that fails. Sets PINNED_FAILED to how many
# failed, and adds "label<TAB>exit code<TAB>milliseconds" to PINNED_RESULTS.
run_pinned() {
  local slots=$1 logs=$2 list=$3 quiet=${4:-}
  local -a free=() lines=()
  local -A slot_of=() label_of=() start_of=() log_of=()
  local n=0 line label folder command pid code slot log
  PINNED_FAILED=0
  for ((slot = 0; slot < slots; slot++)); do
    free+=("$slot")
  done
  mapfile -t lines < "$list"
  lines+=("")
  for line in "${lines[@]}"; do
    # A free slot for the next, or, after the last, every run ended.
    while [ ${#free[@]} -eq 0 ] || { [ -z "$line" ] && [ ${#slot_of[@]} -gt 0 ]; }; do
      wait -n -p pid
      code=$?
      local took=$(((${EPOCHREALTIME//[.,]/} - start_of[$pid]) / 1000))
      PINNED_RESULTS+=("${label_of[$pid]}"$'\t'"$code"$'\t'"$took")
      if [ "$code" -eq 0 ]; then
        [ -n "$quiet" ] || printf 'ok    %-30s %4d s\n' "${label_of[$pid]}" $((took / 1000))
      else
        PINNED_FAILED=$((PINNED_FAILED + 1))
        printf 'FAIL  %-30s %4d s  %s\n' "${label_of[$pid]}" $((took / 1000)) "${log_of[$pid]}"
        tail -n 20 "${log_of[$pid]}" | sed 's/^/      /'
      fi
      free+=("${slot_of[$pid]}")
      unset "slot_of[$pid]"
    done
    [ -n "$line" ] || break
    IFS=$'\t' read -r label folder command <<< "$line"
    n=$((n + 1))
    slot=${free[0]}
    free=("${free[@]:1}")
    log=$(printf '%s/%03d-%s.log' "$logs" "$n" "${label//[^A-Za-z0-9_.-]/-}")
    # shellcheck disable=SC2086 # the command's words are split on purpose
    (pin "$slot" && cd "$folder" && export CARGO_MANIFEST_DIR=$folder && exec $command) > "$log" 2>&1 &
    pid=$!
    slot_of[$pid]=$slot
    label_of[$pid]=$label
    start_of[$pid]=${EPOCHREALTIME//[.,]/}
    log_of[$pid]=$log
  done
}

# test_runs JSON: the test binaries in the file of cargo's JSON messages, as
# run_pinned's lines, run from the repository's root: each labelled by its
# source, and run in its package's folder, as cargo test runs it.
test_runs() {
  local root
  root=$(pwd -W 2> /dev/null || pwd)
  grep '"overflow_checks":[a-z]*,"test":true}' "$1" \
    | sed -n 's/.*"manifest_path":"\([^"]*\)".*"src_path":"\([^"]*\)".*"executable":"\([^"]*\)".*/\2\t\1\t\3/p' \
    | sed 's/\\\\/\//g' \
    | sed "s|^${root//\\//}/||I" \
    | sed 's|/Cargo.toml\t|\t|' \
    | sort
}
