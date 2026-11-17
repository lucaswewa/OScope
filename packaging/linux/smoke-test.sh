#!/usr/bin/env bash
# Checks an installed OScope service (docs/deploying-on-ubuntu.md):
#
# - it is enabled, runs as oscoped, may use the serial ports, and answers as
#   the version installed;
# - the simulated instrument is well;
# - a frame captured into a new project is kept, then the project is deleted;
# - asked to restart, it is started again by systemd.
#
# Run as one of its operators, a member of the group oscoped, it also checks
# what they may do: restart the service without sudo, read its log and its
# data, change its configuration, and use the serial ports.
#
#   bash packaging/linux/smoke-test.sh
#
# It restarts the service twice: run it while no one uses the instrument. It
# needs curl and jq, and serves for a service on this computer without TLS,
# as the .deb configures it. It reads the port from the configuration, and
# ends with "smoke-test: ok".
set -euo pipefail
shopt -s inherit_errexit

config=/etc/oscope/oscope.toml
fail() {
    echo "FAIL $*" >&2
    echo "     its log: journalctl -u oscope -n 50" >&2
    exit 1
}
ok() { echo "ok   $*"; }
for tool in curl jq; do
    command -v "$tool" >/dev/null || fail "no $tool: the administrator installs it with: sudo apt install $tool"
done
[ -r "$config" ] || fail "no $config: is OScope's .deb installed?"
port=$(awk -F= '
    /^[[:space:]]*\[/ { table = $0; gsub(/[[:space:]]|#.*/, "", table) }
    table == "[network]" && $1 ~ /^[[:space:]]*port[[:space:]]*$/ {
        value = $2; sub(/#.*/, "", value); gsub(/[[:space:]"]/, "", value); print value
    }' "$config")
base="http://127.0.0.1:${port:-8210}"
# Whether this account is in the group $1. grep reads all its input: with
# pipefail, -q would stop early, and fail what wrote to it.
in_group() { id -nG | tr ' ' '\n' | grep -x "$1" >/dev/null; }

# A request: its body, or a failure saying why unless its status is 2xx.
request() {
    local out code
    out=$(curl -sS --max-time 30 -w '\n%{http_code}' "$@") || fail "${*: -1}: no answer"
    code=${out##*$'\n'}
    out=${out%$'\n'*}
    case "$code" in
        2??) printf '%s\n' "$out" ;;
        *) fail "${*: -1}: $code $out" ;;
    esac
}
# The URL of an href, which is whole or a path.
url() {
    case "$1" in
        http://* | https://*) printf '%s\n' "$1" ;;
        *) printf '%s%s\n' "$base" "$1" ;;
    esac
}
# Waits up to 60 s for a command to succeed.
wait_for() {
    local what=$1
    shift
    for _ in $(seq 300); do
        if "$@" >/dev/null 2>&1; then return 0; fi
        sleep 0.2
    done
    fail "$what, in 60 s"
}
# Invokes the action $1 with the input $2, and prints its output once done.
invoke() {
    local href invocation
    href=$(request -X POST -H 'Content-Type: application/json' -d "$2" "$base/api/v1/$1" | jq -r .href)
    [ "$href" != null ] || fail "$1: no invocation"
    for _ in $(seq 1500); do
        invocation=$(request "$(url "$href")")
        case $(jq -r .status <<<"$invocation") in
            completed) jq -c .output <<<"$invocation"; return 0 ;;
            pending | running) sleep 0.2 ;;
            *) fail "$1: $invocation" ;;
        esac
    done
    fail "$1 didn't end in 300 s"
}
# Runs the job that the action $1 submits, and prints the job once completed.
job() {
    local href job
    href=$(invoke "$1" "$2" | jq -r .href)
    [ "$href" != null ] || fail "$1: no job"
    for _ in $(seq 1500); do
        job=$(request "$(url "$href")")
        case $(jq -r .state <<<"$job") in
            completed) printf '%s\n' "$job"; return 0 ;;
            failed | cancelled) fail "$1: $job" ;;
            *) sleep 0.2 ;;
        esac
    done
    fail "$1 didn't end in 300 s"
}
main_pid() { systemctl show -p MainPID --value oscope; }
pid_changed() { local now; now=$(main_pid); [ "$now" != 0 ] && [ "$now" != "$1" ]; }
healthy() { curl -fsS --max-time 5 "$base/health"; }

# The service, its account and its answers.
version=$(dpkg-query -W -f='${Version}' oscope 2>/dev/null) || fail "OScope's .deb isn't installed"
[ "$(systemctl is-enabled oscope)" = enabled ] || fail "the service isn't enabled"
wait_for "the service is active" systemctl is-active --quiet oscope
account=$(ps -o user:32= -p "$(main_pid)")
[ "$account" = oscoped ] || fail "the service runs as $account, not oscoped"
ok "oscope $version: enabled, active, as oscoped"
dialout=$(getent group dialout | cut -d: -f3)
awk '/^Groups:/ { $1 = ""; print }' "/proc/$(main_pid)/status" | tr ' ' '\n' | grep -x "$dialout" >/dev/null ||
    fail "the service can't use the serial ports: its groups lack dialout"
ok "it may use the serial ports, where the stage comes: group dialout"
wait_for "$base/health answers" healthy
served=$(request "$base/api/v1/system/version" | jq -r .)
[ "$served" = "$version" ] || fail "it serves $served, not $version"
ok "it answers on $base, as $served"

# The simulated instrument. After each start, the stage waits to be homed, as
# a real controller does: well, though in alarm. The smoke test moves nothing.
status=$(request "$base/api/v1/status/instrument")
stage=$(request "$base/api/v1/stage/status")
unhomed=$(jq '.state == "alarm" and (.reason | test("homing"; "i"))' <<<"$stage")
for subsystem in camera stage lights disks jobs; do
    state=$(jq -r --arg s "$subsystem" '.subsystems[] | select(.subsystem == $s) | .state' <<<"$status")
    if [ "$state" != ok ] && ! { [ "$subsystem" = stage ] && [ "$unhomed" = true ]; }; then
        fail "the $subsystem: ${state:-missing}: $(jq -c . <<<"$status")"
    fi
done
ok "the instrument: $(jq -r --argjson unhomed "$unhomed" '[.subsystems[]
    | if .subsystem == "stage" and $unhomed then "stage to be homed" else "\(.subsystem) \(.state)" end]
    | join(", ")' <<<"$status")"
if [ "$unhomed" = true ]; then
    echo "note the stage waits to be homed, as after each start: this test moves nothing"
fi

# A frame captured into a new project, kept, then the project deleted.
project=""
streaming=$(request "$base/api/v1/camera/streaming")
cleanup() {
    if [ -n "$project" ]; then
        (invoke projects/delete "{\"id\": \"$project\"}") >/dev/null 2>&1 || true
    fi
    if [ "$streaming" = false ]; then
        (invoke camera/stop_streaming '{}') >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT
[ "$streaming" = true ] || invoke camera/start_streaming '{}' >/dev/null
project=$(invoke projects/create "{\"name\": \"smoke test $(date '+%F %T')\"}" | jq -r .id)
captured=$(job capture/single "{\"project\": \"$project\", \"name\": \"smoke\"}")
acquisition=$(jq -r '.results[0] | split("/") | last' <<<"$captured")
contents=$(request "$base/api/v1/projects/project/$project")
image=$(jq -r --arg a "$acquisition" \
    '.acquisitions[] | select(.id == $a) | "\(.image.width) × \(.image.height)"' <<<"$contents")
[ -n "$image" ] || fail "the project $project has no acquisition $acquisition"
ok "a frame of $image captured into a new project"
if in_group oscoped; then
    [ -d "/var/lib/oscope/data/projects/$project" ] || fail "no /var/lib/oscope/data/projects/$project to read"
    ok "its folder can be read: /var/lib/oscope/data/projects/$project"
fi
invoke projects/delete "{\"id\": \"$project\"}" >/dev/null
project=""
ok "the project deleted"

# Asked to restart, through the system Thing: exit code 75, and systemd starts
# it again. The new process's camera doesn't stream.
first=$(main_pid)
request -X POST -H 'Content-Type: application/json' -d '{}' "$base/api/v1/system/restart" >/dev/null
streaming=true
wait_for "systemd starts it again" pid_changed "$first"
wait_for "the new process answers" healthy
ok "restarted through the API: process $first, then $(main_pid)"

# What its operators may do.
if in_group oscoped; then
    first=$(main_pid)
    systemctl --no-ask-password restart oscope || fail "an operator can't restart it without sudo"
    wait_for "the restarted process answers" healthy
    pid_changed "$first" || fail "systemctl restart didn't start a new process"
    ok "restarted by an operator, without sudo: process $first, then $(main_pid)"
    log=$(journalctl -u oscope -n 200 --no-pager)
    grep -q "serving" <<<"$log" || fail "an operator can't read its log"
    ok "its log can be read: journalctl -u oscope"
    [ -w "$config" ] || fail "an operator can't change $config"
    ok "its configuration can be changed: $config"
    in_group dialout || fail "an operator can't use the serial ports: not in the group dialout"
    ok "the serial ports can be used: group dialout"
else
    echo "note $(id -un) isn't in the group oscoped: the operator's checks are skipped"
fi
echo "smoke-test: ok"
