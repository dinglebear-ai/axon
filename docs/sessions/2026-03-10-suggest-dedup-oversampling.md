# Session: Axon Suggest — Already-Indexed URL Deduplication Fix
**Date:** 2026-03-10 | **Duration:** ~20 min
**Working Directory:** `/home/jmagar/workspace/axon_rust`

---

## Session Overview

Fixed a gap in `axon suggest` where the command could return fewer results than requested, and could present URLs from already-crawled domains. The core issue was that the LLM was asked for exactly `desired` suggestions but could only see a 500-URL sample of up to 50,000 indexed URLs — causing post-filter rejections to silently reduce the output count.

---

## Timeline

1. Investigated `crates/vector/ops/commands/suggest.rs` and `crates/cli/commands/suggest.rs`
2. Traced the full pipeline: `build_suggest_prompt_context` → `build_suggest_user_prompt` → `request_suggestions_from_llm` → `filter_new_suggestions`
3. Identified two root causes (see Key Findings)
4. Implemented fix: `llm_request` over-sampling field + improved prompt rules
5. Verified no suggest-related compile errors

---

## Key Findings

- **`suggest.rs:157-161`**: `AXON_SUGGEST_EXISTING_URL_LIMIT` defaults to 500; `AXON_SUGGEST_INDEX_LIMIT` defaults to 50,000. The LLM only sees 500 of potentially 50,000 indexed URLs.
- **`suggest.rs:207-223`** (before fix): Prompt asked for exactly `desired` suggestions — post-filter rejection of already-indexed URLs silently reduced output below `desired`.
- **`filter_new_suggestions`** was already correct — it rejects indexed URLs and trims to `desired`. The gap was upstream at the LLM request count.
- `already_indexed()` + `url_lookup_candidates()` correctly check both trailing-slash variants. The dedup logic itself was sound.
- Pre-existing compile errors in `crates/web/tailscale_auth.rs` (unrelated `AuthOutcome` match arms) prevented running unit tests — confirmed pre-existing, not caused by this session.

---

## Technical Decisions

**Over-sampling (3×)**: Request `desired * 3` (capped at 100) from the LLM. Post-filter rejects already-indexed ones; `filter_new_suggestions` trims to `desired`. This ensures the output count reaches `desired` even when many LLM suggestions get rejected.

**Cap at 100**: LLM context budget. Requesting 300 suggestions for `--limit 100` would be wasteful; the cap keeps it reasonable.

**Prompt improvements**: Added two new rules:
1. Avoid well-covered domains (LLM can see page counts in `INDEXED_BASE_URLS_WITH_PAGE_COUNTS`)
2. Prefer new domains/paths not in the indexed sample
Also relabeled `ALREADY_INDEXED_URLS` as `(sample — more may be indexed)` to set correct LLM expectations.

**Rejected alternative — larger URL sample**: Sending more than 500 indexed URLs in the prompt costs tokens proportionally and LLMs don't use long URL lists well. Over-sampling is cheaper and more reliable.

**Rejected alternative — domain-level blocking**: Blocking entire domains that are "well-covered" would be too aggressive and could miss legitimate gaps (e.g., new API versions on a heavily-indexed domain).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/commands/suggest.rs` | Added `llm_request` field to `SuggestPromptContext`; compute as `(desired * 3).min(100)`; use in prompt instead of `desired`; added two new prompt rules; relabeled indexed URL list |

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| `axon suggest 10` with 8 of 10 LLM suggestions already indexed | Returns 2 results | Asks for 30, filters, returns up to 10 novel results |
| LLM context for already-indexed content | Only 500 URLs shown; no domain-level guidance | 500 URLs shown + explicit prompt rules about well-covered domains |
| `ALREADY_INDEXED_URLS` label in prompt | `ALREADY_INDEXED_URLS:` | `ALREADY_INDEXED_URLS (sample — more may be indexed):` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon 2>&1 \| grep -i suggest` | No suggest errors | No output (clean) | ✅ Pass |
| `git diff crates/vector/ops/commands/suggest.rs` | Shows `llm_request` field + prompt changes | Confirmed 3 hunks as expected | ✅ Pass |
| Unit tests (suggest module) | 4 tests pass | Could not run — pre-existing `tailscale_auth.rs` compile errors block test compilation | ⚠️ Blocked (pre-existing) |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session (code change only).

---

## Risks and Rollback

**Risk**: For `desired = 34` (edge case), `llm_request = 100` (cap triggers), so the user gets at most 34 from 100 suggestions. This is fine — the cap just prevents absurd LLM requests for small `desired` values.

**Rollback**: Revert `crates/vector/ops/commands/suggest.rs` to remove `llm_request` field; restore `ctx.desired` in prompt format string.

---

## Decisions Not Taken

- **Larger indexed URL sample in prompt**: Rejected — token cost scales linearly with URL count; LLMs don't reliably use long lists anyway. Over-sampling is more effective per token.
- **Domain-level hard block**: Rejected — too aggressive; would suppress valid suggestions on domains with coverage gaps.
- **Fragment/query normalization in `url_lookup_candidates`**: Checked — `normalize_url` doesn't strip `#fragments` or `?params`, but spider already strips fragments before storing. Not a real-world issue.

---

## Open Questions

- Pre-existing `tailscale_auth.rs` compile errors (`AuthOutcome::DualAuth` / `SshKey` match arms) need resolution before the test suite can run. Unrelated to this change.
- Is 3× the right oversample factor? Could be tunable via `AXON_SUGGEST_OVERSAMPLE` env var if users hit edge cases.

---

## Next Steps

- Fix pre-existing `tailscale_auth.rs` match exhaustiveness errors to unblock test suite
- Consider adding `AXON_SUGGEST_OVERSAMPLE` env var (default 3, range 1–10) for tunability
- Consider adding a test case: `filter_new_suggestions` returns `desired` count even when LLM output has many indexed URLs
