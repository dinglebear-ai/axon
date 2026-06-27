---
name: extract
description: Use Axon extract for schema-shaped, LLM-assisted structured extraction from one or more URLs.
---

# Axon Extract

Use Axon's `extract` command when the user wants structured data from web pages: pricing tiers, product records, directory entries, tables, or any JSON-shaped output.

## When To Use

- The user asks for structured data rather than a readable markdown page.
- The target content spans repeated cards, listings, product pages, or directories.
- A schema, field list, or output contract is available or easy to infer.
- A plain `scrape` would leave too much manual parsing.

## Workflow

1. Scope the target URLs with `axon search`, `axon map`, or known URLs.
2. Write the desired fields or output schema into your task notes when it is more than a one-liner.
3. Run `axon extract ... --wait true --json` and save output under `.axon/`.
4. Inspect the result against the desired fields before transforming or summarizing it.

## Examples

```bash
mkdir -p .axon

axon extract "https://example.com/pricing" \
  --wait true \
  --json > .axon/pricing-extract.json

axon extract "https://example.com/products" "https://example.com/products/page-2" \
  --wait true \
  --json > .axon/products.json
```

For broad sites, crawl or map first:

```bash
axon map "https://docs.example.com" --json > .axon/docs-map.json
axon crawl "https://docs.example.com/reference" --max-pages 100 --wait true --output-dir .axon/reference-crawl
```

## Notes

- Axon is self-hosted; do not mention hosted API credits, hosted-team limits, or unrelated API keys.
- Prefer `scrape` for a single readable page and `extract` when the desired result is structured fields.
- If extraction needs authenticated or interactive page state, use Axon's Chrome rendering and automation-script support from the scrape/crawl flow first, then extract from the reachable URL set.

## See Also

- [scrape](../scrape/SKILL.md) for one-page markdown extraction.
- [crawl](../crawl/SKILL.md) for bulk site capture.
- [map](../map/SKILL.md) for URL discovery.
