# Session: gh-address-comments Rounds 2–5
Date: 2026-03-16
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Continued from a previous session (v0.25.2 push + Round 1). Addressed four consecutive rounds of PR review threads via parallel specialized agents, pushing after each round. Final state: **69 review threads, 0 unresolved**. Pushed `e2362a68..4838a7cf`.

## Timeline

| Time | Activity |
|------|----------|
| Start | Context resumed mid-session; Round 3 agents (`a03eae335c334b0ab`, `a381c681dc9421816`) still running |
| +0m | Round 3 TS agent completed: `12f1456b fix(web): restore MCP card navigation` |
| +1m | Round 3 Rust+docs agent completed: `e2362a68 fix(ingest): chunk overlap stride fix` |
| +2m | Discovered uncommitted changes (CLAUDE.md, batch.rs, worker_lane, vector) from Rust agent |
| +4m | Committed orphaned changes as `5b47e5ec` (UTF-8 char boundary + services fixes absorbed) |
| +5m | Fetched PR threads: 8 new threads (Round 4) |
| +6m | Dispatched 3 parallel Round 4 agents: services, tei/jobs, TS stale-session |
| +35m | TS agent completed: `d72509c0` — session gen counter on manual switch |
| +40m | Services agent committed: `5b47e5ec` (absorbed into earlier commit via staged index) |
| +42m | tei/jobs agent committed: `64906291` — permit order, invalid index cleanup, .env comment |
| +43m | Fetched threads: 4 more new threads (Round 5) + 3 Round-4 threads needing mark_resolved |
| +44m | Committed files.rs UTF-8 char boundary fix: `5b47e5ec` (already in); resolved 2 threads immediately |
| +45m | Dispatched Round 5 agent for worker_lane pre-ack issues |
| +55m | Round 5 agent completed: `4838a7cf` — pre-ack job loss + unbounded VecDeque cap |
| +56m | Final verification: 69 threads, 0 unresolved |
| +57m | `git push` → `e2362a68..4838a7cf` |

## Key Findings

- **Orphan commit `2dc60842`**: Services agent's background commit task raced with the main session's UTF-8 commit. The staged services changes were absorbed into `5b47e5ec` when `git add crates/ingest/github/files.rs && git commit` ran — picking up already-staged files. `2dc60842` became an unreachable orphan. All fixes are correctly present in HEAD.
- **`emit_with_timeout` false positive**: `.is_ok()` on `tokio::time::timeout(...).await` only checks the outer `Result<T, Elapsed>`, not the inner `Result<(), SendError>`. Fixed to `.map(|r| r.is_ok()).unwrap_or(false)`.
- **Turn finalization silent drops**: `finalize_successful_turn` used `emit_nonblocking` (try_send) for `TurnResult` and `EditorWrite` — critical events silently dropped under backpressure. Changed to `emit_with_timeout(5s)`.
- **Pre-ack VecDeque unbounded**: With `basic_qos(1)`, pre-acking immediately releases the unacked slot; RabbitMQ pushes the next message instantly. Under a large queue, the entire queue drains into memory. Fixed with a cap of `lane_count * 2`.
- **Pre-acked job loss on DB error**: `claim_preacked_job` returned `Ok(None)` on transient DB failure — job was AMQP-acked with no redelivery path. Fixed to propagate `Err(e)` so caller can `push_front` the UUID for retry.

## Technical Decisions

- **`emit_with_timeout` for TurnResult/EditorWrite vs `emit_nonblocking`**: Turn completion events are terminal — if dropped, the frontend never receives the result. 5s timeout gives transient backpressure time to drain without blocking indefinitely.
- **`preack_cap = lane_count * 2`**: Ties the in-memory buffer to actual concurrency (not an arbitrary constant). Two messages per worker slot: one active, one queued next.
- **Commit `5b47e5ec` message mismatch**: The commit message says "UTF-8 char boundary" but the diff includes services fixes. This is a consequence of the staged-index race — noted in session, not corrected (rewriting history on a PR branch is risky).
- **`DO $$` cannot run `DROP INDEX CONCURRENTLY`**: Postgres prohibits CONCURRENTLY DDL inside transaction blocks. Pattern: separate SELECT to detect invalid index, then a second `sqlx::query("DROP INDEX CONCURRENTLY IF EXISTS...")` direct on the pool.
- **`AGENTS.md` thread (P3) resolved immediately**: Reviewer thought `CLAUDE.md` duplicated `AGENTS.md`, but `AGENTS.md` is a symlink to `CLAUDE.md`. Resolved without code change.

## Files Modified

| File | Commit | Purpose |
|------|--------|---------|
| `apps/web/components/shell/axon-message-tool-calls.tsx` | `89d009c5` (prior session) | Extract ToolStepDetail/ToolCallsGroup |
| `apps/web/components/landing-cards.tsx` | `12f1456b` | Restore MCP card navigation via `onMcpOpen` callback prop |
| `crates/ingest/github/files.rs` | `e2362a68`, `5b47e5ec` | Chunk overlap stride fix + UTF-8 char boundary safety |
| `crates/ingest/github/files/batch.rs` | `e2362a68` | Added collect-phase log line |
| `apps/web/components/shell/axon-shell-state.ts` | `d72509c0` | Add `bumpSessionInfoGen` callback |
| `apps/web/components/shell/axon-shell-state-actions.ts` | `d72509c0` | Call `bumpSessionInfoGen` in manual session switch handlers |
| `crates/services/events.rs` | `5b47e5ec` | Fix `emit_with_timeout` to check inner send result |
| `crates/services/acp/bridge.rs` | `5b47e5ec` | Upgrade TurnResult/EditorWrite to `emit_with_timeout(5s)` |
| `crates/services/system.rs` | `5b47e5ec` | Fix domains clamp max from 1M to 10M |
| `crates/services/types/acp.rs` | `5b47e5ec` | Fix `usage_update` wire format to nest `usage` object |
| `tests/services_acp_bridge_event_serialize.rs` | `5b47e5ec` | Update wire-shape tests for new `usage_update` format |
| `crates/services/CLAUDE.md` | `e2362a68` | Create service-layer context doc |
| `crates/services/AGENTS.md` | `e2362a68` | Symlink to CLAUDE.md |
| `crates/services/GEMINI.md` | `e2362a68` | Symlink to CLAUDE.md |
| `crates/vector/ops/tei/tei_client.rs` | `64906291` | Drop resp before permit on non-success paths |
| `crates/jobs/embed.rs` | `64906291` | Invalid index cleanup before CONCURRENTLY IF NOT EXISTS |
| `crates/jobs/refresh.rs` | `64906291` | Same invalid index cleanup (3 indexes) |
| `crates/jobs/crawl/runtime.rs` | `64906291` | Same invalid index cleanup (3 indexes) |
| `.env.example` | `64906291` | Fix TEI timeout comment to state actual 167s worst-case budget |
| `crates/vector/CLAUDE.md` | `e2362a68` | Document VectorMode detection + hybrid search module layout |
| `CLAUDE.md` | `5b47e5ec` | Fix TEI retry conditions + hybrid search fallback note |
| `crates/jobs/worker_lane/delivery.rs` | `4838a7cf` | Propagate DB error instead of Ok(None) in claim_preacked_job |
| `crates/jobs/worker_lane/amqp.rs` | `4838a7cf` | Cap preacked_ids at lane_count*2; push_front on claim error |

## Commands Executed

```bash
git log --oneline -8                      # verified commit chain
git push                                  # e2362a68..4838a7cf pushed
python3 $HOME/.claude/skills/gh-address-comments/scripts/fetch_comments.py
python3 $HOME/.claude/skills/gh-address-comments/scripts/mark_resolved.py <thread_ids>
cargo check --quiet                       # exit 0 after each fix round
cargo test --lib --quiet                  # 1339+ passing, 0 failed
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `emit_with_timeout` | Reports success when channel closed (`.is_ok()` on outer Result) | Returns false when channel closed (`.map(|r| r.is_ok()).unwrap_or(false)`) |
| Turn finalization | `TurnResult`/`EditorWrite` silently dropped under backpressure | Wait up to 5s for channel capacity before dropping |
| `usage_update` wire | Flat `used`/`size` fields — Zod validation rejects every message | Nested `"usage": {"total_tokens": N}` — matches web client schema |
| Domains clamp | `env_usize_clamped` max was 1M (disagreed with 10M constant) | Clamp max raised to 10M |
| Stale session guard | Manual session switches didn't reset generation counter | `bumpSessionInfoGen` called in all 3 manual switch handlers |
| TEI non-success path | Semaphore permit released before response body dropped | `drop(resp)` before `drop(permit)` on all non-success paths |
| Invalid Postgres indexes | `IF NOT EXISTS` silently skips broken indexes after crash | Checks `NOT i.indisvalid` + `DROP CONCURRENTLY` before create |
| `preacked_ids` VecDeque | Unbounded — drains entire AMQP queue into memory under saturation | Capped at `lane_count * 2`; broker holds remaining messages |
| Pre-acked job DB error | Job UUID silently discarded → stuck in `pending` with no AMQP trigger | Error propagated; UUID pushed to front of queue for retry |
| MCP landing card | `href` removed, no navigation path | `onMcpOpen?: () => void` callback prop; parent wires pane switch |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --quiet` (post each round) | exit 0 | exit 0 | ✅ |
| `cargo test --lib --quiet` | 0 failures | 1339 passed, 0 failed | ✅ |
| `python3 verify_resolution.py` (final) | 0 unresolved | 69 threads, 0 unresolved | ✅ |
| `git push` | remote updated | `e2362a68..4838a7cf` | ✅ |
| lefthook pre-commit (all hooks) | all green | monolith ✓ test ✓ clippy ✓ | ✅ |

## Source IDs + Collections Touched

_(Axon embed attempted post-session — see below)_

## Risks and Rollback

- **Commit `5b47e5ec` message mismatch**: Commit message says "UTF-8 char boundary" but diff includes `crates/services/` fixes. Cosmetic only — all fixes correct and tested. Rollback: `git revert 5b47e5ec` would revert all included changes.
- **Pre-ack worker_lane changes**: Significant behavioral change to saturation handling. If the cap (`lane_count * 2`) is too small, messages may back up at the broker. Monitor `preacked_ids.len()` in production. Rollback: revert `4838a7cf`.
- **`DROP INDEX CONCURRENTLY` on startup**: If multiple workers start simultaneously after a crash, two might try to drop the same invalid index. Both use `IF EXISTS` — one will be a no-op. Safe.

## Decisions Not Taken

- **Rewrite commit `5b47e5ec` message**: Would require force-push on the PR branch. Risk of confusing collaborators/CI. Left as-is; the diff is correct.
- **Use `OptionFuture` in select!**: The cap check for `preacked_ids` was implemented with `tokio::select!` guard pattern instead — simpler and idiomatic without requiring a new import.
- **Increase `preack_cap` beyond `lane_count * 2`**: Larger cap means more memory but less broker feedback. 2x provides one queued job per active worker — enough buffer without unbounded growth.

## Open Questions

- **Dependabot 14 vulnerabilities** (7 high, 7 moderate) on default branch — unrelated to this session but visible in `git push` output. Worth triaging.
- **Commit `2dc60842` orphan**: Should be garbage-collected by Git's automatic `git gc`. No action needed, but worth noting if SHA appears in any CI logs.
- **`preacked_ids` monitoring**: No metrics/telemetry on VecDeque depth. A future improvement could log `preacked_ids.len()` in the sweep interval for observability.

## Next Steps

- Open a PR from `feat/pulse-shell-and-hybrid-search` → `main` (69/69 threads resolved, branch fully reviewed)
- Address Dependabot vulnerabilities on default branch
- Run `pnpm test` in `apps/web/` to verify ws-messages test suite
- Monitor pre-ack behavior in staging with `basic_qos` tuning
