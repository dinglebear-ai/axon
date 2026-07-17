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
axon map "https://docs.example.com" --json > .axon/docs-map.json
```

## Guidance

- Use map before crawl when the site may be huge.
- Map is bounded URL discovery only: sitemap and llms.txt first, then one root-page anchor fetch.
- After mapping, select the relevant URLs and pass them to `scrape`, `crawl`, or `extract`.

## See Also

- [search](../search/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [crawl](../crawl/SKILL.md)
