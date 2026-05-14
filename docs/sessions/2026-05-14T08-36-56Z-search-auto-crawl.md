# Search Auto-Crawl Session

Date: 2026-05-14
Worktree: `.worktrees/search-auto-crawl`
Branch: `codex/search-auto-crawl`
PR: https://github.com/jmagar/axon/pull/86

## Summary

Implemented the search auto-crawl plan in an isolated worktree. `axon search`
now queues one-page crawl jobs for returned result URLs, reports queued jobs and
rejections in JSON output, and keeps MCP `search` explicitly side-effect-free in
the generated MCP schema docs.

## Changes

- `src/cli/commands/search.rs`
  - passes `ServiceContext` into search handling
  - queues crawl jobs for search result URLs
  - clears caller headers and URL whitelists for search-created crawl jobs
  - caps auto-crawls to one page/depth one and disables sitemap expansion
  - preserves `--wait` for search-created crawl jobs
  - rejects missing, duplicate, blocked, and enqueue-failed URLs with structured reasons
  - returns an error before JSON output when every result fails to queue
- `src/core/http/*`
  - added DNS-aware URL validation for non-reqwest fetch paths
  - added resolved-IP SSRF regression coverage
- `src/jobs/lite/config_snapshot*`
  - split endpoint/path snapshot helpers
  - rejects malformed endpoint URLs instead of silently falling back
  - normalizes default host output paths for container workers without changing caller-facing result paths
- `src/jobs/lite/workers/runners/crawl.rs`
  - validates crawl job URLs with DNS-aware SSRF checks before Spider crawl
  - preserves caller-facing output paths and records worker paths separately when they differ
  - records sitemap backfill failures in result JSON
  - fails unexpected embed enqueue errors instead of marking the crawl as completed
- `scripts/mcp_doc_renderer.py` and `docs/MCP-TOOL-SCHEMA.md`
  - generated note documenting that MCP `search` has no auto-crawl side effect

## Reviews Addressed

- Lavra review:
  - fixed DNS-resolution SSRF gap in search enqueue and crawl worker paths
  - narrowed container output-path rewriting
  - replaced ignored live enqueue-failure test with deterministic unit coverage
  - moved config snapshot tests to the relevant modules
  - gated non-JSON warnings correctly
- Three `code_simplifier` agents:
  - simplified search enqueue aggregation
  - deduplicated URL parse/host validation logic
  - reduced config snapshot test visibility and locality
- PR Review Toolkit agents:
  - added hardened-config snapshot tests
  - added invalid/missing/duplicate URL tests
  - added endpoint, path contract, sitemap error, and worker SSRF tests
  - moved generated MCP docs text into the generator
  - preserved caller-facing and worker-facing output paths explicitly

## Verification

Final green pass after rebasing onto `origin/main`:

```text
python3 scripts/generate_mcp_schema_doc.py --check
python3 scripts/enforce_monoliths.py --base $(git merge-base HEAD origin/main) --head HEAD
git diff --check
RUSTC_WRAPPER= cargo check --bin axon
RUSTC_WRAPPER= cargo clippy --bin axon -- -D warnings
RUSTC_WRAPPER= cargo test --lib -- --nocapture
```

Results:

- generated MCP schema doc check passed
- monolith policy passed with warnings only for existing-large functions under the hard limit
- whitespace check passed
- cargo check passed
- clippy passed
- full library tests passed: 1610 passed, 0 failed, 5 ignored

## PR Comments

Fetched PR comments for #86 with the vibin gh-address-comments fetch script:

```text
python3 <vibin>/skills/gh-address-comments/scripts/fetch_comments.py --pr 86 --repo jmagar/axon --no-beads
```

Result: one CodeRabbit processing comment, no reviews, no review threads, and no
actionable comments to resolve.

## Open Questions

- CodeRabbit was still processing when comments were fetched. Re-run the PR
  comment fetch before merge if CodeRabbit posts actionable review threads.
- The Spider fetch path still relies on preflight DNS validation plus Spider
  blacklist patterns; there is no Spider connect-time resolver hook wired here.
