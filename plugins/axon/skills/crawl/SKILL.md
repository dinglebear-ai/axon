---
name: crawl
description: Use Axon crawl to capture many pages from a site or scoped documentation section.
---

# Axon Crawl

Use `axon crawl` when the user needs many pages from the same site, such as an entire docs section, changelog archive, blog, or product catalog.

## When To Use

- The user says "crawl", "bulk scrape", "get all pages", or "capture the docs".
- A single URL is not enough.
- You need content indexed or saved across a bounded path.

## Examples

```bash
mkdir -p .axon

axon crawl "https://docs.example.com" \
  --max-pages 100 \
  --max-depth 3 \
  --wait true \
  --output-dir .axon/docs-crawl

axon crawl "https://docs.example.com/reference" \
  --budget "/reference=200" \
  --wait true \
  --output-dir .axon/reference
```

## Guidance

- Scope crawls with path-specific URLs, `--max-pages`, `--max-depth`, `--budget`, or a tight start URL.
- Use `--wait true` when the answer depends on the completed crawl.
- Use `axon crawl status <job_id>`, `errors`, `list`, or `recover` for async jobs.
- Use `map` first if you only need URL discovery.

## See Also

- [map](../map/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [extract](../extract/SKILL.md)
