#!/usr/bin/env bash
set -euo pipefail

pattern='AXON_LITE|--lite|lite_mode|migrate-env|axon_refresh_jobs|axon_graph_jobs|AXON_PG_URL|AXON_PG_MCP_URL|AXON_REDIS_URL|AXON_AMQP_URL|AXON_(BATCH|CRAWL|EMBED|EXTRACT|INGEST)_QUEUE'

paths=(
  src
  scripts
  tests
  README.md
  CLAUDE.md
  docs/architecture/overview.md
  docs/guides/configuration.md
  docs/operations/deployment.md
  docs/reference/job-lifecycle.md
  docs/reference/mcp/overview.md
  docs/operations/operations.md
  docs/operations/security.md
  docs/guides/getting-started.md
  docs/reference/commands
  docs/reference/env-matrix.toml
)

if rg -n "$pattern" "${paths[@]}" \
  --glob '!target/**' \
  --glob '!scripts/check_legacy_runtime_terms.sh'
then
  echo "legacy runtime terms found in active surfaces" >&2
  exit 1
fi
