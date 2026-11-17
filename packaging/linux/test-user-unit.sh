#!/usr/bin/env bash
# Tries oscope.service as the user's own unit, without root: starts it,
# restarts it through the system Thing (exit code 75), stops it, and removes
# it. The root-only lines (the account and its groups, the folders, the
# hardening) are left out; the rest is the unit as installed.
#
#   packaging/linux/test-user-unit.sh [path/to/oscope-server]
#
# It uses port 8228, and a unit and a folder named oscope-test.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
server="$(realpath "${1:-${CARGO_TARGET_DIR:-target}/debug/oscope-server}")"
port=8228
unit=oscope-test.service
units="$HOME/.config/systemd/user"
work="$(mktemp -d)"
url="http://127.0.0.1:$port"

cleanup() {
    systemctl --user stop "$unit" 2>/dev/null || true
    rm -f "$units/$unit"
    systemctl --user daemon-reload
    rm -rf "$work"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*" >&2
    systemctl --user status "$unit" --no-pager >&2 || true
    exit 1
}

# Waits up to 30 s for a command to succeed.
wait_for() {
    local what="$1"
    shift
    for _ in $(seq 300); do
        if "$@" >/dev/null 2>&1; then return 0; fi
        sleep 0.1
    done
    fail "$what"
}

main_pid() { systemctl --user show -p MainPID --value "$unit"; }
pid_changed() { local now; now="$(main_pid)"; [ "$now" != 0 ] && [ "$now" != "$1" ]; }

cat >"$work/oscope.toml" <<EOF
[server]
port = $port

[storage]
data_root = "$work/data"
scratch_root = "$work/scratch"

[devices]
camera = "fake"
stage = "fake"
illumination = "fake"
objective = "fake"
EOF

mkdir -p "$units"
grep -v -E '^(User|Group|SupplementaryGroups|StateDirectory|StateDirectoryMode|ReadWritePaths|NoNewPrivileges|ProtectSystem|ProtectHome|PrivateTmp)=' \
    "$here/oscope.service" |
    sed -e "s|^ExecStart=.*|ExecStart=$server --config $work/oscope.toml|" \
        -e 's|^WantedBy=.*|WantedBy=default.target|' >"$units/$unit"
systemctl --user daemon-reload

systemctl --user start "$unit"
wait_for "the server answers" curl -fs "$url/health"
first="$(main_pid)"
echo "started: process $first answers on $url"

curl -fs -X POST -H 'Content-Type: application/json' -d '{}' "$url/api/v1/system/restart" >/dev/null
wait_for "systemd starts it again" pid_changed "$first"
wait_for "the new server answers" curl -fs "$url/health"
second="$(main_pid)"
restarts="$(systemctl --user show -p NRestarts --value "$unit")"
[ "$restarts" = 1 ] || fail "$restarts restarts, where 1 was expected"
echo "restarted: process $first exited with 75, and systemd started $second"

systemctl --user stop "$unit"
status="$(systemctl --user show -p ExecMainStatus --value "$unit")"
result="$(systemctl --user show -p Result --value "$unit")"
[ "$status" = 0 ] && [ "$result" = success ] || fail "stopped with $status, $result"
echo "stopped: exit code 0, result success"
echo "test-user-unit: ok"
