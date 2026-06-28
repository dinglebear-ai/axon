---
name: download
description: Save website or documentation content locally with Axon map, scrape, crawl, output-dir, and screenshots.
---

# Axon Download

Axon does not have a dedicated `download` command and does not create browser-ready offline mirrors with fetched assets and rewritten links. Use this skill to save markdown crawl artifacts, manifests, HTML/raw HTML captures, WARC archives, and screenshots locally with Axon's real capture commands.

## When To Use

- The user wants local markdown files, HTML/raw HTML captures, WARC archives, screenshots, or saved crawl artifacts.
- The goal is durable files rather than an immediate answer.
- The user says "download this site", "save the docs", or "archive this section".

## Recipes

Single page:

```bash
mkdir -p .axon/download
axon scrape "https://example.com/page" --output .axon/download/page.md
```

Section capture:

```bash
mkdir -p .axon/download
axon crawl "https://docs.example.com/reference" \
  --max-pages 200 \
  --wait true \
  --output-dir .axon/download/reference
```

Screenshot:

```bash
axon screenshot "https://example.com" --output .axon/download/example.png
```

## Guidance

- Start with `map` if you need to choose which paths to capture.
- Use `crawl` with explicit caps for large sections.
- Expect markdown, manifests, WARC files, and screenshots, not a full offline website mirror.
- Keep generated captures out of commits unless the user explicitly asks for curated artifacts.

## See Also

- [map](../map/SKILL.md)
- [scrape](../scrape/SKILL.md)
- [crawl](../crawl/SKILL.md)
