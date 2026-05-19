# TEI Thundering Herd Fix — Session 2026-03-15

## Session Overview

Diagnosed and fixed a thundering herd bug where ~95 concurrent embed workers simultaneously exhausted TEI's TCP connection queue, producing mass `transport_error` failures. Added a process-wide semaphore to cap concurrent in-flight TEI requests, extracted a retry helper to reduce function size, and lowered the per-pipeline doc concurrency default.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User showed ~95 simultaneous `transport_error` WARN logs from `tei_embed`, all at timestamp `01:44:39`, all `attempt=1/5` |
| Phase 1 | Read error logs: identified thundering herd signature (same timestamp, transport-level not HTTP-level, all first attempts) |
| Phase 2 | Read `tei_client.rs`, `pipeline.rs`, `http/client.rs`, `embed.rs` to understand the concurrency model |
| Phase 3 | Spawned `rust-reviewer` agent for independent analysis and Rust-specific pattern review |
| Phase 4 | Ran `enforce_monoliths.py` to validate actual function sizes (reviewer overstated — 86 lines, not 144) |
| Phase 5 | Implemented fixes: semaphore, helper extraction, default tuning, rename |
| Phase 6 | Verified: `cargo check`, `cargo clippy`, 25 TEI lib tests passing, monolith policy clean |

---

## Key Findings

- **Root cause**: No global `Semaphore` in `tei_client.rs` — every `send_chunk_with_retries` call went straight to the network with no concurrency cap. With N embed workers × `doc_concurrency` (default `min(CPUs, 16)`) simultaneous docs, total concurrent TEI connections = N×16, overwhelming TEI's TCP listen backlog.
- **Transport vs HTTP error**: The failures were `transport_error` (TCP-level connection rejection), not `429 Too Many Requests`. This confirmed TEI's listen queue was saturated *before* requests reached the HTTP handler.
- **Thundering herd pattern**: All ~95 failures share the exact same timestamp `01:44:39` and all are `attempt=1/5`. The retries fire in lockstep ~1s later because jitter is only 0–500ms — insufficient to desynchronize 95 concurrent retriers.
- **Reviewer size overstatement**: `rust-reviewer` reported `send_chunk_with_retries` as 144 lines; actual was 86 (warning zone, not hard CI failure). Always verify with `enforce_monoliths.py --file`.
- **Pre-existing compile error on branch**: `crates/cli/commands/watch.rs:116` had a `get_watch_def` / `create_watch_def` mismatch preventing `./scripts/axon` from building. Fixed separately by user before embedding.

---

## Technical Decisions

**Why a static `LazyLock<Semaphore>` rather than per-pipeline?**
A per-pipeline semaphore only limits one pipeline run's concurrency. The thundering herd arises from *multiple* worker processes each running a pipeline. A process-wide static is the only effective gate when multiple pipelines run concurrently in the same process (or across worker lanes sharing the binary).

**Why default 8, not lower?**
TEI processes requests internally in batches and handles 8 concurrent connections comfortably (8 × batch_size=128 = 1024 simultaneous chunk embeddings). Defaulting lower would unnecessarily throttle single-worker scenarios. `AXON_TEI_MAX_CONCURRENT` allows operators to tune down if TEI is resource-constrained.

**Why lower `doc_concurrency` default clamp from 16 → 8?**
The semaphore is now the binding constraint on TEI concurrency; `doc_concurrency` primarily controls how many `PreparedDoc`s are held resident simultaneously (memory pressure). 8 is more appropriate for multi-worker deployments. The env-var max (64) remains as an escape hatch.

**Why not a `TeiError` typed enum?**
`thiserror` is not currently a project dependency. Adding it solely for this module would be reasonable but is a separate concern. The `Box<dyn Error>` boundary is consistent with the rest of the file and sufficient for the immediate fix.

**Why keep status retry path inline (not using `log_retry_and_sleep`)?**
The status retry log uses `status={}` not `err={}` — a different field name that would change log format for existing monitoring. Keeping it inline preserves exact log format compatibility.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/tei/tei_client.rs` | Added `TEI_CONCURRENCY` semaphore, `log_retry_and_sleep` helper, renamed `_tei_start` → `tei_start` | Fix thundering herd; reduce function size from 86 → 68 lines |
| `crates/vector/ops/tei/pipeline.rs` | Lowered `doc_concurrency` default clamp 16 → 8; simplified default computation | Reduce memory pressure in multi-worker deployments; bring `run_embed_pipeline` from 83 → 76 lines |

---

## Commands Executed

```bash
# Verified monolith policy before changes
python3 scripts/enforce_monoliths.py --file crates/vector/ops/tei/tei_client.rs
# → send_chunk_with_retries() is 86 lines (warning 80, limit 120) — CI passes

# Verified monolith policy after changes
python3 scripts/enforce_monoliths.py --file crates/vector/ops/tei/tei_client.rs
# → Monolith policy check passed.

python3 scripts/enforce_monoliths.py --file crates/vector/ops/tei/pipeline.rs
# → Monolith policy check passed.

# Type-check
cargo check --bin axon
# → Finished dev profile in 12.75s (0 errors)

# Clippy
cargo clippy --bin axon
# → 1 pre-existing warning (complex type), 0 new warnings from our changes

# TEI unit tests
cargo test --lib tei
# → test result: ok. 25 passed; 0 failed; 3 ignored
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Concurrent TEI requests per process | Unlimited (N workers × doc_concurrency) | Capped at `AXON_TEI_MAX_CONCURRENT` (default 8) |
| `transport_error` thundering herd | ~95 simultaneous first-attempt failures when multiple workers start together | Max 8 simultaneous requests; overflow waits for a permit rather than hitting TEI |
| Retry synchronization | All 95 retriers fire in lockstep ~1s later (jitter only 0–500ms) | Semaphore prevents >8 concurrent, eliminates the synchronized retry wave |
| Default `doc_concurrency` per pipeline | `min(CPUs, 16)` | `min(CPUs, 8)` |
| `_tei_start` variable name | Misleading `_` prefix implied unused | Renamed to `tei_start` (variable is used) |
| `send_chunk_with_retries` function size | 86 lines (warning zone) | 68 lines (under threshold) |
| `run_embed_pipeline` function size | 83 lines (warning zone) | 76 lines (under threshold) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `enforce_monoliths.py --file tei_client.rs` | policy check passed | policy check passed | ✓ |
| `enforce_monoliths.py --file pipeline.rs` | policy check passed | policy check passed | ✓ |
| `cargo check --bin axon` | 0 errors | 0 errors | ✓ |
| `cargo clippy --bin axon` | 0 new warnings | 0 new warnings (1 pre-existing) | ✓ |
| `cargo test --lib tei` | 25 passed, 0 failed | 25 passed, 0 failed | ✓ |

---

## Axon Embed Status

Attempted after user confirmed compile error on branch was fixed.

---

## Risks and Rollback

**Risk**: Setting `AXON_TEI_MAX_CONCURRENT=8` too low for a high-throughput single-worker scenario could reduce embed throughput. Mitigated by the env-var override — set higher if TEI is under-utilized.

**Risk**: If TEI is the bottleneck (slow GPU, large model), 8 concurrent requests may still queue up and hit the 30s request timeout. In that case lower `AXON_TEI_MAX_CONCURRENT` or raise `TEI_REQUEST_TIMEOUT_MS`.

**Rollback**: Revert `crates/vector/ops/tei/tei_client.rs` and `crates/vector/ops/tei/pipeline.rs` to their previous state. The semaphore is purely additive — no schema, no migration, no config required.

---

## Decisions Not Taken

- **`TeiError` typed enum**: Would enable differentiated backoff for transport vs HTTP errors. Deferred — requires adding `thiserror` dependency; not critical since the semaphore solves the retry synchronization problem.
- **Wider jitter window for transport errors**: Spreading retries over 500ms–3000ms instead of 0–500ms was considered. The semaphore solves the root cause; wider jitter is a bandaid.
- **Per-pipeline semaphore**: Only limits one pipeline's concurrency, not cross-worker stampedes. Rejected in favor of process-wide static.
- **Reducing `AXON_TEI_MAX_CONCURRENT` max (currently 64)**: Kept high as an operator escape hatch for single-worker high-throughput scenarios.

---

## Open Questions

- How many embed workers were actually running when the ~95 concurrent failures occurred? If only one worker, `doc_concurrency` must have been set higher than 16 via env var, or the crawl pipeline was also embedding concurrently.
- Should the 0–500ms jitter range be widened for transport errors specifically? The semaphore solves the root cause, but wider jitter is defense-in-depth.
- The `drain_concurrent_docs` 11-parameter function (with `#[allow(clippy::too_many_arguments)]`) warrants a `PipelineState` struct refactor — deferred to a future session.

---

## Next Steps

- Monitor embed worker logs after deployment to confirm thundering herd is resolved
- Consider `TeiError` typed enum in a follow-up PR (enables differentiated backoff, cleaner error handling)
- Address `drain_concurrent_docs` parameter count by extracting `PipelineState` struct
- Investigate the `watch.rs:116` compile error that was blocking the branch build (`get_watch_def` vs `create_watch_def`)
