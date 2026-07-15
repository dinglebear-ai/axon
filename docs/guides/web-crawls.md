# Web Sources
Last Modified: 2026-07-15

Web acquisition is handled through the unified source pipeline.

## Site Scope

Use site or docs scope for multi-page acquisition:

```text
axon https://example.com/docs --scope site
```

The web adapter owns URL discovery, render-mode behavior, sitemap backfill,
manifest items, document preparation, and embedding handoff.

## Page Scope

Use page scope or the retained `scrape` command for exactly one page. Page
scope does not fan out to sibling links or sitemap-discovered pages.

## Output

Clean content may be returned or written according to output policy, while
embedding is enabled by default unless the caller opts out.
