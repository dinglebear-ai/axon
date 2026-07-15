# Source Pipeline
Last Modified: 2026-07-15

The unified source pipeline is Axon's canonical ingestion path.

## Flow

```text
SourceRequest
  -> adapter resolution
  -> source ledger generation
  -> manifest diff
  -> acquisition
  -> document preparation
  -> parsing and enrichment
  -> graph updates
  -> embedding
  -> vector publish
  -> cleanup debt
  -> SourceResult
```

## Public Entry Points

- `axon <source>` indexes a source.
- `axon scrape <url>` is retained as a one-page source request with page scope.
- `axon map <source>` discovers source items without vector publishing.
- REST and MCP callers use the same source DTOs.

## Non-Goals

There is no separate public crawl, embed, ingest, code-search, purge, or dedupe
pipeline. Those tokens are removed or folded into source, query, watch, and
prune behavior.
