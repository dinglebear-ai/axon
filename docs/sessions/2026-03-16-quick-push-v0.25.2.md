# Session: quick-push v0.25.2
Date: 2026-03-16
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Executed `/quick-push` to commit and push 57 changed/new files accumulated on the `feat/pulse-shell-and-hybrid-search` branch. Version bumped from `0.25.1` → `0.25.2` (patch/refactor). Resolved a monolith policy violation before commit succeeded.

## Timeline

| Time | Activity |
|------|----------|
| Start | Ran `git log --oneline -5` + `git diff --stat HEAD` — 57 files, 4301 insertions / 783 deletions |
| +1m | Read Cargo.toml version (`0.25.1`), CHANGELOG.md structure |
| +2m | Bumped version to `0.25.2` in Cargo.toml; ran `cargo check` to update Cargo.lock |
| +3m | Updated CHANGELOG.md — new `[0.25.2]` section with highlights, `(pending)` row in 0.25.1 section removed |
| +4m | Staged all + first commit attempt → **pre-commit hook blocked**: monolith violation `axon-message-list.tsx: 509 lines (limit 500)` |
| +6m | Extracted `ToolStepDetail` + `ToolCallsGroup` into new `axon-message-tool-calls.tsx`; updated imports in `axon-message-list.tsx` |
| +7m | Re-verified: `enforce_monoliths.py --staged` → only warning (`ingest_github()` 86L, limit 120L), no violations |
| +8m | Staged extracted files, committed successfully (`89d009c5`) |
| +9m | Amended CHANGELOG.md with real SHA `89d009c5`; second commit `f8f387bc` |
| +10m | `git push` → remote `feat/pulse-shell-and-hybrid-search` updated (`7b173bf8..f8f387bc`) |

## Key Findings

- `axon-message-list.tsx` was 509 counted lines (526 raw; monolith script excludes blank lines). Limit is 500.
- `ToolStepDetail` (lines 20–62) and `ToolCallsGroup` (lines 64–118) were self-contained helper components with no callers outside the file — clean extraction with zero logic change.
- `FileCode2` import moved to `axon-message-tool-calls.tsx` (was only used in `ToolStepDetail`); removed from `axon-message-list.tsx`.
- `buildToolHeader` import also moved to the new file; `PulseToolUse` type import similarly.
- `ingest_github()` at `crates/ingest/github.rs:250` is 86 lines (warning threshold 80, hard limit 120) — warning only, commit proceeds.

## Technical Decisions

- **Extract to `axon-message-tool-calls.tsx`** rather than `axon-message-tool-step.tsx` or similar — name reflects the full feature scope (ToolStepDetail is an implementation detail of ToolCallsGroup).
- **Patch bump (0.25.1 → 0.25.2)** — changes are predominantly refactor/split/hardening; no new user-visible features beyond what 0.25.1 already described.
- **Two-commit strategy for CHANGELOG SHA** — necessary because SHA is only known after commit; second commit is minimal and passes all hooks cleanly.

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | Modified | Version bump 0.25.1 → 0.25.2 |
| `Cargo.lock` | Modified | Updated by `cargo check` with new version |
| `CHANGELOG.md` | Modified | New [0.25.2] section + SHA backfill |
| `apps/web/components/shell/axon-message-list.tsx` | Modified | Remove ToolStepDetail/ToolCallsGroup, import from new file |
| `apps/web/components/shell/axon-message-tool-calls.tsx` | Created | ToolStepDetail + ToolCallsGroup components extracted |
| `apps/web/components/shell/axon-shell-{desktop,mobile,conversation-pane,right-pane,sidebar-pane}.tsx` | Created | Shell component splits |
| `apps/web/__tests__/axon-shell-state.test.ts` | Created | Shell state tests |
| `apps/web/__tests__/axon-shell.test.tsx` | Created | Shell component tests |
| `apps/web/__tests__/ws-messages-{actions,pulse,subscription,tracked}.test.ts` | Created | ws-messages hook tests |
| `apps/web/hooks/ws-messages/{pulse,subscription,tracked,provider-actions,provider-effects,provider-runtime}.ts` | Created | ws-messages provider splits |
| `crates/cli/commands/common/job_output.rs` | Created | JobStatus trait + impl_job_status! macro |
| `crates/cli/commands/common/url_inputs.rs` | Created | Positional + --urls CSV input merge utility |
| `crates/core/config/cli.rs` | Modified | TextArg → GraphArgs with GraphSubcommand |
| `crates/core/config/parse.rs` + `build_config.rs` + `helpers.rs` | Modified | Config parsing hardening |
| `crates/crawl/engine.rs` + `engine/collector.rs` | Modified | Crawl engine hardening |
| `crates/ingest/github.rs` + `files.rs` + `files/batch.rs` | Modified | Re-exports, batch hardening |
| `crates/jobs/crawl/runtime.rs` + `worker/job_context.rs` + `result_builder.rs` | Modified | Crawl worker hardening |
| `crates/jobs/{embed,extract,refresh}.rs` + `ingest/schema.rs` | Modified | Jobs hardening |
| `crates/services/graph.rs` + `system.rs` + `watch.rs` | Modified | Service layer additions |
| `crates/vector/ops/qdrant.rs` + `qdrant/client.rs` | Modified | qdrant_scroll_pages_while, env_usize_clamped |
| `tests/services_system_services.rs` | Modified | Tests updated for new system service API |

## Commands Executed

```bash
git log --oneline -5
git diff --stat HEAD
grep -m1 '^version' Cargo.toml                    # → 0.25.1
cargo check --quiet                               # update Cargo.lock, exit 0
python3 scripts/enforce_monoliths.py --staged     # → 509 lines violation
wc -l apps/web/components/shell/axon-message-list.tsx  # → 526 (raw)
python3 scripts/enforce_monoliths.py --staged     # → only warning after extraction
git add . && git commit [first attempt → hook blocked]
git add <extracted files> && git commit           # → 89d009c5 ✓
git push                                          # → 7b173bf8..f8f387bc ✓
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon-message-list.tsx` | 509 counted lines (monolith violation) | ~410 counted lines; ToolStepDetail/ToolCallsGroup in new file |
| Version | 0.25.1 | 0.25.2 |
| Graph CLI | `graph` took `TextArg` (free text) | `graph` takes `GraphArgs` with typed subcommands |
| Job output | Repeated display logic per job type | `JobStatus` trait + `impl_job_status!` macro shared across types |
| Qdrant scroll | `qdrant_scroll_pages` (unbounded) for domains | `qdrant_scroll_pages_while` with 10k cap for detailed domains |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | exit 0 | exit 0, no output | ✅ |
| `enforce_monoliths.py --staged` (after extraction) | no FILE violations | only warning for ingest_github() | ✅ |
| `git commit` | success | `89d009c5` created | ✅ |
| `git push` | remote updated | `7b173bf8..f8f387bc` | ✅ |

## Source IDs + Collections Touched

_(Session doc embed — see below after Axon embed attempt)_

## Risks and Rollback

- **Monolith extraction** is purely structural — no logic changed. `ToolStepDetail` and `ToolCallsGroup` are exported from the new file and imported by `axon-message-list.tsx`. Rollback: merge the two files back.
- **Version bump** is in Cargo.toml only; no tagged release was created.
- **Rollback**: `git revert HEAD~1 HEAD` (reverts both commits), `git push --force-with-lease` on this branch (feature branch only, not main).

## Decisions Not Taken

- **Skip version bump** — rejected; CLAUDE.md requires semver bump on every push.
- **Bump to 0.26.0 (minor)** — considered, but the primary changes are refactor/split, not new features. The actual new features (GraphArgs subcommand, ws-messages hooks, job_output utilities) are additive infrastructure, not user-visible feature additions.
- **Add to `.monolith-allowlist`** — explicitly rejected by CLAUDE.md ("Split the file — do not add allowlist exceptions").

## Open Questions

- Biome `noImportantStyles` warnings in `globals.css:548` — pre-commit output showed warnings but did not block commit. Should these be fixed proactively?
- GitHub dependabot flagged 14 vulnerabilities (7 high, 7 moderate) on the default branch — unrelated to this session but worth tracking.

## Next Steps

- Run `pnpm test` in `apps/web/` to verify the new ws-messages test suite passes cleanly.
- Address Biome `!important` warnings in CSS files if they become blocking.
- Consider opening a PR from `feat/pulse-shell-and-hybrid-search` → `main` once the branch is stable.
