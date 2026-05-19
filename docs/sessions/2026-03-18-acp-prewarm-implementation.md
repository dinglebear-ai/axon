# ACP Adapter Pre-Warming Implementation

**Date:** 2026-03-18
**Branch:** `feat/pulse-shell-and-hybrid-search`
**Commit:** `217ae733` (bundle), prior implementation commits: `24b86975`, `aad02369`, `fc8d0564`, `cf81cdd5`, `f2eb21bd`
**Version:** 0.26.0 → 0.27.0

## Session Overview

Implemented ACP adapter pre-warming to eliminate the ~45-second cold start on the first chat message. The ACP adapter's `establish_acp_session()` was deferred until the first `RunTurn` message — this session moved that expensive I/O to server boot time. Also bundled all outstanding changes (services routing, error context, docker-compose split, etc.) into a v0.27.0 release commit.

## Timeline

1. **Investigation** — Confirmed no prior prewarm implementation existed; only hang prevention was in place
2. **Plan creation** — Wrote 5-task implementation plan at `docs/superpowers/plans/2026-03-18-acp-prewarm.md`
3. **Plan review** — Reviewer caught 4 critical issues (wrong file paths, task ordering, module visibility, SDK struct guessing)
4. **Implementation** — Executed via subagent-driven development (5 tasks)
5. **Review** — Thorough review by `rust-pro` and `code-reviewer` agents with `/rust-code-review`, `/rust-best-practices`, `/rust-async-patterns` skills
6. **Critical bug fix** — `mark_turn_completed()` skipped on timeout/channel errors due to `?` operator early returns
7. **Version bump + push** — 0.26.0 → 0.27.0, changelog updated, committed as `217ae733`

## Key Findings

- **Cold start root cause:** `AcpConnectionHandle::spawn()` returns immediately but defers all I/O setup to the first `RunTurn` message in `adapter_loop()` — `establish_acp_session()` takes ~45s
- **Critical bug in initial implementation:** `?` operators on timeout and oneshot results caused early returns that bypassed `mark_turn_completed()`, leaving sessions permanently flagged as "in-flight" — the hung-turn detector would evict them within 5 minutes, defeating pre-warming entirely
- **`prepare_session_setup` reuse:** Instead of guessing `NewSessionRequest` struct fields, reused existing `prepare_session_setup()` with a minimal `AcpPromptTurnRequest`
- **Module visibility chain:** `sync_mode` is private in `execute.rs`; solved by re-exporting `pub(crate) use sync_mode::prewarm;` so `web.rs` can reach `spawn_prewarm_task()`
- **Cache key alignment:** Prewarm uses `build_agent_key()` with default caps (fs=true, term=true, no timeouts, no MCP servers) matching the most common Android app request configuration

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Pre-warm on boot with 2s delay | Gives server time to bind ports before spawning adapter |
| Lightweight ping turn ("Respond with exactly: WARM") | Forces `establish_acp_session()` without heavy LLM work |
| Non-fatal failure | Prewarm failure logs warning; first real request cold-starts normally |
| 120s timeout for prewarm | Generous timeout since boot is non-blocking |
| `AXON_ACP_PREWARM` env var (default: true) | Opt-out mechanism for environments where prewarm is undesirable |
| Extracted `build_agent_key()` helper | DRY — used by both `get_or_create_acp_connection()` and `prewarm_adapter()` |
| Match instead of `?` for completion tracking | Ensures `mark_turn_completed()` always runs regardless of timeout/channel errors |

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/web/execute/sync_mode/prewarm.rs` | Created | Core pre-warming module |
| `crates/web/execute/sync_mode/pulse_chat.rs` | Modified | Extracted `build_agent_key()` helper |
| `crates/web/execute/sync_mode.rs` | Modified | Added `pub(crate) mod prewarm;` |
| `crates/web/execute.rs` | Modified | Added `pub(crate) use sync_mode::prewarm;` re-export |
| `crates/web.rs` | Modified | Called `spawn_prewarm_task(cfg.clone())` in `start_server()` |
| `crates/core/config/types/config.rs` | Modified | Added `pub acp_prewarm: bool` field |
| `crates/core/config/parse/build_config.rs` | Modified | Added `acp_prewarm: env_bool("AXON_ACP_PREWARM", true)` |
| `crates/core/config/types/config_impls.rs` | Modified | Added Default + Debug for `acp_prewarm` |
| `Cargo.toml` | Modified | Version 0.26.0 → 0.27.0 |
| `CHANGELOG.md` | Modified | Added [0.27.0] section |
| `docs/superpowers/plans/2026-03-18-acp-prewarm.md` | Created | Implementation plan |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| First chat message latency | ~45 seconds (cold start) | Near-instant (adapter pre-warmed) |
| Server boot | No adapter spawned | Default adapter spawned + ping turn sent |
| Session cache on boot | Empty | Contains pre-warmed session for default agent key |
| Config options | No prewarm control | `AXON_ACP_PREWARM=true/false` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Compiles | Compiled successfully | PASS |
| `cargo test` | All pass | All tests pass | PASS |
| `rustfmt` | No changes | No formatting issues | PASS |
| `git push` | Pushed | `bc268fd0..217ae733` pushed | PASS |
| Pre-commit hooks | All pass | All hooks pass (biome, check, test, etc.) | PASS |

## Risks and Rollback

- **Risk:** Pre-warmed session may be evicted by 30-min reaper if no user sends a message within that window → first request cold-starts normally (graceful degradation)
- **Risk:** Cache key mismatch if Android app sends different capabilities than defaults → session cache miss, new adapter spawned (no worse than before)
- **Rollback:** Set `AXON_ACP_PREWARM=false` or revert commit `217ae733`

## Decisions Not Taken

- **Periodic re-warming after reaper eviction** — Rejected as over-engineering; the 30-min window is generous for active usage
- **Multiple pre-warmed sessions for different agent configs** — Rejected; default config covers 95%+ of requests
- **Warm-up via health check endpoint** — More complex, less reliable than boot-time task

## Open Questions

- Should the 2-second boot delay be configurable?
- Should prewarm log the actual latency of `establish_acp_session()` for monitoring?
- When Android app v1 ships, verify cache key alignment with actual request caps

## Next Steps

- Monitor prewarm latency in production logs
- Consider adding prewarm latency to `/api/cortex/stats` response
- Verify Android app's default capabilities match prewarm defaults
