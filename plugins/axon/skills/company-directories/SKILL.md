---
name: company-directories
description: Use when extracting structured company lists from public or authorized directories into JSON, CSV, CRM-ready lists, or research tables.
---

# Axon Company Directories

Use this to turn startup or company directories into structured lists.

## Onboarding Interview

Infer the directory, filters, result count, and output format from context. If the source is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the directory URL/name, required filters, or target result count.

## Axon Collection Plan

Use `axon map`, `axon scrape`, and `axon extract` when listings are public and
static. Use separate browser automation for filters, pagination, infinite
scroll, or profile clicks only when the user confirms authorized access and
source terms permit automated extraction; feed discovered URLs/content back to
Axon.

Suggested sources include YC companies, Crunchbase, Product Hunt, G2 categories, or any custom directory URL.

For finished deliverables, run async Axon commands in blocking mode: `axon extract ... --wait true --json` and `axon crawl ... --wait true`. For public directories, pass `--respect-robots true` unless the user gives a specific authorized reason not to.

Stop at login walls, paywalls, CAPTCHAs, or contractual restrictions unless the
user confirms authorized access and source terms allow automated extraction.

## Extraction Fields

Capture fields that are visible:

- name
- description
- industry/category
- stage/founded/location/team size/funding when visible
- tags
- directory profile URL
- company website URL

Leave unavailable fields blank. Do not infer.

## Final Deliverable

```markdown
# Company Directory Export: [Source]

## Summary
[Filters, count extracted, limitations]

## Companies
[Table or link to JSON/CSV; each row includes sourceUrl, profileUrl, extractedAt, fieldsObserved, confidence, and limitations]

## Sources
[Directory pages and profiles used]

## Rerun Inputs
workflow: company-directories
directory: [source]
filters: [criteria]
max_results: [number]
output: [json/csv/markdown]
```

## JSON Shape

Use `source`, `filters`, `extractedAt`, `totalResults`, and `companies[]` with `name`, `url`, `description`, `industry`, `stage`, `founded`, `location`, `teamSize`, `funding`, `tags`, `profileUrl`, `websiteUrl`, `sourceUrl`, `fieldsObserved`, `confidence`, and `limitations`.

## Quality Bar

- Deduplicate companies.
- Track pagination progress.
- Respect source terms, rate limits, and robots where applicable; use `--respect-robots true` for public directory crawls by default.
- Stop at login walls, paywalls, or CAPTCHA blocks unless authorized and allowed.
- Record filters/query/date plus a rerun command or structured rerun config.
- For examples, see [workflow-output-templates.md](../../examples/workflow-output-templates.md).
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
