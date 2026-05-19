# Session: CLAUDE.md Audit and Corrections
**Date:** 2026-02-19
**Branch:** `chore/housekeeping`
**Preceded by:** `docs/sessions/2026-02-19-readme-audit-and-rewrite.md`

---

## Session Overview

Applied the `claude-md-management:claude-md-improver` skill to audit and correct `CLAUDE.md`. All 28 changes were factual corrections (no generic additions) — primarily propagating the same stale data that had already been corrected in `README.md` during the previous session. The key issue was that `CLAUDE.md` still described a `cortex` binary that doesn't exist.

---

## Timeline

| Step | Activity |
|------|----------|
| 1 | Ran `claude-md-improver` skill — discovery + quality assessment |
| 2 | Found 1 CLAUDE.md at repo root (no package-level files) |
| 3 | Assessed quality: **38/100 (D)** — commands non-executable due to wrong binary name |
| 4 | Presented quality report with proposed diffs — user approved |
| 5 | Applied 28 targeted edits to `CLAUDE.md` |
| 6 | Verified no remaining `cortex` references (only legitimate: `--collection cortex` default) |

---

## Key Findings

1. **Binary name pervaded CLAUDE.md**: `cortex` appeared 14 times in copy-paste-ready commands — every single one broken. The binary is `axon` per `Cargo.toml:9-11`.
2. **Score 0/15 on Currency criterion**: Six distinct wrong facts in a single file — wrong binary, wrong defaults, wrong image, wrong network, wrong credentials, wrong function name.
3. **`common.rs` function wrong**: CLAUDE.md claimed `run_embed_and_save()` — actual functions are URL parsing utilities (`parse_urls`, `expand_url_glob_seed`). Confirmed by direct read.
4. **Architecture tree 2 subdirs behind**: `crawl_jobs/` and `ops/` absent; `s6-rc.d/` documented as `services.d/`.
5. **Performance table incomplete**: Missing `Backfill concurrency` column (third concurrency axis existed in `config.rs` but not documented).

---

## Technical Decisions

1. **Factual corrections only**: Did not add generic content or best-practice boilerplate — only fixed what was demonstrably wrong against the codebase.
2. **Kept DB schema section intact**: All 4 schemas were verified accurate in the README audit; no changes needed.
3. **Kept Gotchas section intact**: All gotchas were verified accurate in the README audit; no changes needed.
4. **Added `probe.rs` to architecture tree**: It's not a command but is a real module (`probe.rs` exists) used by `doctor.rs` — appropriate to document in the file tree.

---

## Files Modified

| File | Changes |
|------|---------|
| `CLAUDE.md` | 28 corrections: binary name (×14), defaults (×3), Docker infra (×3), architecture tree (×4), performance table (×1), added `debug` command + `recover` subcommand |

---

## Commands Executed

None (this was a documentation-only session — no build/test commands run).

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Title | `Cortex CLI` | `Axon CLI` |
| Build commands | `cargo build --bin cortex` (×3) | `cargo build --bin axon` |
| Binary alias note | "Two aliases: cortex + axon" | "Binary is `axon`" |
| `debug` in commands table | Absent | Present |
| `recover` in job subcommands | Absent | Present |
| Job subcommand examples | `cortex crawl ...` (×6) | `axon crawl ...` |
| `--max-pages` default | `200` | `0` (uncapped) |
| `--collection` default | `spider_rust` | `cortex` |
| AMQP fallback creds | `guest:guest` | `axon:axonrabbit` |
| `common.rs` description | `run_embed_and_save()` | URL parsing utilities |
| `jobs/` crate tree | 4 files | `common.rs` + `crawl_jobs_dispatch.rs` + `crawl_jobs/` added |
| `vector/` crate tree | `mod.rs` + `ops.rs` | Added `ops_dispatch.rs` + `ops/` |
| s6 service dir | `services.d/` | `s6-rc.d/` |
| Redis image | `redis:7.4-alpine` | `redis:8.2-alpine` |
| Docker services table | 5 rows | 6 rows (`axon-webdriver` added) |
| Docker network | `cortex` | `axon` |
| Performance profiles | 5 columns | 6 columns (Backfill concurrency added) |
| Debug build | `./target/debug/cortex` | `./target/debug/axon` |
| `axon doctor` example | `cortex doctor` | `axon doctor` |
| Docker build gotcha | `--bin cortex` | `--bin axon`; Dockerfile path corrected |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| No remaining `cortex` refs in CLAUDE.md (except collection default) | 0 occurrences | 0 occurrences | PASS |
| Binary in Cargo.toml | `name = "axon"` | `Cargo.toml:10: name = "axon"` | PASS |
| `--collection` default in config.rs | `cortex` | `config.rs:347: default_value = "cortex"` | PASS |
| Redis in docker-compose | `redis:8.2-alpine` | `docker-compose.yaml:44` | PASS |
| Network name in docker-compose | `axon` | `docker-compose.yaml:157` | PASS |

---

## Source IDs + Collections Touched

| File | Collection | Chunks | Status |
|------|-----------|--------|--------|
| `docs/sessions/2026-02-19-readme-audit-and-rewrite.md` | `cortex` | 1 | Embedded + retrieved (previous session) |

This session's doc will be embedded below.

---

## Risks and Rollback

- **Risk**: `CLAUDE.md` is the primary AI context file. Incorrect information was corrected; no new information was added that could mislead future sessions.
- **Rollback**: `git checkout HEAD -- CLAUDE.md` restores the previous (inaccurate) version.
- **Note**: The README.md was also modified by a linter during this session — the Monolith Guardrails section updated file size limit from `400` → `500` lines and expanded exempt path patterns. This was an external change, not made by this session.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|----------------|
| Add environment variables section to CLAUDE.md | README already has comprehensive env var tables; CLAUDE.md's role is code patterns and commands, not env reference |
| Add Chrome flags to CLAUDE.md | Already added to README; CLAUDE.md is developer-facing, not user reference |
| Expand Gotchas section | All existing gotchas were accurate; no new ones were discovered that warranted addition |

---

## Open Questions

1. **`axon_main.rs` at repo root**: arch-explorer noted this file exists but it's not in `Cargo.toml`. Is it dead code? Should it be removed?
2. **`crawl_jobs/` dispatch criteria**: What logic in `crawl_jobs_dispatch.rs` routes between v1 and v2? Not documented anywhere.
3. **Monolith Guardrails change**: The linter changed file size limit from `400` → `500` and expanded exempt paths — was this an intentional policy change? The README now reflects these new values.

---

## Next Steps

1. Investigate `axon_main.rs` — likely dead code, should be deleted.
2. Document v2 dispatch logic in CLAUDE.md or architecture doc once understood.
3. Consider a follow-up session to add the undocumented Chrome/watchdog/cron flags to CLAUDE.md's Global Flags Reference (currently only in README).
