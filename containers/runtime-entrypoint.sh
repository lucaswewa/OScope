#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  case "$argument" in
    --config|--config=*) exec /usr/local/bin/oscope-server "$@" ;;
    --help|-h|--version|-V) exec /usr/local/bin/oscope-server "$@" ;;
  esac
done
config=${OSCOPE_CONFIG:-/var/lib/oscope/data/config/oscope.toml}
if [ "$config" = /var/lib/oscope/data/config/oscope.toml ] && [ ! -e "$config" ]; then
  # Seed once; upgrades must not overwrite instrument configuration.
  mkdir -p /var/lib/oscope/data/config
  (umask 077; cp /etc/oscope/oscope.toml "$config")
fi
exec /usr/local/bin/oscope-server --config "$config" "$@"
