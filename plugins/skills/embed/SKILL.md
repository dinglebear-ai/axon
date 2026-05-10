---
name: embed
description: Use when the user wants to embed a local file, directory, or URL into Qdrant; index local documents or code into the RAG; add files from disk to the knowledge base; or re-embed stale content. Triggers on "embed this file", "index this directory", "add to Qdrant", "embed local files", "embed this folder", "index my docs", "add this PDF", "embed into the knowledge base". Different from scrape/crawl (which fetches from web) — embed indexes content already on disk or from a URL directly.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-embed

Embeds local files, directories, or URLs directly into Qdrant.

## MCP (preferred)

```json
{ "action": "embed", "input": "./docs" }
{ "action": "embed", "input": "/path/to/document.md" }
{ "action": "embed", "input": "https://example.com/article" }
```

## CLI fallback

```bash
axon embed ./docs
axon embed /path/to/document.md
axon embed https://example.com/article
```

## What gets embedded

- **Files**: chunked at 2000 chars with 200-char overlap, each chunk = one Qdrant point
- **Directories**: all readable text files recursively (`.md`, `.txt`, `.rs`, `.py`, `.ts`, etc.)
- **URLs**: fetches the page, converts to markdown, then chunks and embeds

## Job lifecycle

Embed is async:
```json
{ "action": "embed", "subaction": "status", "job_id": "<uuid>" }
{ "action": "embed", "subaction": "list", "limit": 10 }
```

## Re-embedding

Re-running embed on the same input replaces existing points for that source, then re-indexes cleanly.

## Verify results

```json
{ "action": "sources" }   // confirm new URLs appear
{ "action": "stats" }     // check total point count increased
```
