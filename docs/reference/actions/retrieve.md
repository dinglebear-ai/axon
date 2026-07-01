# axon retrieve
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon retrieve ...` |
| REST | `POST /v1/retrieve` (Implemented) |
| MCP | `{ "action": "retrieve" }` |
| Service | `services::query::retrieve` |

Parity notes: Supports collection, max_points, cursor, and token_budget.
<!-- END GENERATED ACTION SURFACES -->


Retrieve stored document content from Qdrant by URL. The command resolves URL variants, fetches matching chunks, orders by `chunk_index`, and prints reconstructed text.

## Related Retrieval Commands

| Command | Meaning |
|---|---|
| `search` | External web discovery; current runtime also auto-queues bounded crawl/index jobs for results. |
| `query` | Ranked semantic search over content already indexed in Qdrant. |
| `retrieve` | Stored content lookup/reconstruction by known URL or source identity. |
| `ask` | RAG synthesis over indexed context with an LLM answer. |

## Synopsis

```bash
axon retrieve <url> [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>` | URL (or URL-like target) used for payload `url` lookup. |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL. |

`retrieve` reads existing points from Qdrant and does not call TEI.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `axon` | Qdrant collection to read from. Also settable via `AXON_COLLECTION`. |
| `--since <time>` | none | Restrict retrieved chunks to content indexed on or after this time. Supports `7d`, `30d`, `YYYY-MM-DD`, and RFC3339. |
| `--before <time>` | none | Restrict retrieved chunks to content indexed on or before this time. Supports the same formats as `--since`. |
| `--max-points <n>` | service ceiling | Maximum chunks to fetch before reconstructing the document. Values above the service ceiling are capped at 500 chunks. |
| `--json` | `false` | Outputs `{url, chunks, content}` JSON. |

Note: `retrieve` runs synchronously and does not enqueue jobs.

## Examples

```bash
# Retrieve indexed content by source URL
axon retrieve https://docs.rs/spider

# Specific collection
axon retrieve https://qdrant.tech/documentation --collection docs-local

# JSON output
axon retrieve https://docs.rs/spider --json

# Limit the number of chunks reconstructed
axon retrieve https://docs.rs/spider --max-points 50

# Time-bounded retrieval
axon retrieve https://docs.rs/spider --since 30d --before 2026-05-01
```

## Notes

- Lookup tries normalized URL variants (`target`, normalized, no-trailing-slash, trailing-slash).
- Retrieved points are capped at 500 chunks per request (hard ceiling). `--max-points` can request a smaller cap.
- MCP `retrieve` accepts the same collection and time-filter controls via `collection`, `since`, and `before`, plus `max_points` and `response_mode`.
- If no matching payload URL is found, output is `no content found for URL: ... — run 'axon sources' to list indexed URLs`.
