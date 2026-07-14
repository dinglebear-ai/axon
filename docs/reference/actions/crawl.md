# axon crawl
Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon crawl` reserved; use `axon <url> --scope site|docs` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


`axon crawl` is reserved after the unified source cutover. It is not a
canonical user-facing command and should fail before dispatch with replacement
guidance. Use `axon <url> --scope site` or `axon <url> --scope docs` for
site/docs acquisition, `axon scrape <url>` for exactly one page, REST
`POST /v1/sources`, or MCP `action=source`.

## Synopsis

```bash
axon <url> --scope site [FLAGS]
axon <url> --scope docs [FLAGS]
axon scrape <url> [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>` | Web source to acquire through `SourceRequest` |

## URL Input Rules

- `axon crawl ...` is a reserved removed-command token and is not routed as a
  bare source.
- At least one URL is required for the replacement source command.
- URL inputs are normalized and deduplicated before enqueue/run.

## Source Job Replacement

Site/docs acquisition creates Source jobs, not Crawl jobs. The same job id is
used through acquire, prepare, embed, publish, graph, artifacts, and progress
events. There is no child Embed handoff.

Key replacement flags:

| Flag | Meaning |
|------|---------|
| `--scope site` | Acquire a bounded site graph through the web Source adapter. |
| `--scope docs` | Acquire a documentation subtree through the web Source adapter. |
| `--scope page` | Acquire exactly one page; `axon scrape <url>` is the convenience form. |
| `--max-pages <n>` | Bound site/docs acquisition. |
| `--max-depth <n>` | Bound link traversal depth. |
| `--wait true` | Wait for the submitted Source job. |
| `--no-embed` | Acquire/normalize without vector writes. |
| `--warc <path>` | Store WARC output as an ArtifactStore-backed artifact when supported by the web adapter. |
| `--automation-script <path>` | Run Chrome automation steps for matching pages when using a Chrome-capable render path. |

Inspect work with `axon jobs list`, `axon jobs status <job_id>`,
`GET /v1/jobs`, or `GET /v1/jobs/{id}`.

## Examples

```bash
# Default async site acquisition
axon https://example.com --scope site

# Chrome-only crawl with custom limits
axon https://example.com --scope site --render-mode chrome --max-pages 200 --max-depth 3

# Archive every fetched page to a WARC 1.1 file
axon https://example.com --scope site --wait true --warc out/example.warc

# Chrome crawl driven by web-automation steps
axon https://example.com --scope site --render-mode chrome --automation-script steps.json

# Job status
axon jobs status 550e8400-e29b-41d4-a716-446655440000

# One-page scrape projection
axon scrape https://example.com --wait true

# Enqueue locally and print JSON
axon https://example.com --scope site --json
```

## Behavior Notes

- `AXON_SERVER_URL` does not route CLI work through old family routes. Use
  REST `/v1/sources` or MCP `action=source` for client/server source work.
- The old `/v1/crawl` route and MCP `crawl` action are removed.
- Historical JSON fields named `crawl_jobs` may still appear in search/research
  output for compatibility; the queued work is Source jobs.
- Legacy `JobKind::Crawl` rows may exist from older databases. They are
  migration-only and are dead-lettered with `legacy.crawl.removed` instead of
  recovered or requeued.
