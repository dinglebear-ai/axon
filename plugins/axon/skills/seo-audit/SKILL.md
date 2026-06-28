---
name: seo-audit
description: Use when auditing SEO with axon map, scrape, search, rendered checks, metadata review, sitemap/site analysis, SERP comparison, or scraped-page findings.
---

# Axon SEO Audit

Use this to turn a website into a specific, prioritized SEO audit.

## Onboarding Interview

Infer the site, target keywords, and output format from context. If the site is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the site URL, required target keywords, or whether a specific page/competitor set matters.

## Axon Collection Plan

1. Run `axon map --json` to understand URL structure. Treat this as discovery,
   not proof of sitemap health, orphan pages, broken links, canonical tags, or
   alt coverage.
2. Run targeted `axon scrape --json` on homepage, product/service pages,
   pricing, docs, blog, about, and high-value landing pages.
3. Inspect raw/rendered HTML when metadata, canonical tags, structured data, or
   image alt attributes matter. Mark unavailable metadata as "not observed," not
   "missing."
4. Run `axon endpoints <url> --include-bundles true --capture-network` when JavaScript bundles, API endpoints, or rendered network behavior could affect indexing or page health.
5. Search target keywords when provided; scrape top ranking pages for comparison.
6. Use `axon crawl --render-mode chrome --automation-script ... --wait true` for repeatable rendered capture before escalating to browser/link probing for broken links, console/network failures, or rendered-only SEO evidence.
7. For public-site crawl checks, pass `--respect-robots true` unless there is an explicit authorized reason not to.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- Site Structure: URL patterns, sitemap health, internal linking, orphan/broken pages.
- On-Page SEO: titles, meta descriptions, H1/H2 hierarchy, content quality.
- Keyword And SERP: target keywords, ranking pages, competitor page patterns.
- Technical Issues: broken links, duplicate content signals, missing metadata.

## Final Deliverable

```markdown
# SEO Audit: [Site]

## Executive Summary
[Top risks and opportunities]

## Site Structure
[Pages found, URL quality, sitemap/internal-link notes]

## On-Page SEO
[Per-page title, meta, headings, content, linking notes]

## Keyword Opportunities
[Target keywords, missing pages, content gaps]

## Competitor/SERP Comparison
[Who outranks the site and why]

## Prioritized Recommendations
[High/medium/low impact fixes with exact changes]

## Sources
[URLs scraped and what was checked]

## Rerun Inputs
workflow: seo-audit
site: [url]
keywords: [list]
output: [markdown/json]
```

## Quality Bar

- Make recommendations specific, not generic.
- Show the page or source behind each issue.
- Distinguish technical findings from content strategy guesses.
