# Session: Monolith Policy Docs Update + cargo fmt Fix

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Duration:** Short (~20 min)

---

## Session Overview

Updated monolith policy documentation to accurately reflect the enforce script's actual thresholds (warn@80 / hard-fail@120 for functions), audited all 7 CI gates for current pass/fail status, and fixed a widespread `cargo fmt` failure across 22 files.

---

## Timeline

1. Read `docs/monolith-policy.md` and `scripts/enforce_monoliths.py` — discovered policy doc said "80 lines" but script already had `WARN=80 / MAX=120`
2. Updated `docs/monolith-policy.md` and `CLAUDE.md` to reflect the two-tier threshold
3. User asked what CI gates exist — enumerated all 7 from `ci.yml` and `lefthook.yml`
4. User asked if CI is currently passing — ran all gates locally
5. Found `cargo fmt` failing across 68 diff hunks in 22 files
6. Ran `cargo fmt --all`, verified clean

---

## Key Findings

- `enforce_monoliths.py:21-23` already had `DEFAULT_FUNCTION_WARN_LINES = 80` and `DEFAULT_FUNCTION_MAX_LINES = 120` — docs were stale, not the script
- `cargo fmt` was failing on the current branch — 68 diff hunks across 22 files, primarily `stats.rs`, `worker_process.rs`, `crawl.rs`, `evaluate.rs`
- `cargo audit` is not installed locally (CI-only via `cargo install cargo-audit`)
- `.monolith-allowlist` has 14 entries all dated 2026-02-19 — significant allowlist debt to retire
- All other gates (check, clippy, test, monolith, no-legacy-symbols) were already passing

---

## Technical Decisions

- **Two-tier threshold documented**: warn@80 / hard-fail@120. The distinction matters — functions between 80-120 lines get flagged as warnings but don't block CI, giving authors a softer nudge before the hard limit.
- **File size kept as single hard-fail at 500**: No warn tier for files, consistent with script behavior (`DEFAULT_FILE_MAX_LINES = 500`, no separate warn).
- **`cargo fmt --all` not `--staged`**: Formatted all files, not just staged — the branch had accumulated formatting debt beyond staged files.

---

## Files Modified

| File | Change |
|------|--------|
| `docs/monolith-policy.md` | Updated function limit line to "warn at 80, hard fail at 120" |
| `CLAUDE.md` | Same update + added "(hard fail)" label to 500-line file limit |
| 22 source files (batch, crawl, doctor, embed, extract, map, status, content, engine, sitemap, worker files, ops/*) | `cargo fmt --all` formatting fix — no logic changes |

---

## Commands Executed

```bash
# Audit gate status
python3 scripts/enforce_monoliths.py --staged          # ✅ pass
python3 scripts/enforce_no_legacy_symbols.py           # ✅ pass
cargo fmt --all -- --check                             # ❌ 68 hunks, 22 files
RUSTFLAGS="-D warnings" cargo check --all-targets      # ✅ pass
cargo clippy --all-targets -- -D warnings              # ✅ pass
cargo test --all                                       # ✅ pass

# Fix
cargo fmt --all                                        # applied
cargo fmt --all -- --check                             # ✅ pass (verified clean)
```

---

## Behavior Changes (Before/After)

| Item | Before | After |
|------|--------|-------|
| `docs/monolith-policy.md` function limit | "80 lines" (single threshold) | "warn at 80 lines, hard fail at 120 lines" |
| `CLAUDE.md` monolith section | "≤ 80 lines" | "warn at 80 lines, hard fail at 120 lines" |
| `cargo fmt --check` | ❌ 68 diff hunks | ✅ clean |
| CI `fmt` gate | Would fail on push | Passes |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `enforce_monoliths.py --staged` | pass | "Monolith policy check passed." | ✅ |
| `enforce_no_legacy_symbols.py` | pass | "Legacy symbol deny-check passed." | ✅ |
| `cargo fmt --all -- --check` (after fix) | pass | exit 0 | ✅ |
| `cargo check --all-targets` | pass | "Finished" | ✅ |
| `cargo clippy --all-targets -- -D warnings` | pass | "Finished" | ✅ |
| `cargo test --all` | pass | "3 passed; 0 failed" | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations this session.

---

## Risks and Rollback

- **Formatting changes are trivial to roll back**: `git checkout -- .` restores all pre-fmt state. No logic changed.
- **Doc changes**: text-only, no code impact. Rollback via `git revert` if needed.

---

## Decisions Not Taken

- **Add `unwrap()`/`expect()` to legacy symbol deny-list**: Discussed as a natural next policy addition. Not implemented — user deferred.
- **Add `std::fs` ban for async code**: Identified as a candidate (already in MEMORY.md as critical pattern). Not formalized into policy.
- **Warn tier for file size (500 lines)**: Considered adding a warn@400 / fail@500 split. Kept as single hard-fail to match script behavior.
- **Document CI gates in CLAUDE.md**: Noted as a gap (gates exist in CI but not described in dev docs). Not added this session.

---

## Open Questions

- **`.monolith-allowlist` debt**: 14 entries all from 2026-02-19, several marked "temporary CI unblock". These need refactoring tickets and retirement plan.
- **`cargo audit` local install**: Not available locally. Worth adding to dev setup docs or a `make audit` target.
- **`cargo deny check` not run locally**: Only runs in CI. Unclear if it would pass locally given current dep state.

---

## Next Steps

- [ ] Commit the fmt fix + doc updates to `perf/command-performance-fixes`
- [ ] Retire `.monolith-allowlist` entries as functions get refactored (esp. `worker_process.rs` at 342 lines)
- [ ] Consider adding `unwrap()`/`expect()` policy (could use existing `enforce_no_legacy_symbols.py` pattern)
- [ ] Document existing CI gates in CLAUDE.md dev section
