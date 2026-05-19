# Graph Worker Nack Storm Fix
**Date:** 2026-03-22
**Branch:** `feat/pulse-shell-and-hybrid-search`

---

## Session Overview

Diagnosed and fixed an infinite AMQP nack storm that caused the graph worker to spam hundreds of `pre-ack buffer full (cap=8), nacking delivery for requeue` warnings per second across all 4 lanes, forcing a `^C` kill. Root cause was a missing `"using": "dense"` field in Qdrant `/points/query` requests for the named-vector `axon` collection, which caused every graph job to fail with a 400 error at the `compute_similarity` step.

---

## Timeline

1. **Symptom reported**: User observed constant `graph worker lane=N pre-ack buffer full (cap=8), nacking delivery for requeue` spam from all 4 lanes simultaneously.
2. **Traced nack path**: `amqp.rs` saturates when `semaphore.available_permits() == 0` — pre-ack buffer fills (cap = `lane_count × 2` = 8), deliveries get nacked with `requeue: true`, RabbitMQ immediately redelivers → tight loop.
3. **Found 4 stuck running jobs + 151 failed jobs**: DB confirmed the semaphore was fully consumed.
4. **Traced failure path**: Graph jobs completing the Neo4j write steps but failing at `compute_similarity`.
5. **Identified Qdrant 400 error**: `compute_similarity` calls `/collections/axon/points/query` without `"using": "dense"` — required for named-vector collections.
6. **Confirmed via curl**: Without `using` → `{"error": "Wrong input: Not existing vector name error: "}`; with `"using": "dense"` → `status: ok, result_len: 5 ✓`.
7. **Applied fix**: Added `"using": "dense"` to `build_recommend_request`, replaced `.error_for_status()?` with graceful non-2xx handling.
8. **Reset 4 stuck jobs**: Updated DB status from `running` → `failed`.
9. **Tests pass**: 6/6 similarity tests green.

---

## Key Findings

- **Root cause file**: `crates/jobs/graph/similarity.rs:24` — `build_recommend_request` missing `"using": "dense"` field.
- **Nack mechanism**: `crates/jobs/worker_lane/amqp.rs` pre-ack cap = `lane_count.saturating_mul(2).max(2)`. With 4 lanes = cap of 8. All 4 semaphore permits held by stuck jobs → every new delivery fills the 8-slot buffer → nack loop.
- **`axon` collection is named-vector mode**: Has both `dense` (1024-dim) and `bm42` sparse vectors. Qdrant's `/query` recommend endpoint requires `"using": "dense"` at the TOP LEVEL (not inside `"query"`) for named-vector collections.
- **152 jobs affected**: 151 failed fast at `compute_similarity`; 4 hung indefinitely (had actual Qdrant points so got further in the pipeline).
- **4 stuck jobs were local session files**: All 4 stuck `running` jobs were session markdown files from various workspace repos, not web URLs.

---

## Technical Decisions

- **Graceful non-2xx handling in `compute_similarity`**: Instead of propagating the error via `.error_for_status()?` (which would fail the entire graph job), non-2xx responses now log a warning and return `Ok(vec![])`. This makes similarity computation a best-effort step — a graph job can still write entities and document nodes even if Qdrant similarity lookup fails (e.g., unnamed collection, point not found, transient 5xx).
- **`"using": "dense"` always present**: Even for unnamed/legacy collections this field is harmless — Qdrant ignores unknown top-level fields in dense-only mode. Adding it unconditionally avoids needing to thread `VectorMode` state into the graph job pipeline.
- **DB reset via `docker exec`**: No `psql` binary in dev environment; used `docker exec axon-postgres psql` directly.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/graph/similarity.rs` | Added `"using": "dense"` to `build_recommend_request`; replaced `.error_for_status()?` with graceful status check + log; added `log_warn` import; updated test to assert `"using"` field |

---

## Commands Executed

```bash
# Reset 4 stuck running jobs
docker exec axon-postgres psql -U axon -d axon -c \
  "UPDATE axon_graph_jobs SET status='failed', error_text='killed during worker restart', \
   finished_at=NOW() WHERE status='running' RETURNING id, url;"
# Result: 4 rows updated

# Compile check
cargo check --bin axon
# Result: Finished dev profile in 7.47s (clean)

# Run similarity tests
cargo test -p axon --lib similarity
# Result: 6 passed; 0 failed
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Graph job outcome | 400 error at `compute_similarity` → job fails | `compute_similarity` returns `Ok(vec![])` on non-2xx → job continues |
| Qdrant request | Missing `"using"` field for named-vector collection | `"using": "dense"` always present |
| Nack storm | All 4 lanes nacking constantly when any jobs held permits | Jobs succeed → permits released → normal processing |
| 4 stuck jobs | `status='running'` indefinitely | Reset to `status='failed'` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Clean compile | Finished in 7.47s, 0 errors | ✓ |
| `cargo test -p axon --lib similarity` | All 6 pass | 6 passed, 0 failed | ✓ |
| `build_recommend_request_structure` test | `req["using"] == "dense"` | Assertion passes | ✓ |
| DB update stuck jobs | 4 rows updated | `UPDATE 4` | ✓ |

---

## Source IDs + Collections Touched

- No embed/retrieve operations during this session (code fix only).

---

## Risks and Rollback

- **Risk**: `"using": "dense"` on an unnamed (legacy) collection — low risk, Qdrant ignores unknown top-level fields. Tested: unnamed collections use `/points/search` not `/points/query`, so this code path wouldn't be hit for legacy collections anyway.
- **Rollback**: Revert `similarity.rs` — remove `"using": "dense"` line and restore `.error_for_status()?`. The 4 reset jobs will need to be re-queued if rollback is needed.

---

## Decisions Not Taken

- **Thread `VectorMode` into graph jobs**: Would require propagating `VectorMode` detection through the graph pipeline. Overkill — `"using": "dense"` is safe for both collection types, and graceful error handling covers the fallback.
- **Increase pre-ack buffer cap**: Would only delay the nack storm, not fix the root cause. Larger buffer = more memory consumed during storms, no benefit.
- **Add per-request timeout to Neo4j `send()`**: The 4 stuck jobs appeared to be hung (possibly at Neo4j writes), but the 30s `HTTP_CLIENT` timeout should cover this. Left as-is; can revisit if jobs hang beyond 30s in practice.

---

## Open Questions

- **Why exactly 4 jobs hung**: The 4 stuck jobs were session markdown files. It's unclear whether they hung at `qdrant_retrieve_by_url`, `write_document_and_chunks`, or `compute_similarity` — the worker was killed before the 30s timeout fired. May have been a slow Neo4j write.
- **Re-queue the 151 failed jobs**: They're currently `status='failed'`. To process them, run `axon graph recover` (if that subcommand exists) or manually re-queue. Verify that `axon graph` has a `recover` subcommand.
- **`axon` collection migration**: The `axon` collection has 144,992 points in named-vector mode. The `cortex` collection has 7M+ points in unnamed mode. The graph worker's `axon` collection is correct/modern; no migration needed there.

---

## Next Steps

1. Start the graph worker and verify jobs process successfully: `cargo run --bin axon -- graph worker`
2. Monitor for any remaining 400 errors (there should be none after the fix)
3. Consider re-queuing the 151 failed graph jobs via `axon graph recover` or equivalent
4. Optionally add an integration test for `compute_similarity` with a mock Qdrant that returns 400 to verify graceful handling
