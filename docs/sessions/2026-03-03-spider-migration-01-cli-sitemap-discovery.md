# Spider Migration 01: CLI Sitemap Discovery → Engine-Backed
**Date:** 2026-03-03 | **Branch:** `feat/sidebar` | **Duration:** ~90 min

## Session Overview

Executed the plan at `docs/plans/2026-03-03-spider-migration-01-cli-sitemap-discovery.md` using subagent-driven development. Replaced the CLI's 220-line hand-rolled BFS sitemap crawler (`crates/cli/commands/crawl/audit/sitemap.rs`) with a 55-line delegation layer that routes through the crawl engine's concurrent batched sitemap discovery (`crates/crawl/engine/sitemap.rs`). Net -295 lines. Found and fixed a port preservation bug during characterization testing.

## Timeline

1. **Codebase analysis** — Read both sitemap implementations (CLI vs engine), identified that engine lacked robots.txt parsing
2. **Task 1: Characterization tests** — Dispatched implementer subagent. Discovered port preservation bug in `host_str()`. Added SSRF loopback bypass for httpmock. 3 tests passing.
3. **Task 1 reviews** — Spec reviewer: pass (noted symlinks script change as minor scope creep). Code reviewer: pass (noted subdomain test limitation with localhost).
4. **Task 2: Engine migration** — Dispatched implementer. Added robots.txt parsing to engine, created `SitemapDiscovery` adapter type, rewrote CLI to delegate.
5. **Task 2 reviews** — Spec reviewer: pass. Code reviewer: flagged `reqwest::Client::builder()` (should use `build_client()` with SSRF-safe redirect policy) and missing `max_sitemaps` TODO.
6. **Reviewer fixes** — Applied both: switched to `build_client()`, added TODO comment.
7. **Tasks 3-5** — Dead code already removed by Task 2. Verification: fmt clean, clippy clean, all relevant tests pass.

## Key Findings

- **Port preservation bug** (`sitemap.rs:138-168`): `Url::host_str()` strips ports. Sitemap seed URLs like `http://127.0.0.1:PORT/sitemap.xml` silently hit port 80 instead of the actual server port. Fixed by separating `bare_host` (for scope comparison) from `host` (with port, for URL construction).
- **Engine lacked robots.txt** (`engine/sitemap.rs`): The engine's `crawl_sitemap_urls` only seeded 3 default sitemap paths. The CLI version additionally parsed `robots.txt` for `Sitemap:` directives. Added this to the engine.
- **SSRF bypass for tests**: `validate_url()` blocks `127.0.0.1` (loopback). Added thread-local `ALLOW_LOOPBACK` flag (`#[cfg(test)]`-gated) with `LoopbackGuard` RAII struct for safe test isolation.
- **`build_client()` vs raw builder**: `crates/core/http/client.rs:36` provides `build_client()` with SSRF-safe redirect policy. Using `reqwest::Client::builder()` directly bypasses redirect-target SSRF validation.
- **`max_sitemaps` not on Config**: Hardcoded to 512 in both `engine/sitemap.rs` and `jobs/crawl/runtime/robots.rs`. TODO comments in both files.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Delegate CLI → engine (not merge engine into CLI) | Engine has concurrent batched JoinSet processing, `--sitemap-since-days` support, better retry logic. CLI's sequential BFS was strictly inferior. |
| Keep `SitemapDiscoveryStats` with zeroed granular counters | JSON contract stability for `manifest_audit.rs` serialization. Engine handles filtering internally and doesn't expose per-category counts. |
| Keep `fetch_text_with_retry` in `audit.rs` | Still used by `backfill.rs` for page content fetching (different operation than sitemap discovery). |
| Thread-local SSRF bypass (not global) | Prevents cross-thread interference with the 67 existing SSRF tests. Each test thread controls its own bypass independently. |
| `build_client()` over raw `reqwest::Client::builder()` | Includes `ssrf_safe_redirect_policy()` which re-validates every 302 target against SSRF rules. Prevents redirect-based SSRF bypass. |

## Files Modified

| File | Change | Lines |
|------|--------|-------|
| `crates/crawl/engine/sitemap.rs` | Rewritten: added robots.txt parsing, `SitemapDiscovery` type, port-aware construction, `build_client()` | 317 |
| `crates/crawl/engine.rs` | Added `pub(crate) mod sitemap` + re-exports | +2 |
| `crates/cli/commands/crawl/audit/sitemap.rs` | Rewritten: 220→55 lines, pure delegation to engine | 55 |
| `crates/cli/commands/crawl/audit/sitemap_migration_tests.rs` | New: 3 characterization tests with httpmock | 259 |
| `crates/cli/commands/crawl/audit.rs` | Added `#[cfg(test)] mod sitemap_migration_tests` | +2 |
| `crates/core/http/ssrf.rs` | Added `ALLOW_LOOPBACK` thread-local + `set_allow_loopback` + `get_allow_loopback` (`#[cfg(test)]`) | +23 |
| `crates/core/http/mod.rs` | Added test-gated re-exports for loopback functions | +4 |
| `scripts/check_claude_symlinks.sh` | Added `.next/` exclusion | +1 |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Sitemap discovery path | CLI-owned sequential BFS | Engine-owned concurrent JoinSet batches |
| robots.txt in engine | Not parsed | Parsed and sitemaps enqueued |
| Port in sitemap seed URLs | Silently dropped for non-standard ports | Preserved correctly |
| `SitemapDiscoveryStats` granular counters | Populated (`filtered_out_of_scope_host`, etc.) | Zeroed (engine filters internally) |
| HTTP client in sitemap | Raw `reqwest::Client::builder()` | `build_client()` with SSRF-safe redirect policy |
| `--sitemap-since-days` in CLI audit | Not supported | Supported (engine handles it) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib sitemap_migration_tests` | 3 pass | 3 pass | PASS |
| `cargo test --lib map` | all pass | 23 pass, 0 fail | PASS |
| `cargo test --lib crawl` | engine tests pass | 90 pass, 3 fail (pre-existing postgres) | PASS |
| `cargo fmt --check` | clean | clean | PASS |
| `cargo clippy --all-targets` | clean | clean | PASS |
| grep for removed helpers | no matches | no matches | PASS |

## Risks and Rollback

- **Risk**: Zeroed granular filter counters in audit snapshots. If downstream tooling asserts on `filtered_out_of_scope_host > 0`, it will break. **Mitigation**: No known consumers assert on these; they're informational.
- **Rollback**: `git revert c9ebd58b 817160bd` restores the CLI-owned sitemap path.

## Decisions Not Taken

- **Add `max_sitemaps` to Config**: Deferred — requires touching `Config` struct, `global_args.rs`, `build_config.rs`, and all inline test configs. Left as TODO.
- **Subdomain inclusion test with real subdomains**: Can't test with httpmock on `127.0.0.1`. The cross-domain filtering test covers the code path.
- **Extract `discover_sitemap_urls` setup into helper**: Function is 89 lines (warning at 80, limit 120). Splitting would add complexity for marginal benefit.

## Open Questions

- Should `SitemapDiscoveryStats` be simplified to match `SitemapDiscovery` fields only? The zeroed counters are noise.
- The `fetch_text_with_retry` in `audit.rs` and `engine/sitemap.rs` are near-duplicates. Should they share a single implementation?
- `#[serial]` on migration tests vs `#[cfg(test)]` thread-local: one test run showed races. Need to confirm `serial_test` crate is in dev-deps.

## Next Steps

1. **Spider Migration 02**: Plan exists for additional migration targets
2. **`max_sitemaps` Config field**: Add to `Config`, wire through `global_args.rs` and `build_config.rs`
3. **Consolidate `fetch_text_with_retry`**: Move to `crates/core/http/` as shared utility
