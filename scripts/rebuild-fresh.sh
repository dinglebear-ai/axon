#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sha="$(git rev-parse HEAD)"
echo "Rebuilding axon-web + axon-workers at git SHA: $sha"

if [[ "${AXON_AUTO_CACHE_GUARD:-true}" == "true" ]]; then
  "$repo_root/scripts/cache-guard.sh" prune
fi

if [[ "${AXON_ENFORCE_DOCKER_CONTEXT_PROBE:-true}" == "true" ]]; then
  "$repo_root/scripts/check_docker_context_size.sh"
fi

# Ensure the shared axon network exists (created by docker-compose.services.yaml).
if ! docker network inspect axon >/dev/null 2>&1; then
  echo "Creating axon network (run 'just services-up' to start infra)..."
  docker network create axon
fi

AXON_GIT_SHA="$sha" docker compose up -d --build --force-recreate axon-workers axon-web

echo
echo "Verifying running container revisions..."
"$repo_root/scripts/check-container-revisions.sh"
