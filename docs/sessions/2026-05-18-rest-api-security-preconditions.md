---
date: 2026-05-18 00:19:00 EST
repo: git@github.com:jmagar/axon.git
branch: feat/rest-api-security-preconditions
head: 609a0b39
plan: none (executed from bead axon_rust-2qva.1)
agent: Claude (claude-sonnet-4-6)
session id: 4a6a85d2-a111-4bc6-86d5-aea70dc2bf40
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon-rust/4a6a85d2-a111-4bc6-86d5-aea70dc2bf40/
working directory: /home/jmagar/workspace/axon_rust/.worktrees/rest-security-2qva1
worktree: /home/jmagar/workspace/axon_rust/.worktrees/rest-security-2qva1  609a0b39 [feat/rest-api-security-preconditions]
pr: "#105 — feat!: security hardening — required_scope catch-all, LoopbackDev bypass, scope promotions (v4.0.0) — https://github.com/jmagar/axon/pull/105"
---

## User Request

Execute bead `axon_rust-2qva.1` via `/work-it` — three security pre-conditions required before the REST API surface expansion (epic `axon_rust-2qva`): fix `required_scope` catch-all, add unconditional auth for destructive ops in LoopbackDev mode, and promote Ask/Research/Evaluate/Suggest to `axon:write` scope.

## Session Overview

Implemented all three security fixes in an isolated worktree, created PR #105, ran lavra-review + code-simplicity review, addressed all findings from both agents (Debug scope correction, ElicitDemo explicit arm, wildcard removal for compiler-enforced exhaustiveness, invariant comments, tighter test assertions), and resolved all 4 PR review threads from cubic-dev-ai, copilot, and coderabbit. Final state: 1842 tests passing, PR #105 clean.

## Sequence of Events

1. Created worktree `.worktrees/rest-security-2qva1` on branch `feat/rest-api-security-preconditions`
2. Read bead `axon_rust-2qva.1` description and located exact code targets in `src/services/action_api.rs` and `src/web/actions.rs`
3. **F1**: Changed `_ => None` to `_ => Some("axon:write")` in `required_scope()`
4. **F10**: Moved `Ask`, `Evaluate`, `Suggest`, `Research` from `axon:read` arm to a new `axon:write` arm
5. **F5**: Added `requires_unconditional_auth` check in `authorize_action()` before the `auth_required` gate
6. Added 3 integration tests (migrate loopback, dedupe loopback, ask no-token)
7. Updated CHANGELOG.md (breaking change entry), bumped Cargo.toml + README.md to v4.0.0
8. All hooks passed; committed and pushed; created PR #105
9. Ran lavra-review (security-sentinel + code-simplicity-reviewer) in parallel
10. Addressed all findings: Debug→write, ElicitDemo explicit, wildcard removed (compiler exhaustiveness), invariant comments, tighter 401 assertions, import cleanup
11. Discovered `crate::mcp::schema::AxonRequest` path was redundant after adding `use` import — fixed clippy warning
12. All 3 external reviewers (cubic-dev-ai, copilot, coderabbit) flagged `ask_requires_write_scope` as not testing 403 path
13. Added `authorize_action_read_only_token_forbidden_for_ask` unit test constructing `AuthContext{scopes:["axon:read"]}` and asserting ask/evaluate/suggest/research are denied
14. Also synced `apps/web/package.json` to 4.0.0
15. All 4 PR threads resolved; verification clean

## Key Findings

- `src/services/action_api.rs:85` — wildcard `_ => None` caused complete auth bypass for any unrecognised `AxonRequest` variant via `authorize_action`'s `None`-check at `src/web/actions.rs:199`
- `src/web/actions.rs:190` — `if !state.auth_required { return Ok(()); }` allowed destructive ops (migrate/dedupe) to run without credentials in LoopbackDev mode
- `Debug(_)` was in the `axon:read` arm despite triggering Gemini headless completions (external process, API quota) — corrected to `axon:write`
- `ElicitDemo` was the only variant hitting the wildcard — added explicit arm, then removed the wildcard entirely so the compiler enforces scope assignment for future variants
- The test infra (`build_auth_layer` with static token) always grants both `axon:read` AND `axon:write` scopes, making integration-level 403 tests impossible without directly constructing `AuthContext` — solved with unit test approach

## Technical Decisions

- **Removed wildcard arm** rather than keeping `_ => Some("axon:write")`: compiler exhaustiveness is a stronger guarantee than a runtime catch-all. Adding a new `AxonRequest` variant without a `required_scope` arm now fails to compile, preventing silent auth misconfigurations.
- **Unit test for 403 path** rather than integration test: the test server infra always grants both scopes, making a real 403 integration test structurally impossible without refactoring `spawn_test_server`. Directly constructing `AuthContext{scopes:["axon:read"]}` and exercising the scope logic is equivalent and more targeted.
- **Breaking change → v4.0.0**: scope promotion of Ask/Evaluate/Suggest/Research is a breaking API contract change per project versioning rules (`feat!` → major). Any `axon:read` token calling these actions will now receive 403 without credential reissuance.
- **`requires_unconditional_auth` as local variable** rather than a separate function: the guard is self-contained and the comment + inline pattern match is clearer than extracting a helper.

## Files Modified

| File | Purpose |
|------|---------|
| `src/services/action_api.rs` | F1: wildcard `_ → Some("axon:write")`; F10: Ask/Evaluate/Suggest/Research/Debug → write; ElicitDemo explicit; invariant comment; wildcard removed |
| `src/services/action_api_tests.rs` | 4 unit tests: `required_scope_ask_evaluate_suggest_research_are_write`, `required_scope_migrate_dedupe_are_write`, `required_scope_elicit_demo_is_write`, `required_scope_read_only_ops_are_read` |
| `src/web/actions.rs` | F5: `requires_unconditional_auth` guard; `use crate::mcp::schema::AxonRequest` import; invariant comment linking to action_api.rs |
| `src/web/actions/tests.rs` | 3 integration tests + 1 unit test; tightened 401 assertions; scope-boundary 403 unit test |
| `CHANGELOG.md` | v4.0.0 breaking change entry with migration note |
| `Cargo.toml` | Version bump 3.0.0 → 4.0.0 |
| `README.md` | Version bump 3.0.0 → 4.0.0 |
| `apps/web/package.json` | Version bump 3.0.0 → 4.0.0 |

## Commands Executed

```bash
# Worktree creation
git worktree add -b feat/rest-api-security-preconditions .worktrees/rest-security-2qva1 HEAD

# Focused test runs
cargo test --lib -- migrate_requires_auth  # → 1 passed
cargo test --lib -- dedupe_requires_auth ask_requires  # → 2 passed
cargo test --lib -- required_scope_ask required_scope_migrate  # → 4 passed
cargo test --lib  # → 1842 passed, 6 ignored

# Clippy (with strict flags matching lefthook)
cargo clippy --workspace --all-targets --locked -- -D warnings  # → clean after fixing qualified path

# PR creation
gh pr create --title "feat!: security hardening ..."  # → #105
```

## Errors Encountered

- **`AuthPolicy::Token` does not exist**: test used wrong enum variant. Root cause: miremembered the variants. Fixed to `AuthPolicy::Mounted { auth_state: None }`.
- **`AskRequest` field name mismatch** (`question` vs `query`): test helper used wrong field names from an older schema. Fixed by reading `src/mcp/schema.rs` for actual fields.
- **`AskRequest: Default not satisfied`**: request structs don't derive `Default`. Fixed by using explicit field initialization.
- **Clippy `unnecessary_qualification`**: after adding `use crate::mcp::schema::AxonRequest`, the function signature still had the full path. Fixed by `sed` substitution.
- **Wildcard unreachable**: once `ElicitDemo` was explicit, `_ => Some("axon:write")` became unreachable. Resolved by removing the wildcard entirely (desirable — compiler enforces exhaustiveness).

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Unrecognised `AxonRequest` variant | auth bypassed (`_ => None` → `Ok(())`) | requires `axon:write` scope (compile-enforced exhaustiveness) |
| `action:migrate` in LoopbackDev (no token) | allowed (no auth gate) | returns 401 UNAUTHORIZED |
| `action:dedupe` in LoopbackDev (no token) | allowed (no auth gate) | returns 401 UNAUTHORIZED |
| `action:ask` with `axon:read` token | allowed | returns 403 FORBIDDEN |
| `action:evaluate` with `axon:read` token | allowed | returns 403 FORBIDDEN |
| `action:suggest` with `axon:read` token | allowed | returns 403 FORBIDDEN |
| `action:research` with `axon:read` token | allowed | returns 403 FORBIDDEN |
| `action:debug` with `axon:read` token | allowed | returns 403 FORBIDDEN |
| Adding new `AxonRequest` variant without scope | silent auth bypass | compile error (exhaustiveness) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib -- migrate_requires_auth` | 1 passed | 1 passed | ✅ |
| `cargo test --lib -- dedupe_requires_auth ask_requires` | 2 passed | 2 passed | ✅ |
| `cargo test --lib -- required_scope_ask required_scope_migrate required_scope_wildcard required_scope_read` | 4 passed | 4 passed | ✅ |
| `cargo test --lib -- authorize_action_read_only ask_requires` | 2 passed | 2 passed | ✅ |
| `cargo test --lib` (full suite) | all passed | 1842 passed, 6 ignored | ✅ |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean | clean | ✅ |
| PR review threads | 0 open | 0 open | ✅ |

## Risks and Rollback

- **Breaking change**: Any integration with `axon:read` tokens calling ask/evaluate/suggest/research via the HTTP API will receive 403 after deploying. CHANGELOG.md documents the migration path ("re-issue tokens with axon:write scope"). No automated rollback — token reissuance is manual.
- **Rollback**: `git revert df56854e a05d9ac2 609a0b39` reverts all three commits. Version-bearing files would need manual restoration to 3.0.0.
- **MCP stdio path not covered**: The `requires_unconditional_auth` guard is only in `src/web/actions.rs` (REST/HTTP path). MCP stdio always uses `LoopbackDev` and calls `dispatch_action` directly without `authorize_action`. Migrate/dedupe via `axon mcp` CLI are still gated only by the existing `required_scope` check, not the new unconditional guard. This is acceptable (stdio = local process trust) but worth documenting.

## Decisions Not Taken

- **Integration test for 403 path**: Considered refactoring `spawn_test_server` to accept a custom scope list. Rejected — the test infra coupling was too broad; direct `AuthContext` construction in a unit test is cleaner and equally effective.
- **`requires_unconditional_auth` as a separate function** (`fn is_always_auth_required(action: &AxonRequest) -> bool`): Rejected — a local variable with an inline comment is more readable for a one-off guard in a short function.
- **Keeping the wildcard `_ => Some("axon:write")`**: Rejected after clippy flagged it as unreachable once `ElicitDemo` was explicit. Compiler exhaustiveness is strictly better than a runtime fallback.

## Open Questions

- Should `action:debug` ever be `axon:read`? It currently triggers Gemini LLM-assisted troubleshooting (per CLAUDE.md), so `axon:write` is correct. But lightweight "doctor-only" debug (no LLM) could potentially be read-scoped in the future — would require splitting the action.
- MCP stdio path for migrate/dedupe: the unconditional auth guard was intentionally scoped to the HTTP action API. Should a future bead add equivalent protection to the MCP stdio dispatch path, or is process isolation sufficient?

## Next Steps

**Unfinished from this session:**
- None — bead `axon_rust-2qva.1` is complete and PR #105 is clean.

**Follow-on tasks (not started):**
- `axon_rust-2qva.2` — SSRF fix: `validate_url_with_dns` in `services/scrape.rs` + `services/map.rs` + 2s timeout (Wave 1, unblocked)
- `axon_rust-2qva.4` — HttpError type + expand `taxonomy.rs` (Wave 1, unblocked)
- Close bead `axon_rust-2qva.1` after PR #105 is merged to main
- Merge PR #105 to main once CI passes
