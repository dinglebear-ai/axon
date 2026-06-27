---
name: knowledge-ingest
description: Use when ingesting docs portals where Axon Chrome rendering, crawl automation scripts, or host browser-assisted URL discovery are needed.
---

# Axon Knowledge Ingest

Use this when a docs portal needs JS rendering, pagination handling, or
browser-assisted URL discovery before Axon ingestion.

## Onboarding Interview

Infer the portal URL, output format, auth needs, and page limit from context. If the portal is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the portal URL, whether authentication is required, or the desired output format.

## Axon Collection Plan

Use Axon `scrape`/`crawl --render-mode chrome` and `--automation-script` when
Chrome rendering or scripted capture-time page steps can collect content. For
login flows or interactive portal navigation, use host browser automation to
establish access and discover URLs, then feed discovered URLs/content to Axon.

For finished ingest deliverables, run async Axon commands in blocking mode: `axon crawl ... --wait true`, `axon ingest ... --wait true`, and `axon embed ... --wait true`. If you enqueue instead, include the job ID, status command, and worker requirement in the handoff.

Collect:

- open the portal and inspect navigation
- identify sections, categories, sidebar links, and article URLs
- follow sidebar navigation, next links, pagination, load-more controls, or search when supported by Axon automation scripts or host browser discovery
- scrape article content as markdown
- extract metadata such as title, section, last updated date, author, and tags

Try `axon map` as a supplement for public URLs. Stop at unavailable auth-gated
content unless the user has authorized access and the source permits collection.

## Final Deliverable

```markdown
# Knowledge Ingest: [Portal]

## Summary
[Pages extracted, sections covered, limitations]

## Output
[JSON/markdown/merged file path or content]

## Sections
[Section names and article counts]

## Failed Or Restricted Pages
[Any access/loading issues]

## Sources
[URLs extracted]

## Rerun Inputs
workflow: knowledge-ingest
url: [portal url]
format: [json/markdown/merged]
max_pages: [number]
```

## JSON Shape

Use `source`, `url`, `extractedAt`, `totalArticles`, and `sections[]` with article `title`, `url`, `section`, `content`, and `metadata`.

When Axon does not emit this shape directly, transform scraped/extracted results
into this JSON schema as the final deliverable.

## Quality Bar

- Preserve code examples, tables, and formatting.
- Strip nav chrome, headers, and footers.
- Track extraction progress and page failures.
- Respect authentication boundaries.
