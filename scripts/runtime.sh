#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
base=(docker compose -f compose.runtime.yaml)
gpu=("${base[@]}" -f compose.runtime.gpu.yaml)
case "${1:-status}" in
  build)
    revision=unknown
    if [ -z "$(git -c safe.directory="$PWD" status --porcelain)" ]; then
      revision=$(git -c safe.directory="$PWD" rev-parse HEAD)
    fi
    OSCOPE_SOURCE_REVISION="$revision" "${base[@]}" build backend ;;
  account) shift; "${base[@]}" run --rm --no-deps backend account "$@" ;;
  up) "${gpu[@]}" up -d --wait --wait-timeout 90 ;;
  up-cpu) "${base[@]}" up -d --wait --wait-timeout 90 ;;
  stop) "${base[@]}" stop backend ;;
  down) "${base[@]}" down ;;
  status) "${base[@]}" ps ;;
  logs) "${base[@]}" logs --tail=100 backend ;;
  health) "${base[@]}" exec -T backend /usr/local/bin/oscope-healthcheck ;;
  gpu-check) "${gpu[@]}" run --rm --no-deps -T --entrypoint /usr/local/bin/oscope-cuda-check backend --require-cuda --smoke ;;
  config) "${base[@]}" config --quiet; "${gpu[@]}" config --quiet ;;
  *) echo 'usage: scripts/runtime.sh [build|account|up|up-cpu|stop|down|status|logs|health|gpu-check|config]' >&2; exit 2 ;;
esac
