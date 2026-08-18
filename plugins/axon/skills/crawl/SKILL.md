---
name: crawl
description: Use Axon site-scope source indexing to capture many pages from a site or scoped documentation section.
---

# Axon Site Capture

> **There is no `axon crawl` command.** Crawling is a *scope* of the unified
> source pipeline, not a separate command or job family. Use
> `axon <url> --scope site`.

Use site-scope indexing when the user needs many pages from the same site: an
entire docs section, changelog archive, blog, or product catalog.

## When To Use

- The user says "crawl", "bulk scrape", "get all pages", or "capture the docs".
- A single URL is not enough.
- You need content indexed or saved across a bounded path.

## Examples

```bash
mkdir -p .axon

axon "https://docs.example.com" \
  --scope site \
  --max-pages 100 \
  --max-depth 3 \
  --wait true \
  --output-dir .axon/docs-site

axon "https://docs.example.com/reference" \
  --scope site \
  --budget "/reference=200" \
  --wait true \
  --output-dir .axon/reference
```

`axon source <url> --scope site` is the explicit form; the bare `axon <url>`
above is the same thing.

## Guidance

- Scope the capture with a path-specific start URL plus `--max-pages`,
  `--max-depth`, `--budget PATH=N`, or `--exclude-path-prefix`.
- `--wait true` blocks until the job finishes. Without it the CLI enqueues and
  returns a job id — use that whenever the deliverable depends on completed
  indexing.
- Inspect async work through the **unified** job surface: `axon jobs get <job_id>`,
  `axon jobs events <job_id>`, `axon jobs list`, `axon jobs retry`,
  `axon jobs recover`. There are no per-family `crawl status`/`crawl errors`
  subcommands.
- Use `map` first if you only need URL discovery.
- Chrome rendering and capture-time automation stay on the same command:
  `--render-mode chrome`, `--automation-script <path>`,
  `--chrome-wait-for-selector`.

## See Also

- [map](../map/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [extract](../extract/SKILL.md)
