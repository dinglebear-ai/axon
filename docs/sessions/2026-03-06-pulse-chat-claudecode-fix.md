# Session: Pulse Chat CLAUDECODE Env Fix + Push (v0.7.2)

**Date:** 2026-03-06
**Branch:** feat/services-layer-refactor
**Commit:** 107d2a6c
**Version bump:** 0.7.1 → 0.7.2

---

## Session Overview

Fixed the root cause of Pulse Chat `session/new` failures in local dev, verified end-to-end streaming worked in the web UI, then pushed all changes via `quick-push`. The root cause was the `CLAUDECODE` environment variable being inherited from the parent Claude Code session and blocking the `claude` CLI subprocess inside `claude-agent-acp`.

---

## Timeline

1. **Investigation**: `session/new` returned "Query closed before response received"
2. **Network check**: `api.claude.ai` → NXDOMAIN; `api.anthropic.com` → 404 (reachable). Network not the issue.
3. **Process inspection**: Found running `claude` process + Zed's `claude-agent-acp` v0.20.2
4. **Debug logging**: Enabled DEBUG on `claude-agent-acp`; saw: `"Claude Code cannot be launched inside another Claude Code session. Nested sessions share runtime resources... To bypass this check, unset the CLAUDECODE environment variable."`
5. **Fix applied**: Added `command.env_remove("CLAUDECODE")` to `spawn_adapter()` in `crates/services/acp.rs`
6. **Manual verification**: `env -u CLAUDECODE /usr/local/bin/claude-agent-acp` → `session/new` succeeded
7. **End-to-end test**: curl to `http://localhost:3000/api/pulse/chat` → streaming NDJSON with correct text
8. **User confirmed**: "tru its workin great job"
9. **`quick-push` invoked**: Version bump 0.7.1→0.7.2, CHANGELOG update, commit attempt
10. **Monolith check failed**: `route.ts` at 504 effective lines (limit 500) — removed debug block + collapsed ternary → 500
11. **Test failure**: `allowed_flags_all_cli_flags_start_with_double_dash` panicked on `("agent", "agent")` entries in `constants.rs`
12. **Fix**: Removed 3 pulse_chat flags from `ALLOWED_FLAGS` (they're direct-dispatch, not subprocess CLI args)
13. **All 846 tests pass**, commit and push succeeded

---

## Key Findings

- **Root cause of `session/new` failure**: `CLAUDECODE` env var is set by Claude Code on all child processes. When `axon serve` (running inside a Claude Code session) spawns `claude-agent-acp`, which in turn spawns the `claude` CLI, the CLI detects the nested session and exits with code 1. The error message is only visible in DEBUG mode.
- **Fix location**: `crates/services/acp.rs` in `AcpClientScaffold::spawn_adapter()` — same block that strips `OPENAI_BASE_URL`/`OPENAI_API_KEY`/`OPENAI_MODEL`.
- **`ALLOWED_FLAGS` contract**: `crates/web/execute/tests/ws_protocol_tests.rs:151` enforces that ALL entries in `ALLOWED_FLAGS` have a CLI flag value starting with `--`. Direct-dispatch-only flags must NOT be in this list.
- **Monolith limit**: `apps/web/app/api/pulse/chat/route.ts` was 504 effective lines after prior session's additions; trimmed to exactly 500 by removing a debug log block and collapsing a ternary.

---

## Technical Decisions

- **`env_remove("CLAUDECODE")` over `CLAUDECODE=`**: Removing the var entirely is the recommended approach per the error message ("unset the CLAUDECODE environment variable"). Setting it to empty string might not satisfy the check.
- **Remove pulse_chat flags from `ALLOWED_FLAGS` entirely**: These flags (`agent`, `model`, `session_id`) are extracted in `extract_params()` for direct-dispatch logic, never passed to a subprocess. Keeping them in `ALLOWED_FLAGS` was incorrect and violated the test contract.
- **Collapse ternary vs. refactor**: Chose minimal code change to stay within monolith limit rather than extracting a new function (YAGNI; the function is called once).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/acp.rs` | Added `command.env_remove("CLAUDECODE")` in `spawn_adapter()` |
| `crates/web/execute/constants.rs` | Removed 3 pulse_chat direct-dispatch flags from `ALLOWED_FLAGS` |
| `apps/web/app/api/pulse/chat/route.ts` | Removed debug log block; collapsed `truncateForLog` ternary to stay ≤500 lines |
| `Cargo.toml` | Version bumped 0.7.1 → 0.7.2 |
| `CHANGELOG.md` | Added v0.7.2 entry |
| `docker/s6/cont-init.d/17-materialize-claude-credentials` | New: stages Claude credentials in container (from prior session) |
| `docker-compose.yaml` | Mounts `~/.claude` read-only into `axon-workers` (from prior session) |
| `docker/Dockerfile` | Minor update (from prior session) |

---

## Commands Executed

```bash
# Manual verification before fix
env -u CLAUDECODE /usr/local/bin/claude-agent-acp
# → session/new succeeded with full NDJSON response

# End-to-end curl test
curl -s -N -X POST http://localhost:3000/api/pulse/chat \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"hello"}],"agent":"claude"}'
# → Streaming NDJSON, text: "Hello. What can I help you build?"

# Final commit (after all pre-commit hooks passed)
git commit → [feat/services-layer-refactor 107d2a6c]
git push → 107d2a6c pushed to origin/feat/services-layer-refactor
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `session/new` in local dev | "Query closed before response received" (exit code 1 from `claude` CLI) | Succeeds; returns session ID + NDJSON stream |
| Pulse Chat response in web UI | Error / no response | Clean streaming response |
| `ALLOWED_FLAGS` test | Fails: `(agent, agent)` not `--`-prefixed | Passes: flags removed from list |
| `route.ts` monolith check | Fails: 504 effective lines | Passes: 500 effective lines |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `env -u CLAUDECODE claude-agent-acp` → session/new | session ID returned | Full NDJSON response | ✅ |
| `curl …/api/pulse/chat` | Streaming text | "Hello. What can I help you build?" | ✅ |
| `cargo test` (846 tests) | 0 failures | 0 failures | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings (pre-commit) | ✅ |
| `monolith` check | ≤500 lines | 500 lines for route.ts | ✅ |
| `git push` | Success | 107d2a6c pushed | ✅ |

---

## Source IDs + Collections Touched

None this session (no axon embed/crawl/scrape operations performed).

---

## Risks and Rollback

- **`env_remove("CLAUDECODE")`**: Low risk. If the `claude` CLI changes its nested-session detection behavior, this removal is harmless. To rollback: revert `crates/services/acp.rs` to the prior commit.
- **Removing pulse_chat flags from `ALLOWED_FLAGS`**: Zero risk for subprocess security (flags were never reaching subprocess). Direct-dispatch logic in `sync_mode.rs` extracts them from the raw WS params dict, not from `ALLOWED_FLAGS`.

---

## Decisions Not Taken

- **Set `CLAUDECODE=""` instead of unsetting**: Rejected — error message says "unset", and empty string might be truthy in the check.
- **Add pulse_chat flags to `ALLOWED_FLAGS` with `--` prefix**: Rejected — these params are consumed before subprocess dispatch; adding `--agent`, `--model` CLI flags to the subprocess would be wrong.
- **Refactor `route.ts` below 500 lines**: Rejected — extracting helpers adds indirection for a one-off function; minimally trimmed instead.

---

## Open Questions

- Whether `CLAUDECODE` removal also prevents issues when axon runs as a systemd/Docker service (not inside a Claude Code session) — likely harmless but untested in that path.
- GitHub Dependabot flagged 5 vulnerabilities (2 high, 3 moderate) on the default branch. These are pre-existing and not introduced this session.

---

## Next Steps

- Open a PR from `feat/services-layer-refactor` → `main`
- Address Dependabot vulnerabilities on default branch
- Consider adding a test that spawns `claude-agent-acp` with `CLAUDECODE` set to confirm the env_remove takes effect
