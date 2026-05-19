# Session Addendum: Crawl Proof Testing + Postgres Duration Storage

**Date:** 2026-03-03
**Continues from:** `docs/sessions/2026-03-03-spider-feature-flags-autoswitch-inline-chrome.md`
**Branch:** feat/sidebar

---

## Session Overview

Post-implementation proof testing of the inline Chrome AutoSwitch feature across multiple real sites. Discovered floating-ui.com/docs is a dead URL (404), not a Chrome failure. Clarified Postgres duration storage via Postgres MCP. Identified that `Page.setContent()` limitation hypothesis was premature — the failure was a dead URL.

---

## Key Findings

### Crawl Results Across Real Sites

| Site | Pages | Thin % | Outcome |
|------|-------|--------|---------|
| `https://floating-ui.com/docs` | 0/0 | 0% | 404 — URL is dead, `/docs` path doesn't exist on Netlify |
| `https://mantine.dev/core/button` | 1/1 | 0% | Correct — single page URL, not a docs root |
| `https://www.radix-ui.com/primitives/docs` | 107/107 | 1.9% | Clean crawl, 2 thin pages filtered |
| `https://storybook.js.org/docs` | 1346+ crawled | unknown | Still running at time of save |
| `https://chakra-ui.com/` | 252/252 | 0% | Clean, all HTTP |
| `https://framer.com/motion` | 721+ crawled | 0% | Auto-switch armed, no thin pages triggered |

### floating-ui.com/docs — Root Cause

```
curl -sI https://floating-ui.com/docs
→ HTTP/2 404 (Netlify)

curl -sIL https://floating-ui.com
→ HTTP/2 200
```

The `/docs` path returns 404 — floating-ui restructured their site. Spider crawled the 404 response body, it was under 200 chars (thin), inline Chrome couldn't recover a 404, `drop_thin_markdown: true` dropped it. **Not a bug in `Page.setContent()` or the inline Chrome path.** The implementation is correct.

### Postgres Duration Storage

Two complementary metrics stored per job — not contradictory:
- `result_json->>'elapsed_ms'` — spider crawl phase only (internal timer, excludes queue wait / embed dispatch)
- `EXTRACT(EPOCH FROM (finished_at - started_at))` — full job wall-clock time derivable from timestamps

No dedicated `duration_ms` column. Both metrics available without schema changes.

### mantine.dev/core/button — Why 1 Page

`include_subdomains: false` + start URL is a single component page (`/core/button`), not the docs root. Spider followed no outbound links. Correct behavior — to crawl full Mantine docs, start URL should be `https://mantine.dev` or `https://mantine.dev/getting-started`.

---

## Technical Decisions

### `Page.setContent()` hypothesis retracted

Earlier concern that `Page.setContent()` can't load external JS bundles was premature — the only evidence was floating-ui.com/docs returning 0 pages, which turned out to be a 404. No true CSR SPA has been tested against the inline Chrome path with a valid URL yet. The hypothesis remains unverified either way.

### Postgres MCP over `docker exec psql`

Switched to `mcp__postgres__query` for all Postgres queries. Cleaner, no shell escaping, results come back as structured JSON. Should be default for all future DB inspection.

---

## Commands Executed

```bash
# Check floating-ui URL
curl -sI https://floating-ui.com/docs
→ HTTP/2 404

curl -sIL https://floating-ui.com
→ HTTP/2 200
```

```sql
-- Postgres MCP: inspect two failed crawl jobs
SELECT url, result_json, config_json FROM axon_crawl_jobs
WHERE id IN ('f9d1e145-...', '3e41ee53-...');
```

---

## Open Questions

- `https://floating-ui.com` redirects to 200 — what is the correct docs URL now? Likely `https://floating-ui.com/v1/docs/getting-started` or similar.
- storybook.js.org crawl still running at session save — result unknown.
- True CSR SPA test (valid URL, empty HTTP shell) still untested against inline Chrome `Page.setContent()`. The limitation may or may not exist.

---

## Next Steps

- Find the current floating-ui docs URL and re-crawl
- Let storybook.js.org finish and review thin page count
- Test inline Chrome against a confirmed CSR SPA with a valid URL to verify `Page.setContent()` behavior
- Consider adding `duration_ms` as a generated column (`finished_at - started_at`) to simplify UI queries
