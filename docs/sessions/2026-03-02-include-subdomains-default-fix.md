# Session: Fix `include_subdomains` Default — Single-Page Crawl Root Cause

**Date:** 2026-03-02
**Branch:** `feat/sidebar`

---

## Session Overview

Systematically debugged why `axon crawl https://code.claude.com/` and `axon crawl https://code.claude.com/docs` each reported only 1 page discovered. Traced the root cause to spider.rs's subdomain scoping behavior triggered by `include_subdomains: true` (the previous default). Fixed by changing the default to `false` in two places, rebuilt, and confirmed one of the two crawls now discovers the expected doc pages.

---

## Timeline

1. **Reproduced the symptom** — both crawl status lines showed `1/1 pages | 0 filtered | 0.0% thin`.
2. **Ruled out content issues** — `code.claude.com/docs` returns HTTP 200 with 30+ real `<a href="/docs/en/*">` anchor tags visible in raw HTML; `wget --spider -r -l 1` confirmed the links are real and reachable.
3. **Ran `axon map https://code.claude.com/docs`** — returned 30 URLs all from `support.claude.com`, `website.claude.com`, `www.claude.com`. Not a single `code.claude.com/docs/en/*` URL.
4. **Traced spider scoping** — `configure_website()` passes `with_subdomains(cfg.include_subdomains)` to spider. Spider's `extract_root_domain("code.claude.com")` strips the subdomain and returns `claude`, making ALL `*.claude.com` subdomains in-scope.
5. **Confirmed root cause** — spider crawls `www.claude.com`, `support.claude.com`, etc. first; they don't link back to `code.claude.com/docs/en/*`, so the crawl terminates with 1 effective page.
6. **Identified secondary issue** — `code.claude.com/` 302-redirects to `www.claude.com/product/claude-code`, a marketing page with no docs links. This URL will always yield 1 page regardless of the subdomain fix (the root simply doesn't host docs content).
7. **Implemented fix** — changed `include_subdomains` default from `true` to `false` in `Config::default()` and the clap `default_value_t`.
8. **Verified** — `cargo check` clean, all relevant test suites pass (config, engine, excludes, http). Rebuild confirmed `code.claude.com/docs` now discovers its docs pages.

---

## Key Findings

- **Spider scoping bug**: `with_subdomains(true)` + `code.claude.com` → spider computes base domain `claude` (not `claude.com`) → all `*.claude.com` subdomains become in-scope → crawl wanders to marketing/support sites instead of following the docs links. Found in `crates/crawl/engine/runtime.rs:139`.
- **`axon map` as diagnostic**: `axon map https://code.claude.com/docs` returning `support.claude.com` / `website.claude.com` URLs was the definitive proof that spider was escaping to the wrong hosts.
- **`code.claude.com/` root is unfixable**: 302 redirect to `www.claude.com/product/claude-code` is a website-side behavior. The correct crawl target for Claude Code docs is `code.claude.com/docs`, not the root.
- **Pre-existing test failure**: `crates::jobs::refresh::tests::claim_due_refresh_schedules_prevents_immediate_duplicate_claims` fails with Postgres duplicate-key error on `pg_type` — pre-existing test environment issue, unrelated to this change.
- **Default exclude list**: `default_exclude_prefixes()` does NOT exclude `/en` — the `/docs/en/*` paths were never being filtered by that mechanism. The subdomain scoping was the only blocker.

---

## Technical Decisions

- **Default `false` instead of opt-in documentation note**: The previous behavior was documented as a gotcha ("may crawl more than expected") but the default was never corrected. Changing the default is the right fix — virtually every single-host crawl wants `include_subdomains: false`.
- **`--include-subdomains true` for explicit opt-in**: Users who genuinely want cross-subdomain crawling (e.g. docs spread across `docs.example.com` and `api.example.com`) can pass the flag explicitly.
- **No change to sitemap backfill scoping**: `append_sitemap_backfill()` in `crawl/engine/sitemap.rs:85` already respects `cfg.include_subdomains` for its own host filtering — no separate change needed.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/core/config/types/config_impls.rs:20` | `include_subdomains: true` → `false` in `Config::default()` |
| `crates/core/config/cli/global_args.rs:16-17` | `default_value_t = true` → `false`; updated doc comment |
| `CLAUDE.md:104` | Updated flags table default and description for `--include-subdomains` |
| `crates/core/CLAUDE.md:53` | Updated Config struct table: `(default true)` → `(default false)` |
| `crates/crawl/CLAUDE.md:84` | Updated troubleshooting row to reflect new default |

---

## Commands Executed

```bash
# Diagnostic — confirmed redirect
curl -s -o /dev/null -w "%{http_code} %{redirect_url}" https://code.claude.com/
# 302 https://www.claude.com/product/claude-code

# Diagnostic — confirmed docs page has real links
curl -s https://code.claude.com/docs | grep -o 'href="/docs/en/[^"]*"' | head -10

# Diagnostic — key finding: spider returning wrong-subdomain URLs
axon map https://code.claude.com/docs
# Returned 30 URLs from support.claude.com, website.claude.com, www.claude.com
# ZERO URLs from code.claude.com/docs/en/*

# Verify build
cargo check --bin axon     # clean
cargo build --bin axon     # clean

# Test suites
cargo test --lib config    # 54 passed, 0 failed
cargo test --lib engine    # 42 passed, 0 failed
cargo test --lib excludes  # 8 passed, 0 failed
cargo test --lib http      # 53 passed, 0 failed
```

---

## Behavior Changes (Before / After)

| Scenario | Before (default `true`) | After (default `false`) |
|----------|------------------------|------------------------|
| `axon crawl https://code.claude.com/docs` | 1 page — spider escapes to `www.claude.com`, `support.claude.com` | Discovers `code.claude.com/docs/en/*` pages correctly |
| `axon crawl https://docs.example.com` | Silently crawls all `*.example.com` subdomains | Stays on `docs.example.com` only |
| `axon crawl https://code.claude.com/` | 1 page (redirect to www, then no links back) | 1 page (same — root redirects to different host; unfixable) |
| Explicit subdomain crawl | `--include-subdomains true` (redundant) | `--include-subdomains true` (required, now explicit) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Clean compile | `Finished dev profile` | ✓ PASS |
| `cargo build --bin axon` | Clean build | `Finished dev profile` | ✓ PASS |
| `cargo test --lib config` | 0 failures | 54 passed, 0 failed | ✓ PASS |
| `cargo test --lib engine` | 0 failures | 42 passed, 0 failed | ✓ PASS |
| `cargo test --lib excludes` | 0 failures | 8 passed, 0 failed | ✓ PASS |
| `cargo test --lib http` | 0 failures | 53 passed, 0 failed | ✓ PASS |
| `axon crawl https://code.claude.com/docs` (post-rebuild) | >1 pages | User confirmed working | ✓ PASS |

---

## Source IDs + Collections Touched

None — no embed/retrieve operations in this session. This session was a code fix, not a crawl/index session.

---

## Risks and Rollback

**Risk**: Any existing automation that relied on the old default subdomain-inclusive behavior will now stay on the exact host. Impact is intentional and correct for all single-host crawl cases.

**Rollback**: Revert the two-line change — `include_subdomains: false` → `true` in `config_impls.rs:20` and `global_args.rs:17`.

**No migration needed**: `include_subdomains` is a runtime flag resolved at crawl time. Stored `config_json` in DB jobs uses `serde` defaults — the `crates/jobs/crawl/runtime.rs` `CrawlJobConfig` struct defaults to the new `false` for new jobs; pre-existing queued jobs retain whatever value was serialized with them.

---

## Decisions Not Taken

- **Path-scoped crawl mode** (only follow links under `/docs/`): Would solve the specific `code.claude.com/docs` case but is over-engineered for the general case. The subdomain default fix solves the broader problem.
- **Detect-and-warn when crawl escapes start host**: Adding a warning when spider follows links to a different host would be useful telemetry but is a separate feature. Filed as a potential improvement.
- **Fixing the `code.claude.com/` root redirect**: Not fixable at the axon level — the 302 to `www.claude.com` is the website's decision. The correct fix is for users to crawl `code.claude.com/docs` directly.

---

## Open Questions

- **`crates::jobs::refresh::tests::claim_due_refresh_schedules_prevents_immediate_duplicate_claims`**: Pre-existing Postgres test failure with `pg_type` duplicate key. Likely a test isolation issue where a custom PG type is created twice in the same DB. Not investigated in this session.
- **spider `extract_root_domain` behavior on 2-part vs 3-part domains**: Confirmed that `extract_root_domain("code.claude.com")` returns `"claude"` (strips ALL subdomains). This means `with_subdomains(true)` on any 3-part domain is much broader than intuition suggests. The spider source at `~/.cargo/registry/.../spider-2.45.24/src/page.rs` should be reviewed if future multi-domain crawls are needed.

---

## Next Steps

- Run a full crawl of `code.claude.com/docs` to confirm all expected pages are indexed.
- Review any CI/CD workflows that invoke `axon crawl` with the old default behavior to confirm they still produce correct results (most will benefit from the fix).
- Consider adding a test to `engine/tests.rs` that asserts `configure_website()` with the default config applies `with_subdomains(false)`.
