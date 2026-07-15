# Ask, Query, Retrieve, And Search
Last Modified: 2026-07-15

These commands have intentionally separate responsibilities.

## Commands

| Command | Purpose |
|---|---|
| `search` | external web discovery, optionally followed by indexing |
| `query` | vector and metadata retrieval from indexed content |
| `retrieve` | fetch stored chunks for a known source or URL |
| `ask` | retrieval plus LLM synthesis with citations |

## Guidance

Use `search` when the answer may not be indexed yet. Use `query` when you want
ranked evidence. Use `retrieve` when you already know the source identity. Use
`ask` when you want a synthesized answer grounded in indexed context.

The commands must not bypass source identity, freshness, vector filters, or
authorization rules.
