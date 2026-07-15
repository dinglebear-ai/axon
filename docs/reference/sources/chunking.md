# Chunking
Last Modified: 2026-07-15

Chunking converts prepared documents into embedding-ready units.

## Requirements

- Preserve source id and generation id.
- Preserve document id and stable chunk ordering.
- Bound chunk size for the active embedding provider.
- Deduplicate exact chunks when configured.
- Keep enough metadata for retrieval, citation, and cleanup.

## Ownership

Document preparation owns clean text. Chunking owns boundaries and payload
metadata for embedding and vector storage.
