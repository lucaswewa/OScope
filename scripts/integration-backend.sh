#!/usr/bin/env bash
# Explicit backend preparation, separate from frontend and browser execution.
set -euo pipefail
cd "$(dirname "$0")/.."
revision=$(python3 scripts/gates.py --backend-revision)
docker build --file containers/development.Dockerfile --target runtime \
  --build-arg "SOURCE_REVISION=$revision" --tag oscope-backend:integration .
docker image inspect oscope-backend:integration --format '{{.Id}} {{index .Config.Labels "org.opencontainers.image.revision"}}'
