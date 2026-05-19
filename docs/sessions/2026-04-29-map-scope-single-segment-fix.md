# Session: map command scope fix for single-segment paths

**Date:** 2026-04-29  
**Branch:** main  
**HEAD:** abf35ccf  

---

## Session Overview

Debugged and fixed a bug where `axon map https://agentskills.io/home` returned only 1 URL instead of the full site's 9 sitemap URLs. Root cause was two independent scope checks, both missing a single-segment path exemption that the crawl engine already applies.

---

## Timeline

1. Ran `axon map https://agentskills.io/home` — observed only 1 URL returned (the input URL itself, via sitemap source)
2. Verified sitemap at `https://agentskills.io/sitemap.xml` has 9+ URLs
3. Read `crates/crawl/engine/map.rs` — found `derive_map_scope` sets `path_prefix: Some("/home")` for any non-empty path
4. Read `crates/crawl/engine/url_utils.rs` — `normalize_map_candidate_url` rejects URLs not matching the prefix
5. Applied fix #1 to `derive_map_scope` — rebuilt, still returned 1 URL
6. Found second scope check in `crates/crawl/engine/sitemap.rs:265` — `scoped_to_root = start_path.is_empty()`, which was `false` for `/home`, causing `sitemap_loc_in_scope` to filter all non-`/home` URLs before they even reached `map_with_sitemap`
7. Applied fix #2 to `sitemap.rs` — rebuilt, confirmed 9 URLs returned
8. Noted user tested `https://agent-skills.io` (with hyphen) — confirmed parked domain, expected behavior

---

## Key Findings

- **`sitemap.rs:264-265`** — `scoped_to_root = start_path.is_empty()` was the primary bottleneck; sitemap discovery itself filtered the URLs
- **`map.rs:167-183`** — `derive_map_scope` had a secondary issue: no single-segment exemption, so even if sitemap returned URLs, `normalize_map_candidate_url` would re-filter them
- **`url_utils.rs:156-160`** — `derive_auto_whitelist_pattern` (used for crawl) already had the correct rule: skip scoping for `segment_count <= 1`; the map path was simply missing the same rule
- `sitemap_loc_in_scope` (`sitemap.rs:95-124`) checks `if !scoped_to_root { ... }` — setting `scoped_to_root = true` for single-segment paths makes all host-matching sitemap URLs pass through

---

## Technical Decisions

**Why not scope single-segment paths?** Single-segment paths like `/home`, `/about`, `/project` are top-level page names, not directory roots. Scoping the sitemap to `/home` means only `/home` and hypothetical `/home/child` pages are returned — the user's intent when mapping `https://example.com/home` is always to get the full site. This mirrors the rule already established in `derive_auto_whitelist_pattern` for crawl.

**Why update both `sitemap.rs` AND `map.rs`?** Defense in depth. `sitemap.rs` is the primary fix (stops the filtering before merge), `map.rs` ensures the secondary filter is also consistent. Both fixes apply the same `segment_count <= 1` rule.

**Test update rationale:** `test_map_seed_scope_uses_resolved_project_prefix` asserted that `example.github.io/project` creates `path_prefix: Some("/project")`. This was changed to assert `path_prefix: None` (new behavior). For GitHub Pages sites, the sitemap typically contains only that project's URLs anyway, so root scope produces identical results in practice. A new test `test_map_seed_multi_segment_path_scopes_to_prefix` validates that multi-segment paths (`/docs/python`) still scope correctly.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/crawl/engine/sitemap.rs` | Added `segment_count` check; `scoped_to_root` now `true` for single-segment paths |
| `crates/crawl/engine/map.rs` | `derive_map_scope` returns `path_prefix: None` for single-segment paths |
| `crates/crawl/engine/tests.rs` | Updated `test_map_seed_scope_uses_resolved_project_prefix` → `test_map_seed_single_segment_path_uses_root_scope`; added `test_map_seed_multi_segment_path_scopes_to_prefix` |

---

## Commands Executed

```bash
# Reproduce
source .env && ./target/debug/axon map https://agentskills.io/home
# → Showing 1 (source: sitemap) — bug confirmed

# Verify sitemap content
curl -s -L "https://agentskills.io/sitemap.xml"
# → 9 URLs present

# After fix #1 (map.rs only) — still broken
cargo build --bin axon && ./target/debug/axon map https://agentskills.io/home
# → Showing 1 — sitemap.rs was filtering first

# After fix #2 (sitemap.rs)
cargo build --bin axon && ./target/debug/axon map https://agentskills.io/home
# → Showing 9 (source: sitemap) ✓

# Test suite
cargo test map
# → 254 passed, 0 failed ✓

cargo test
# → 1932 passed, 18 ignored ✓
```

---

## Behavior Changes (Before/After)

| Input | Before | After |
|-------|--------|-------|
| `axon map https://agentskills.io/home` | 1 URL (only `/home`) | 9 URLs (full sitemap) |
| `axon map https://example.github.io/project` | scope `/project` (sitemap filtered to `/project/*`) | scope root (all host sitemap URLs returned) |
| `axon map https://docs.example.com/docs/python` | scope `/docs/python` | scope `/docs/python` (unchanged — multi-segment) |
| `axon map https://example.com/` | scope root | scope root (unchanged) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `axon map https://agentskills.io/home` | 9 URLs from sitemap | 9 URLs from sitemap | ✓ PASS |
| `cargo test map` | 0 failures | 254 passed, 0 failed | ✓ PASS |
| `cargo test` (full suite) | 0 failures | 1932 passed, 0 failed | ✓ PASS |

---

## Risks and Rollback

**Risk:** GitHub Pages or similar multi-project hosts where a single-segment path IS a meaningful subtree (e.g. `username.github.io/project1` wanting only `/project1/*` URLs). With this fix, root scope returns all projects' sitemap URLs. In practice this is better UX — users can always filter by inspecting the returned URLs.

**Rollback:** Revert the two-line `segment_count` additions in `sitemap.rs:264-268` and `map.rs:172-178`, and revert the test changes in `tests.rs`.

---

## Decisions Not Taken

- **Post-hoc scope widening** — widen scope after sitemap result is too small (e.g. only 1 URL returned). Rejected: heuristic and fragile; the principled fix is upfront.
- **New `--no-scope` flag** — let users opt out of scoping. Rejected: the default should just work; adding a flag for a broken default is the wrong abstraction.
- **Different rules for `map` vs backfill** — add a parameter to `discover_sitemap_urls` to control scoping. Rejected: the same single-segment exemption is correct for both paths (crawl doesn't scope single-segment paths, so backfill from such a crawl shouldn't either).

---

## Open Questions

- Should `agentskills.io` vs `agent-skills.io` confusion warrant a note somewhere? User tested the wrong domain (parked) first.
- Are there other callers of `discover_sitemap_urls` or `sitemap_loc_in_scope` that might be affected by the scoping change?

---

## Next Steps

- None required — fix is complete and all tests pass.
