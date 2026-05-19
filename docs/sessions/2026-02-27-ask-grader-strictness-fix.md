# Session: Fix Axon `ask` Grader Strictness
**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack
**Duration:** Single session

---

## Session Overview

Fixed two systematic gates in `normalize_ask_answer()` that were rejecting valid `platejs.org` answers
with "Insufficient evidence" despite high retrieval scores (0.69–0.94) and 709 embedded chunks.

Root causes:
1. **Gate 5 (Procedural)**: `is_official_docs_source()` only recognized `docs.*` / `developers.*` hostnames — `platejs.org/docs/*` failed silently.
2. **Gate 3 (min citations)**: Default of 2 unique citations rejected focused single-source answers.

Two-layer fix: immediate env-var tuning (day-one relief) + targeted code improvements (robust for any site).

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read plan from pre-existing plan file |
| Step 1 | Read `ask.rs` (726 lines) and `types.rs` (799 lines) to understand gate structure |
| Step 2 | Read `parse.rs` and `performance.rs` for env-var wiring patterns |
| Step 3 | Checked `.monolith-allowlist` — `ask.rs` already allowlisted |
| Step 4 | Implemented all 6 file changes + 5 new tests |
| Step 5 | Fixed test bug (`strict_config_schema_false_bypasses_gate6` hit Gate 3 before Gate 6) |
| Step 6 | Verified: 447 lib tests passing, 0 clippy warnings |

---

## Key Findings

- **Gate pipeline order** (`ask.rs:417–508`): Gates execute in sequence — Gate 3 (min citations) runs before Gates 5/6, so bypassing Gate 6 in a test also requires setting `ask_min_citations_nontrivial = 1`.
- **`url_path_is_docs_like()` coverage**: `/docs/`, `/documentation/`, `/guide/`, `/reference/`, `/api/` path prefixes now recognized as official docs without hostname convention requirements.
- **`test_config()` in `jobs/common/mod.rs:88`** uses `..Config::default()` — no update required when adding new fields with proper defaults.
- **`env_bool()` pattern**: Accepts `"true"`, `"1"`, `"yes"` (case-insensitive); anything else (including empty string) → default. Added to `performance.rs` for reuse.
- **Monolith check**: `ask.rs` at line ~800 is well within the existing allowlist entry (was 726 before, now ~820).

---

## Technical Decisions

### Why path-prefix heuristic instead of just AUTHORITATIVE_DOMAINS?
- Avoids requiring manual per-site registration for every docs site that doesn't use `docs.` hostnames.
- Catches `platejs.org/docs/*`, `vuejs.org/guide/*`, `react.dev/reference/*` automatically.
- `AXON_ASK_AUTHORITATIVE_DOMAINS` still works as an explicit override for edge cases.

### Why boolean flags instead of removing the gates?
- Preserves the quality gates for sources that genuinely lack docs (e.g., `medium.com` blog posts).
- Gives operators an escape hatch without code changes (`AXON_ASK_STRICT_PROCEDURAL=false`).
- Default `true` means existing behavior is unchanged unless explicitly opted out.

### Why lower `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` to 1 in `.env`?
- platejs.org queries are focused (one library) — single authoritative source is valid.
- The default of 2 remains in code; `.env` overrides for this deployment.
- Can be reverted without code changes: remove the line from `.env`.

### Why `api/` in `url_path_is_docs_like()`?
- API reference pages (`example.com/api/client`) are documentation.
- Risk of false positives (REST endpoints) accepted per plan specification.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/core/config/parse/performance.rs` | Added `env_bool(key, default)` helper |
| `crates/core/config/types.rs` | Added `ask_strict_procedural: bool` + `ask_strict_config_schema: bool` fields, defaults, Debug impl |
| `crates/core/config/parse.rs` | Wired `AXON_ASK_STRICT_PROCEDURAL` + `AXON_ASK_STRICT_CONFIG_SCHEMA` env vars |
| `crates/vector/ops/commands/ask.rs` | Added `url_path_is_docs_like()`, updated `is_official_docs_source()`, guarded gates 5+6, added 5 tests |
| `.env` | Added `AXON_ASK_AUTHORITATIVE_DOMAINS=platejs.org` + `AXON_ASK_MIN_CITATIONS_NONTRIVIAL=1` |
| `.env.example` | Documented new ask quality tuning env vars section |

---

## Commands Executed

```bash
# Type check — clean
cargo check
# → Finished `dev` profile in 0.43s

# Ask-specific tests — 31 passing
cargo test ask
# → test result: ok. 31 passed; 0 failed

# Full lib test suite
cargo test --lib
# → test result: ok. 447 passed; 0 failed; 3 ignored

# Clippy
cargo clippy
# → 0 warnings
```

---

## Behavior Changes (Before/After)

### Before
- `axon ask "how do I add bold formatting to a PlateJS editor?"` → `"Insufficient evidence"` even with 709 embedded platejs.org chunks at scores 0.69–0.94.
- Gate 5 rejected `platejs.org/docs/*` because hostname doesn't start with `docs.` or `developers.`.
- Gate 3 rejected single-citation answers even for focused queries.

### After
- `platejs.org/docs/*` URLs now recognized as official docs via path-prefix heuristic.
- `AXON_ASK_MIN_CITATIONS_NONTRIVIAL=1` in `.env` allows single-citation answers.
- New escape hatches: `AXON_ASK_STRICT_PROCEDURAL=false` / `AXON_ASK_STRICT_CONFIG_SCHEMA=false` bypass gates entirely.
- `blog.example.net`, `medium.com` still correctly rejected by Gate 5 (no regression).

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors | Finished in 0.43s | ✅ |
| `cargo test ask` | 31 pass, 0 fail | 31 pass, 0 fail | ✅ |
| `cargo test --lib` | All pass | 447 pass, 0 fail | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| `url_path_is_docs_like("https://platejs.org/docs/plugin/bold")` | `true` | `true` (test) | ✅ |
| `is_official_docs_source("https://platejs.org/docs/getting-started", cfg)` | `true` | `true` (test) | ✅ |
| `normalize_ask_answer` with platejs.org/docs source | Answer (not insufficient) | Passes | ✅ |

---

## Source IDs + Collections Touched

None — this was a pure code change session. No Qdrant embed/retrieve operations.

---

## Risks and Rollback

### Risk: `api/` path prefix too broad
- **Scenario**: A URL like `https://example.com/api/v1/users` treated as official docs when it's a REST endpoint.
- **Mitigation**: Only affects Gate 5 (Procedural queries asking "how do I…"). REST endpoint URLs rarely appear in RAG retrieval for doc queries.
- **Rollback**: Remove `|| path.starts_with("api/")` from `url_path_is_docs_like()`.

### Rollback (full)
```bash
# Revert code changes
git diff HEAD -- crates/vector/ops/commands/ask.rs crates/core/config/ | git apply --reverse
# Revert env
# Remove AXON_ASK_AUTHORITATIVE_DOMAINS and AXON_ASK_MIN_CITATIONS_NONTRIVIAL from .env
```

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Remove Gate 5 entirely | Would let blog posts / unrelated sources pass for procedural queries |
| Add `platejs.org` to hardcoded hostname list | Would require code change for every new docs site; env var is better |
| `*.org/docs/*` wildcard matching | Too broad; path-prefix on any host is specific enough |
| Lower default `ask_min_citations_nontrivial` in code to 1 | Would affect all deployments; safer to leave default=2 and override in `.env` |

---

## Open Questions

- Smoke test against live service not yet run (requires `OPENAI_BASE_URL` and 709 platejs.org chunks in Qdrant). The regression tests use fixtures and confirm the gate logic is correct.
- `api/` path prefix in `url_path_is_docs_like()` — monitor for false positives in production queries.

---

## Next Steps

1. Rebuild Docker workers (`just rebuild` or `docker compose build axon-workers`) to pick up the binary change.
2. Run smoke test: `./scripts/axon ask "how do I add bold formatting to a PlateJS editor?"`
3. Verify citations: `./scripts/axon ask "how do I configure the link plugin in platejs?" --json | jq '.answer'`
4. If `api/` causes false positives, remove it from `url_path_is_docs_like()`.
