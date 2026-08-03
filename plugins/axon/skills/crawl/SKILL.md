---
name: crawl
description: Use Axon's unified source command to capture and index many pages from a bounded site or documentation section.
---

# Axon Site Capture

Use the unified source pipeline with `scope=site` when the user needs many
pages from one site, documentation section, changelog archive, blog, or product
catalog.

## When to use

- The user asks to crawl, bulk capture, or index a docs section.
- A single page is insufficient.
- The source can be bounded by URL, depth, page count, or path budget.

## CLI examples

```bash
mkdir -p .axon

axon source https://docs.example.com   --scope site   --max-pages 100   --max-depth 3   --wait true   --output-dir .axon/docs

axon source https://docs.example.com/reference   --scope site   --budget '/reference=200'   --wait true   --output-dir .axon/reference
```

Rendered capture:

```bash
axon source https://app.example.com/docs   --scope site   --render-mode chrome   --automation-script ./capture.json   --wait true
```

## MCP

The MCP source DTO supports the source, scope, collection, response mode, and
detached execution:

```json
{ "action": "source", "source": "https://docs.example.com", "scope": "site" }
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
```

Use the CLI when the task requires page limits, depth, budgets, output files,
render controls, WARC, headers, selectors, or automation scripts.

## Guidance

- Bound the capture with a scoped start URL and the smallest necessary limits.
- Use `--wait true` when the answer depends on completed indexing.
- Use `axon map <url>` first when only URL discovery is needed.
- Use `axon jobs get <job-id>`, `events`, `cancel`, `retry`, or
  `recover` for detached lifecycle management.
- Use `axon watch create <source> --every-seconds N` for recurring refreshes.

## See also

- [map](../map/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [extract](../extract/SKILL.md)
- [using-axon](../using-axon/SKILL.md)
