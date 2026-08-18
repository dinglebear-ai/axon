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
axon map "https://docs.example.com" --limit 500 --json > .axon/docs-map.json
```

## Guidance

- Map first when a site may be huge, so you can scope the follow-up capture instead of indexing blind.
- Map is bounded URL discovery only: sitemap and llms.txt first, then one root-page anchor fetch.
- After mapping, select the relevant URLs and pass them to `scrape` (one page), `axon <url> --scope site` (a section), or `extract` (structured fields).

## See Also

- [search](../search/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [crawl](../crawl/SKILL.md)
