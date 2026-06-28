---
name: map
description: Use Axon map to discover URLs on a site before scraping, crawling, or extracting.
---

# Axon Map

Use `axon map` when the user knows the site but not the exact URL, or when you need a URL inventory before deciding what to scrape or crawl.

## Examples

```bash
mkdir -p .axon

axon map "https://docs.example.com" --json > .axon/docs-map.json
axon map "https://docs.example.com" --map-fallback crawl --json > .axon/docs-map-crawl.json
```

## Guidance

- Use map before crawl when the site may be huge.
- Use sitemap-backed map for quick discovery, and `--map-fallback crawl` only when necessary.
- After mapping, select the relevant URLs and pass them to `scrape`, `crawl`, or `extract`.

## See Also

- [search](../search/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [crawl](../crawl/SKILL.md)
