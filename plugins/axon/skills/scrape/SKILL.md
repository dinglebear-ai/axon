---
name: scrape
description: Use when the user wants to scrape a single URL or a few URLs to markdown, fetch a page's content, extract text from a web page, or save a URL's content into Qdrant. Triggers on "scrape this URL", "fetch the content of", "get the text from this page", "save this page to axon", "read this webpage into the RAG", or when the user pastes a URL and wants its content extracted. Prefer this over crawl when only specific pages are needed rather than a whole site.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-scrape

Scrapes one or more URLs to markdown and auto-embeds into Qdrant.

## MCP (preferred)

```json
{ "action": "scrape", "url": "https://example.com/article" }
```

With options:
```json
{
  "action": "scrape",
  "url": "https://example.com/docs/page",
  "root_selector": "article, main",
  "exclude_selector": ".sidebar, .nav, .ads",
  "format": "markdown",
  "embed": true
}
```

## CLI fallback

```bash
axon scrape https://example.com/article
axon scrape https://example.com/docs --format html --embed false
```

## Key options

| Option | Default | Notes |
|--------|---------|-------|
| `format` | `markdown` | `markdown`, `html`, `raw_html`, `json` |
| `embed` | `true` | Set `false` to skip Qdrant indexing |
| `root_selector` | — | CSS selector to scope extraction |
| `exclude_selector` | — | CSS selector to strip (nav, ads, etc.) |
| `render_mode` | `auto_switch` | `http`, `chrome`, `auto_switch` |

Use `"render_mode": "chrome"` when the page is JS-heavy or returns thin content with the default HTTP mode.

## After scraping

```json
{ "action": "ask", "query": "..." }
{ "action": "query", "query": "..." }
```
