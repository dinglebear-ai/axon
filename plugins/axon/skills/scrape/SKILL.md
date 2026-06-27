---
name: scrape
description: Use Axon scrape to turn one or more known URLs into markdown, HTML, JSON, or saved artifacts.
---

# Axon Scrape

Use `axon scrape` when the user gives a URL and wants the page content.

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
- Use `crawl` when you need many linked pages.

## See Also

- [search](../search/SKILL.md)
- [map](../map/SKILL.md)
- [crawl](../crawl/SKILL.md)
- [extract](../extract/SKILL.md)
