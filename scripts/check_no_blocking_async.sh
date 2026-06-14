#!/usr/bin/env bash
set -euo pipefail

# TEST-M2 / PERF-C1 regression guard.
#
# The ask/retrieval hot path runs inside the shared async runtime. Calling a
# blocking executor primitive there (`block_on` / `block_in_place`) stalls a
# tokio worker thread and can deadlock the single-threaded test runtime — the
# exact regression class flagged as PERF-C1. Keep these surfaces blocking-free.

pattern='block_on|block_in_place'

paths=(
  src/vector/ops/commands/ask
  src/vector/ops/qdrant/hybrid.rs
  src/vector/ops/qdrant/dual_search.rs
)

if rg -n "$pattern" "${paths[@]}" \
  --glob '!target/**'
then
  echo "blocking-on-async primitive found in the ask/retrieval hot path" >&2
  echo "do not call block_on/block_in_place under these surfaces — use .await" >&2
  exit 1
fi
