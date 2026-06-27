---
name: lead-gen
description: Use when generating CRM-ready lead lists from public or authorized sources with axon search, research, scrape, map, or extraction.
---

# Axon Lead Gen

Use this to extract legitimately accessible prospect lists.

## Onboarding Interview

Infer the prospect target, source, lead count, and output format from context. If the target is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the prospect definition, source/auth requirement, or target lead count.

## Axon Collection Plan

Use `axon search`, `axon research`, `axon scrape`, `axon map`, and `axon extract`
for public or authorized sources. For filters, forms, pagination, or login,
use separate browser automation only when the user confirms authorized access
and source terms permit automated extraction; feed discovered URLs/content back
to Axon.

For finished lead-list deliverables, run async Axon commands in blocking mode: `axon extract ... --wait true --json`, `axon crawl ... --wait true`, and `axon ingest ... --wait true` when ingestion is part of the workflow. For public lead sources, pass `--respect-robots true` unless there is an explicit authorized reason not to.

Do not collect personal contact info unless the user confirms lawful basis and
source terms permit it. Do not infer, enrich, guess, de-obfuscate, or use scraped
personal data for unsolicited outreach. Exclude sensitive/protected traits and
minors. Include source, basis, suppression/opt-out notes, and limitations.

Apply filters such as role, company size, industry, geography, funding stage, and technologies when available.

## Extraction Fields

Capture visible or legitimately accessible fields:

- name
- title
- company
- company URL
- location
- email, phone, and LinkedIn only when visible, lawful, and allowed by source terms
- industry, company size, funding stage
- notes and profile URL

## Final Deliverable

```markdown
# Lead List: [Target]

## Summary
[Source, filters, count, caveats]

## Leads
[Table or link to JSON/CSV; each row includes sourceUrl, profileUrl, extractedAt, fieldsObserved, confidence, and limitations]

## Data Gaps
[Masked, unavailable, or paywalled fields]

## Rerun Inputs
workflow: lead-gen
target: [description]
source: [auto/source/url]
max_leads: [number]
output: [json/csv/markdown]
```

## Quality Bar

- Only extract publicly visible or legitimately accessible data.
- Confirm lawful basis and source terms before collecting personal contact info.
- Note masked, unavailable, or paywalled fields.
- Deduplicate leads.
- Do not bypass CAPTCHAs or access controls.
- Respect rate limits/robots where applicable, cap request volume, and record
  filters/query/date plus a rerun command or structured rerun config.
- For examples, see [workflow-output-templates.md](../../examples/workflow-output-templates.md).
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
