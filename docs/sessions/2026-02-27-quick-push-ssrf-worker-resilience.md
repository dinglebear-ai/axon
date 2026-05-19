# Session: Quick Push — SSRF Hardening + Worker Resilience
**Date**: 2026-02-27 22:16 EST
**Branch**: `feat/crawl-download-pack`
**Commit**: `d9823b2`

---

## Session Overview

Short commit/push session triggered by `/quick-push`. Staged and committed a large working-tree diff (38 modified files + 6 new) spanning SSRF security hardening in the web layer, AMQP reconnect backoff in the Rust worker lane, dynamic multi-lane worker concurrency, MCP response simplification, and expanded test coverage. Fixed Biome lint issues in new test files and created missing AGENTS.md/GEMINI.md symlinks in active agent worktrees to satisfy the pre-commit hook.

---

## Timeline

| Time | Activity |
|------|----------|
| 22:16 | Oriented: confirmed branch `feat/crawl-download-pack`, ran `git diff --stat HEAD` and `git log --oneline -5` |
| 22:17 | Read CHANGELOG.md — found `ebca63c` undocumented; added row + new highlights section |
| 22:18 | `git add .` → first commit attempt — Biome hook failed on 2 new test files |
| 22:19 | Fixed `parser.test.ts:111` (string concat → template literal), `scanner.test.ts` (`process.env['HOME']` → `process.env.HOME`) |
| 22:20 | Second commit attempt — more Biome failures in `parser.test.ts:147` and `session-scanner.ts:71` |
| 22:21 | Ran `npx biome check --write --unsafe` on 3 files from `apps/web/` dir — fixed 3 files |
| 22:22 | Third commit attempt — `claude-symlinks` hook failed (missing AGENTS.md/GEMINI.md in 4 worktree dirs) |
| 22:23 | Created symlinks in `.claude/worktrees/agent-a3f320ff/{docs,crates/web}` and `.claude/worktrees/agent-a124e69c/{docs,crates/web}` |
| 22:24 | Fourth commit attempt — all hooks passed; 442 Rust tests green; commit `d9823b2` created |
| 22:25 | `git push` succeeded — `ebca63c..d9823b2` pushed to remote |

---

## Key Findings

- **Biome `useTemplate` rule** triggered on string concatenation in new test files (`parser.test.ts`, `session-scanner.ts`). Needed `--unsafe` flag because the fix involves template literals (safe but classified as unsafe by Biome's conservative approach).
- **Biome `useLiteralKeys` rule** triggered on `process.env['HOME']` — should be `process.env.HOME`.
- **`claude-symlinks` pre-commit hook** checks ALL directories containing `CLAUDE.md` for sibling `AGENTS.md` + `GEMINI.md` symlinks — including active worktree directories under `.claude/worktrees/`. This means new agent worktrees without symlinks will block commits.
- **Monolith warnings** (non-blocking): `run_amqp_lane()` at 86 lines and `run_polling_lane()` at 83 lines both exceed the 80-line soft warning threshold in `worker_lane.rs:199/315`. Both are under the 120-line hard limit.
- The working directory shifted to `apps/web/` during Biome fix — subsequent `git add` calls needed absolute paths or `cd /home/jmagar/workspace/axon_rust` prefix.

---

## Technical Decisions

1. **SSRF guard via allowlist (not denylist)**: `validateAddDir()` uses `ALLOWED_DIR_ROOTS = ['/home/node', '/tmp', '/workspace']` — explicit allowlist is safer than trying to enumerate all dangerous paths. `process.env.HOME` deliberately excluded to prevent host-path bypass in test/dev environments.

2. **AMQP backoff starts at 2s, caps at 60s**: Balances fast reconnect on transient failure vs. not hammering a restarting RabbitMQ. Reset to 2s on success prevents penalizing stable connections after a recovered failure.

3. **`join_all(1..=WORKER_CONCURRENCY)` over `join!(lane1, lane2)`**: Makes lane count dynamic (driven by `WORKER_CONCURRENCY` const) rather than hardcoded to 2 lanes. Cleaner to extend — adding a third lane is a config change, not a code change.

4. **`respond_with_mode` removed from crawl/list/status/domains**: These responses are always small enough to be inline — artifact path indirection adds complexity with no benefit. Simplified to `AxonToolResponse::ok(...)` directly.

5. **`--unsafe` Biome fix acceptable for test files**: The `useTemplate` fix is semantically equivalent and Biome's "unsafe" classification is conservative. No behavior change, only style.

---

## Files Modified

### Web (TypeScript)
| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/claude-stream-types.ts` | `validateAddDir()` SSRF guard, `PULSE_SKIP_PERMISSIONS` env var, `TOOL_ENTRY_RE` regex filter |
| `apps/web/app/api/mcp/status/route.ts` | `validateStatusUrl()` with blocked hostnames + private IP patterns |
| `apps/web/app/api/mcp/route.ts` | Related MCP route updates |
| `apps/web/app/api/agents/route.ts` | Agents route updates |
| `apps/web/app/agents/page.tsx` | Agents page updates |
| `apps/web/app/mcp/page.tsx` + `components.tsx` | MCP page updates |
| `apps/web/app/settings/page.tsx` | Settings page improvements |
| `apps/web/app/page.tsx` + `globals.css` | Home page + CSS updates |
| `apps/web/components/omnibox.tsx` | Omnibox nav updates |
| `apps/web/components/pulse/pulse-chat-pane.tsx` + `pulse-workspace.tsx` | Pulse UI updates |
| `apps/web/components/recent-sessions.tsx` + `results-panel.tsx` + `service-worker.tsx` | Component updates |
| `apps/web/hooks/use-pulse-settings.ts` + `use-ws-messages.ts` | Hook updates |
| `apps/web/lib/pulse/types.ts` | Type updates |
| `apps/web/lib/sessions/claude-jsonl-parser.ts` + `session-scanner.ts` | Session scanning improvements |
| `apps/web/app/api/pulse/chat/replay-cache.ts` + `route.ts` | Chat route updates |

### Web (New Files)
| File | Purpose |
|------|---------|
| `apps/web/__tests__/sessions/parser.test.ts` | Parser tests for Claude JSONL |
| `apps/web/__tests__/sessions/scanner.test.ts` | Session scanner tests |
| `apps/web/__tests__/__snapshots__/omnibox-snapshot.test.tsx.snap` | Omnibox snapshot |
| `apps/web/components/ui/error-boundary.tsx` | React error boundary component |
| `apps/web/lib/agents/parser.ts` | Agents CLI output parser |
| `scripts/axon-mcp` | Wrapper script for `axon-mcp` binary (sources `.env` automatically) |

### Test Updates
| File | Change |
|------|--------|
| `apps/web/__tests__/pulse/build-claude-args.test.ts` | Expanded (99 → ~200 lines) |
| `apps/web/__tests__/mcp/route.test.ts` | Expanded (188+ lines) |
| `apps/web/__tests__/agents/parser.test.ts` | Updated |
| Various `__tests__/*.test.ts` | Minor snapshot/assertion updates |

### Rust
| File | Change |
|------|--------|
| `crates/jobs/worker_lane.rs` | AMQP reconnect backoff constants + `claim_delivery()` helper |
| `crates/jobs/crawl/runtime/worker/loops.rs` | `join_all` for multi-lane, `run_watchdog_sweep()` helper |
| `crates/jobs/crawl/runtime.rs` + `process.rs` | Related crawl runtime cleanup |
| `crates/crawl/engine/collector.rs` | Crawl engine collector changes |
| `crates/mcp/server.rs` | Remove `respond_with_mode` from crawl/list/status/domains |
| `crates/mcp/schema.rs` | `#[allow(dead_code)]` + comment on `response_mode` fields |

### CHANGELOG
| File | Change |
|------|--------|
| `CHANGELOG.md` | Added `ebca63c` row; added "Security Hardening + Worker Resilience" highlights section |

---

## Commands Executed

```bash
# Orientation
git diff --stat HEAD
git log --oneline -5

# CHANGELOG update - manual Edit tool

# Staging + commit attempts
git add .
git commit -m "feat(web+jobs+mcp): ..."   # attempt 1 → Biome failure
git add <fixed files>
git commit -m "..."                         # attempt 2 → more Biome failures

# Biome auto-fix (run from apps/web/ directory)
npx biome check --write --unsafe \
  __tests__/sessions/parser.test.ts \
  __tests__/sessions/scanner.test.ts \
  lib/sessions/session-scanner.ts
# Result: Fixed 3 files

# Symlink creation for worktrees
cd /home/jmagar/workspace/axon_rust
for dir in .claude/worktrees/agent-a3f320ff/docs \
            .claude/worktrees/agent-a3f320ff/crates/web \
            .claude/worktrees/agent-a124e69c/docs \
            .claude/worktrees/agent-a124e69c/crates/web; do
  (cd "$dir" && ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md)
done

# Final commit (4th attempt)
git commit -m "feat(web+jobs+mcp): SSRF hardening..."
# Result: all hooks passed, 442 Rust tests green

# Push
git push
# Result: ebca63c..d9823b2 pushed to feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `--add-dir` in Pulse | Any path accepted | Only paths under `/home/node`, `/tmp`, `/workspace` allowed |
| `--dangerously-skip-permissions` | Always passed | Skipped if `PULSE_SKIP_PERMISSIONS=false` |
| `--allowedTools` / `--disallowedTools` | Raw string passed to Claude CLI | Filtered through regex; malformed entries dropped |
| MCP HTTP status probe | No SSRF protection | Blocks RFC-1918, localhost, IPv6 loopback/ULA |
| AMQP connection failure | Worker exits immediately | Exponential backoff 2s→60s before giving up |
| Worker lane count | Hardcoded to 2 via `tokio::join!(lane1, lane2)` | Dynamic: `join_all(1..=WORKER_CONCURRENCY)` |
| MCP crawl status/list/domains | Artifact-mode response via `respond_with_mode` | Always inline `AxonToolResponse::ok(...)` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `git log --oneline -3` | `d9823b2` at HEAD | `d9823b2 feat(web+jobs+mcp)...` | ✅ |
| Pre-commit Rust tests | 442 tests green | `running 442 tests... ok` | ✅ |
| Biome check | No errors | `Checked 34 files. No fixes applied.` | ✅ |
| `claude-symlinks` hook | All symlinks present | `OK — all CLAUDE.md files have valid AGENTS.md + GEMINI.md symlinks` | ✅ |
| `git push` | `ebca63c..d9823b2` pushed | Confirmed | ✅ |

---

## Source IDs + Collections Touched

None during this session (no Axon embed/retrieve calls prior to save-to-md).

---

## Risks and Rollback

- **SSRF guard (`validateAddDir`)**: New sessions that pass `--add-dir` paths outside the allowlist will silently have that flag dropped (no error returned to user). If a legitimate path is blocked, add it to `ALLOWED_DIR_ROOTS` in `claude-stream-types.ts:76`.
- **AMQP backoff**: Maximum 60s wait before worker exits on persistent AMQP failure. If RabbitMQ takes >60s to recover, workers will need manual restart. Adjust `AMQP_RECONNECT_MAX_SECS` in `worker_lane.rs:26`.
- **Rollback**: `git revert d9823b2` or `git push origin feat/crawl-download-pack:feat/crawl-download-pack --force-with-lease` after resetting locally. All changes are on feature branch, not main.

---

## Decisions Not Taken

- **`biome check --fix` (safe-only)**: Would not have caught `useTemplate` (classified unsafe). Needed `--unsafe` flag.
- **Manual template literal fixes**: Would have taken longer than auto-fix with no benefit.
- **Skipping `claude-symlinks` hook**: Considered but rejected — the hook exists for a reason and the fix was trivial (4 symlinks).

---

## Open Questions

- The `run_amqp_lane()` (86L) and `run_polling_lane()` (83L) functions in `worker_lane.rs` exceed the 80-line soft monolith warning. Should they be split in a follow-up refactor, or is the warning acceptable given they're still under the 120-line hard limit?
- Are there other active agent worktree directories (beyond `agent-a3f320ff` and `agent-a124e69c`) that might also be missing AGENTS.md/GEMINI.md symlinks?

---

## Next Steps

- PR for `feat/crawl-download-pack` → `main` when feature is complete
- Consider adding `PULSE_SKIP_PERMISSIONS` to `.env.example` documentation
- Consider exposing `AMQP_RECONNECT_INIT_SECS` / `AMQP_RECONNECT_MAX_SECS` as env vars for operator tuning
