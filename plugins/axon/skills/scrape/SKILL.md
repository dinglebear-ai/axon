---
name: scrape
description: Use Axon scrape to turn one or more known URLs into markdown, HTML, JSON, or saved artifacts.
---

# Axon Scrape

Use `axon scrape` when the user gives a URL and wants that page's content.

`scrape` is the retained one-page projection of the unified source pipeline —
same adapter, ledger, embedding, and publication path as `axon <url> --scope page`.
Reach for it rather than a bare `axon <url>`: the web family's default scope is
`site`, so an unscoped URL indexes the whole domain.

## Examples

```bash
mkdir -p .axon

axon scrape "https://example.com" --output .axon/example.md
axon scrape "https://example.com/pricing" --format json --output .axon/pricing.json
axon scrape "https://app.example.com" --render-mode chrome --output .axon/app.md
```

Multiple URLs:

```bash
axon scrape "https://example.com" "https://example.com/docs" --output-dir .axon/pages
```

## Guidance

- Quote URLs.
- Save output to `.axon/` for anything larger than a short answer.
- Use Chrome rendering when HTTP output is thin.
- Use `extract` when the desired output is structured fields rather than markdown.
- Use `axon <url> --scope site` when you need many linked pages — `scrape` is the one-page projection of the same source pipeline.

## See Also

- [search](../search/SKILL.md)
- [map](../map/SKILL.md)
- [crawl](../crawl/SKILL.md)
- [extract](../extract/SKILL.md)
