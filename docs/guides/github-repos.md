# GitHub Repositories
Last Modified: 2026-07-15

GitHub repository indexing is handled as a source adapter, not as a separate
legacy ingest path.

## Inputs

Supported inputs include GitHub repository URLs and canonical git URLs. Private
repositories require credentials configured through the normal runtime
configuration path.

## Behavior

The adapter clones or fetches repository content, applies source limits,
prepares documents, extracts parser metadata where available, updates source
graph candidates, and publishes vectors when embedding is enabled.

## Freshness

Refreshes are represented as new source generations. Query freshness can prefer
committed or completed generations depending on caller intent.
