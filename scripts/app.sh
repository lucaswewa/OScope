#!/usr/bin/env bash
# Same-origin Vue checkpoint, using existing image and persistent dev state.
set -euo pipefail
cd "$(dirname "$0")/.."
base=(docker compose -f compose.runtime.yaml -f compose.web.yaml)
case "${1:-status}" in
  up) "${base[@]}" -f compose.runtime.gpu.yaml up -d --wait --wait-timeout 90 ;;
  up-cpu) "${base[@]}" up -d --wait --wait-timeout 90 ;;
  down) "${base[@]}" down ;;
  status) "${base[@]}" ps ;;
  *) echo 'usage: scripts/app.sh [up|up-cpu|down|status]' >&2; exit 2 ;;
esac
