#!/usr/bin/env bash
# Bootstrap a dedicated Ubuntu 24.04 WSL distribution. Requires root.
# This is a developer bootstrap, not the qualified production host baseline.
set -euo pipefail
if [ "${EUID}" -ne 0 ]; then
  echo 'Run with sudo: sudo bash scripts/setup-wsl.sh' >&2
  exit 2
fi
. /etc/os-release
if [ "$ID" != ubuntu ] || [ "$VERSION_ID" != 24.04 ]; then
  echo 'This bootstrap targets Ubuntu 24.04 only.' >&2
  exit 2
fi
if ! grep -qi microsoft /proc/sys/kernel/osrelease; then
  echo 'This bootstrap targets WSL2. Set up native Linux Docker separately.' >&2
  exit 2
fi
apt-get update -qq
apt-get install -y --no-install-recommends ca-certificates curl gnupg \
  docker.io docker-compose-v2 docker-buildx
if ! command -v nvidia-ctk >/dev/null; then
  curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey \
    | gpg --dearmor --yes -o /usr/share/keyrings/oscope-nvidia-container-toolkit.gpg
  curl -fsSL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
    | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/oscope-nvidia-container-toolkit.gpg] https://#g' \
    > /etc/apt/sources.list.d/oscope-nvidia-container-toolkit.list
  apt-get update -qq
  apt-get install -y --no-install-recommends nvidia-container-toolkit
fi
nvidia-ctk runtime configure --runtime=docker
systemctl enable --now docker
systemctl restart docker
docker version
docker compose version
printf '%s\n' 'WSL Docker setup completed. Run the environment GPU probe next.'
