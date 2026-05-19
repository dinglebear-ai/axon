# Session: CLAUDE.md Consolidation + Search Auto-Crawl Removal

**Date:** 2026-02-24
**Branch:** `fix-crawl`

## Session Overview

Two focused tasks: (1) trimmed the global `~/.claude/CLAUDE.md` from 40.5k chars to 36k chars by consolidating redundant cross-language coding standards into a single source of truth, and (2) removed the auto-crawl behavior from `axon search` that was triggering massive unbounded site crawls from search result base URLs.

## Timeline

1. **CLAUDE.md analysis** — User flagged the 40.5k char file as impacting performance (threshold: 40k). Identified redundancy map across Python, TypeScript, and Cross-Language sections.
2. **Consolidation strategy** — Chose "universal rules once, language-specific deltas only" over wholesale deletion. User explicitly requested keeping the persona/prose section.
3. **CLAUDE.md edits** — Expanded Cross-Language Rules to 8 bullets absorbing repeated patterns. Collapsed Python (30 bullets → 9) and TypeScript (20 bullets → 7) to delta-only. Compressed Decision Trees, Prohibited Technologies, Operational Learnings. Removed duplicate Documentation Structure section.
4. **Search auto-crawl investigation** — Located `extract_crawl_seed()` in `crates/cli/commands/search.rs` — pure function stripping result URLs to origin, then batch-enqueueing crawl jobs via `start_crawl_jobs_batch()`.
5. **Auto-crawl removal** — Deleted: `extract_crawl_seed()`, `CRAWL_SKIP_HOSTS`, 11 unit tests, `--crawl-from-result` CLI flag from all 3 config files (cli.rs, types.rs, parse.rs) + Default/Debug impls. Kept 2 remaining search tests.
6. **Verification** — `cargo check` clean, `cargo test search` — 10 passed, 0 failed.

## Key Findings

- **CLAUDE.md redundancy pattern**: "early returns", "error handling", "async/await", "validate at boundaries" each appeared 3 times across Cross-Language Rules, Python Coding Standards, and TypeScript Coding Standards sections.
- **Documentation Structure** (lines 493-496) was an exact duplicate of the Monorepo Layout tree diagram above it.
- **Search auto-crawl** (`search.rs:105-143`): stripped each Tavily result URL to `scheme://host[:port]` origin, deduplicated into `HashSet`, validated via `validate_url()`, then batch-enqueued as crawl jobs. This meant a search for "rust async" could trigger full-site crawls of docs.rust-lang.org, tokio.rs, etc.
- `start_crawl_jobs_batch` has another caller in `crawl.rs:458` — not dead code after search removal.

## Technical Decisions

- **Consolidation over deletion**: Cross-Language Rules became the single source for universal patterns. Language sections only contain genuinely unique rules (e.g., `match`/`case`, `satisfies`, `using` keyword). This preserves all information while eliminating ~4.5k chars of repetition.
- **Complete auto-crawl removal** (not opt-in flag): The feature was causing real operational pain (massive crawls). If the user wants to crawl a search result, `axon crawl <url>` is the explicit path. YAGNI on an opt-in flag.
- **Full `crawl_from_result` cleanup**: Removed from Config struct, CLI args, parse logic, Default impl, and Debug impl rather than leaving dead fields.

## Files Modified

| File | Purpose |
|------|---------|
| `~/.claude/CLAUDE.md` | Consolidated cross-language rules, compressed Python/TS to deltas, removed duplicates |
| `crates/cli/commands/search.rs` | Removed `extract_crawl_seed()`, `CRAWL_SKIP_HOSTS`, auto-crawl enqueue block, 11 tests |
| `crates/core/config/cli.rs` | Removed `--crawl-from-result` clap arg |
| `crates/core/config/types.rs` | Removed `crawl_from_result` field from Config struct, Default, Debug |
| `crates/core/config/parse.rs` | Removed `crawl_from_result` mapping |

## Commands Executed

| Command | Result |
|---------|--------|
| `wc -c ~/.claude/CLAUDE.md` | 36,036 chars (down from 40,508) |
| `wc -l ~/.claude/CLAUDE.md` | 572 lines (down from 669) |
| `cargo check` | Clean — 0 errors, 0 warnings |
| `cargo test search` | 10 passed, 0 failed |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon search "query"` | Displayed results, then auto-enqueued crawl jobs for each unique result origin | Displays results only — no crawl side effects |
| `--crawl-from-result` flag | Accepted; controlled whether seeds were origin-stripped or exact URLs | Removed; unrecognized flag error if used |
| CLAUDE.md context load | 40.5k chars — triggered performance warning | 36k chars — under 40k threshold |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | `Finished dev profile in 0.40s` | PASS |
| `cargo test search` | All search tests pass | 10 passed, 0 failed | PASS |
| `grep crawl_from_result` | No matches | No matches found | PASS |
| `grep extract_crawl_seed` | No matches | No matches found | PASS |
| `wc -c CLAUDE.md` | <40,000 | 36,036 | PASS |

## Risks and Rollback

- **CLAUDE.md**: All changes are consolidation (no information deleted). Rollback via `git checkout ~/.claude/CLAUDE.md` if any instruction was inadvertently lost.
- **Search auto-crawl**: Low risk — removes a side effect. Any existing queued crawl jobs from past searches will complete normally. Rollback: `git checkout fix-crawl -- crates/cli/commands/search.rs crates/core/config/cli.rs crates/core/config/types.rs crates/core/config/parse.rs`

## Decisions Not Taken

- **Opt-in `--auto-crawl` flag**: Considered adding a flag to re-enable the behavior. Rejected — if the user wants to crawl, `axon crawl <url>` is explicit and controlled. No need for a search command to have crawl side effects.
- **Replace crawl with scrape**: Considered auto-scraping individual result URLs (not origins) to still feed the knowledge base. Rejected — user said "disable", and scraping can be done explicitly.
- **Persona prose removal**: Initially suggested cutting the 8 narrative paragraphs. User explicitly requested keeping them.
- **Moving Python/TS standards to external files**: Would have saved more chars but fragments the instructions. Consolidation-in-place was the better tradeoff.

## Open Questions

- Should `axon search` auto-scrape (not crawl) individual result pages to maintain "Always Be Indexing"? Currently it does neither.
- The `search_limit` config field controls Tavily result count — should there be a default cap documented?

## Next Steps

- None required — both changes are complete and verified.
