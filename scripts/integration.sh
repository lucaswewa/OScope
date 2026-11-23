#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
mkdir -p artifacts
exec 9>artifacts/frontend.lock
flock 9
export OSCOPE_REPO_ROOT=$PWD
export OSCOPE_DOCKER_GID
OSCOPE_DOCKER_GID=$(stat -c '%g' /var/run/docker.sock)
export OSCOPE_BACKEND_IMAGE
OSCOPE_BACKEND_IMAGE=$(docker image inspect "${OSCOPE_BACKEND_IMAGE:-oscope-backend:r07}" --format '{{.Id}}')
export OSCOPE_E2E_RUN_ID="r08-$(date +%s)-$$"
workers=2
if [ "${1:-}" = render-perf ]; then
  shift
  export OSCOPE_RENDER_PERF=1
  workers=1
  set -- --project=chromium --grep @performance "$@"
fi
mkdir -p artifacts/r08/servers artifacts/r08
if [ "$(id -u)" = 0 ]; then
  chown 1000:1000 artifacts/r08/servers
fi
compose=(docker compose -f compose.frontend.yaml)
"${compose[@]}" run --rm --no-deps -T --user 0:0 integration \
  chown 1000:1000 node_modules /home/node/.npm
"${compose[@]}" run --rm --no-deps -T integration npm ci --no-audit --no-fund
cleanup() {
  local ids=()
  mapfile -t ids < <(docker ps -aq --filter "label=oscope.e2e.run=$OSCOPE_E2E_RUN_ID")
  if [ ${#ids[@]} -gt 0 ]; then docker rm -f "${ids[@]}" >/dev/null; fi
}
trap cleanup EXIT
if [ "${OSCOPE_E2E_GPU:-0}" = 1 ]; then
  docker run --rm --gpus all --read-only --tmpfs /tmp:rw,nosuid,nodev,size=256m \
    --entrypoint /usr/local/bin/oscope-cuda-check "$OSCOPE_BACKEND_IMAGE" --require-cuda --smoke
fi
# The schema comes from the exact image used by every browser fixture.
docker run --rm --network none --read-only --tmpfs /tmp:rw,nosuid,nodev,size=256m \
  -e NVIDIA_VISIBLE_DEVICES=void "$OSCOPE_BACKEND_IMAGE" \
  --config /etc/oscope/oscope.toml --print-openapi >artifacts/r08/openapi.json
operation=api:check
if [ "${1:-}" = types-write ]; then operation=api:types; fi
"${compose[@]}" run --rm --no-deps -T integration \
  env OSCOPE_OPENAPI="$PWD/artifacts/r08/openapi.json" npm run "$operation"
if [ "${1:-}" = types ] || [ "${1:-}" = types-write ]; then exit 0; fi
"${compose[@]}" run --rm --no-deps -T integration npm run build
"${compose[@]}" run --rm --no-deps -T integration xvfb-run -a npm run test:e2e -- --workers="$workers" "$@"
