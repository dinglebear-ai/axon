# Session: Claude Code Hooks & Automation Setup

**Date:** 2026-02-23
**Branch:** post-merge/stash-reapply
**Duration:** Full session

---

## Session Overview

Designed and implemented a comprehensive Claude Code automation layer for the axon_rust project. Starting from zero hooks, built a full `.claude/settings.json` with PreToolUse guards and PostToolUse quality checks, a language-agnostic monolith-check skill, and six supporting Python hook scripts. All hooks are wired, validated, and tested.

---

## Timeline

1. **Ran `/claude-code-setup:claude-automation-recommender`** — analyzed codebase, produced recommendations across hooks, skills, MCP servers, subagents
2. **Implemented `cargo check` PostToolUse hook** on every `.rs` edit
3. **Created `skills/monolith-check/`** — language-agnostic wrapper (Rust gets function-size enforcement, all others get file-size checks)
4. **Added `.env.example` hooks** — wrote `audit_env_vars.py` (duplicates + drift + credential leak), wired into PostToolUse
5. **Added `docker-compose.yaml` hooks** — `docker compose config --quiet` + wrote `audit_compose_images.py` (Docker Hub freshness check)
6. **Added CI workflow PreToolUse prompt** — confirms intent before editing `.github/workflows/**`
7. **Added `cargo fmt --check` and `cargo clippy`** to `.rs` PostToolUse
8. **Added `cargo audit` + `cargo deny check`** to `Cargo.toml` PostToolUse (graceful skip if not installed)
9. **Added s6 type file hard block** + s6 run/finish pairing prompt
10. **Parallelized the 4 `.rs` checks** — `check`, `fmt`, `monolith` in parallel; `clippy` after; ~1.7s total vs ~7s sequential
11. **Added three warning-only hook scripts**: `hook_unwrap_delta.py`, `hook_println_check.py`, `hook_command_docs.py`
12. **Added `rust-toolchain.toml` PreToolUse prompt**
13. **Added `hook_changelog_reminder.py`** — reminds to update CHANGELOG.md when crates/ code is edited
14. **Added `hook_justfile_lefthook_sync.py`** — warns when Justfile or lefthook.yml edited without the other

---

## Key Findings

- `cargo check` + `cargo fmt` + `cargo clippy` cannot truly parallelize (shared `target/` lock) — but `enforce_monoliths.py` (pure Python) can run in parallel with the cargo commands, saving ~5s
- `cargo audit` and `cargo deny` are not installed in this environment — hooks gracefully skip with `[skip] not installed` message
- `file_paths` in hook matchers IS supported by Claude Code (hooks are working correctly despite not being in main docs)
- `println!` in `crates/cli/commands/` is correct and intentional — the hook correctly skips this layer and only warns on internal crates (jobs, crawl, ingest, core internals)
- All 4 images in `docker-compose.yaml` are outdated: postgres 17→17.8-alpine, redis 8.2→8.2.4-alpine, rabbitmq 4.0→4.0.9-management, qdrant v1.13.1→v1.17.0
- `.env.example` had 3 duplicate vars (GITHUB_TOKEN, REDDIT_CLIENT_ID, REDDIT_CLIENT_SECRET) — fixed by removing the second block
- `ops_v2/` directory does not exist on this branch — the drift concern was a false alarm from stale memory

---

## Technical Decisions

- **Warning-only for quality nudges** (`unwrap_delta`, `println_check`, `command_docs`, `changelog_reminder`, `justfile_lefthook_sync`) — never block development flow, just inform
- **Hard block for structural files** (`Cargo.lock`, `s6/*/type`, `enforce_monoliths.py`) — these have no legitimate edit path
- **Prompt type for judgment calls** (`rust-toolchain.toml`, `.github/workflows/**`, `s6/*/run`) — requires human confirmation but doesn't auto-block
- **Separate hook scripts over inline shell** for complex logic — `hook_*.py` scripts are readable, testable, and independently runnable
- **Parallel shell execution** uses tmpdir + exit-code files pattern — avoids false positives from non-empty stdout (e.g. "passed" messages)

---

## Files Modified / Created

| File | Action | Purpose |
|------|--------|---------|
| `.claude/settings.json` | Created | Full hook configuration |
| `skills/monolith-check/SKILL.md` | Created | Skill definition — language-agnostic monolith check |
| `skills/monolith-check/check.py` | Created | Wrapper: Rust→enforcer, others→file size |
| `scripts/audit_env_vars.py` | Created | .env.example: duplicates, drift, credential leak |
| `scripts/audit_compose_images.py` | Created | docker-compose.yaml: Docker Hub image freshness |
| `scripts/hook_unwrap_delta.py` | Created | Warn on unwrap() count increase in internal crates |
| `scripts/hook_println_check.py` | Created | Warn on bare println! in internal library code |
| `scripts/hook_command_docs.py` | Created | Warn when CLI command has no matching docs/commands/*.md |
| `scripts/hook_changelog_reminder.py` | Created | Remind to update CHANGELOG.md on crates/ edits |
| `scripts/hook_justfile_lefthook_sync.py` | Created | Warn when Justfile/lefthook.yml edited without the other |
| `.env.example` | Modified | Removed duplicate ingest credentials block; added missing vars |

---

## Hook Summary

### PreToolUse (blocks/prompts before edit)

| Trigger | Type | Behavior |
|---------|------|----------|
| Edit/Write `Cargo.lock` | block | Machine-managed — use cargo add/update |
| Edit `scripts/enforce_monoliths.py` | block | Use .monolith-allowlist instead |
| Edit/Write `docker/s6/s6-rc.d/*/type` | block | Structural longrun files — breaks s6 supervision |
| Edit `docker/s6/s6-rc.d/*/run` | prompt | Ask whether paired finish script needs updating |
| Edit `rust-toolchain.toml` | prompt | Confirm reason and clippy impact of toolchain bump |
| Edit/Write `.github/workflows/**` | prompt | Confirm enhancing vs weakening CI |

### PostToolUse (audits after edit)

| Trigger | Commands | Notes |
|---------|----------|-------|
| Edit/Write `**/*.rs` | check+fmt+monolith (parallel) → clippy → unwrap_delta → println_check → command_docs → changelog_reminder | ~1.7s total |
| Edit `Cargo.toml` | cargo audit + cargo deny check | Graceful skip if not installed |
| Edit/Write `.env.example` | audit_env_vars.py | Duplicates + drift + credential leak |
| Edit/Write `docker-compose.yaml` | docker compose config + audit_compose_images.py | Syntax + Docker Hub freshness |
| Edit `Justfile` or `lefthook.yml` | hook_justfile_lefthook_sync.py | Warns if counterpart not also modified |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `python3 -c "import json; json.load(open('.claude/settings.json'))"` | JSON valid | JSON valid | ✅ |
| `python3 scripts/audit_env_vars.py` | audit passed | Duplicates: none, Drift: none, Credential leak: none — passed | ✅ |
| `docker compose config --quiet` | config valid | config valid | ✅ |
| `python3 skills/monolith-check/check.py --file scripts/audit_env_vars.py` | 207 lines OK | 207 lines (OK) — passed | ✅ |
| Parallel hook timing | <2s | 1.7s | ✅ |
| `hook_command_docs.py` on `scrape.rs` | warn: no scrape.md | `[docs] crates/cli/commands/scrape.rs has no docs/commands/scrape.md` | ✅ |
| `hook_command_docs.py` on `ask.rs` (has ask.md) | silent | silent | ✅ |
| `hook_changelog_reminder.py` on `crates/jobs/common.rs` | reminder fires | `[changelog] CHANGELOG.md unchanged...` | ✅ |
| `hook_justfile_lefthook_sync.py` on `Justfile` | warn about lefthook.yml | `[sync] Edited Justfile but lefthook.yml is unchanged...` | ✅ |

---

## Behavior Changes (Before/After)

- **Before:** No automated quality feedback during Claude Code editing sessions
- **After:** Every `.rs` edit triggers compile check, fmt check, clippy, monolith policy, unwrap delta, println check, command docs check, and changelog reminder — all within ~2s
- **Before:** `.env.example` could drift silently from code
- **After:** Any `.env.example` edit triggers full audit — duplicates, code drift, credential leak detection
- **Before:** `Cargo.lock`, `enforce_monoliths.py`, and s6 type files could be accidentally edited
- **After:** Hard blocked with clear error messages

---

## Risks and Rollback

- **Risk:** Hook overhead adds ~2s per `.rs` edit. Acceptable since hooks don't block Claude's planning.
- **Risk:** `changelog_reminder` fires on every `.rs` edit until CHANGELOG.md is touched — could become noise in long sessions. Mitigation: warning-only, easy to ignore.
- **Rollback:** Delete `.claude/settings.json` to disable all hooks instantly. Scripts in `scripts/hook_*.py` are standalone and cause no harm if left in place.

---

## Decisions Not Taken

- **Block `.env` writes** — user explicitly rejected this; `.env` is the live config and must remain editable
- **Hard-block CI workflow edits** — user wanted prompt/confirmation instead of a hard block to allow legitimate enhancements
- **`cargo clippy` truly parallel with check/fmt** — impossible due to shared `target/` lock; runs sequentially after the parallel group
- **ops/ops_v2 drift hook** — `ops_v2/` doesn't exist on this branch; hook would have been a false alarm
- **CHANGELOG.md direct edit block** — not machine-generated, legitimate to edit by hand

---

## Open Questions

- `cargo audit` and `cargo deny` not installed — need `cargo install cargo-audit cargo-deny` to activate those hooks
- Docker image versions are outdated (postgres, redis, rabbitmq, qdrant) — separate task to update docker-compose.yaml
- `enforce_no_legacy_symbols.py` referenced in Justfile precommit recipe but file doesn't exist — dead reference

---

## Next Steps

- Install `cargo-audit` and `cargo-deny`: `cargo install cargo-audit cargo-deny`
- Update Docker image tags in `docker-compose.yaml` (postgres:17.8-alpine, redis:8.2.4-alpine, rabbitmq:4.0.9-management, qdrant/qdrant:v1.17.0)
- Create `scripts/enforce_no_legacy_symbols.py` or remove the dead reference from Justfile
- Consider adding `docs/commands/` entries for the 12 undocumented commands (scrape, crawl, map, batch, extract, embed, query, retrieve, evaluate, suggest, sources, domains, stats, status, doctor, debug)
