---
name: deep-research
description: Use when the user explicitly needs a rigorous, cited, formal research report on a complex scientific, technical, policy, or market topic.
---

# Axon Deep Research

Use this only for report-scale research: a rigorous, cited synthesis the user
explicitly wants delivered as a formal written report. If the request is a
product pick, a top-N list, a quick lookup, or anything answerable with a short
search, stop; do not use this skill, let the request be handled the standard
way.

## Onboarding Interview

Infer the topic, scope, depth, and output format from context. Ask only when the
topic or a critical scope boundary is unclear. Otherwise default to Thorough for
formal reports.

## Axon Collection Plan

Start with Axon's native research pipeline through the CLI or equivalent tool
surface:

```bash
axon research "<topic>" --research-depth 5 > .axon/research.md
```

Then use `axon ask`, `axon query`, `axon retrieve`, targeted `axon scrape`, or
additional `axon research` passes only when the first-pass synthesis leaves gaps.
Match depth to the task scope:

- Quick: `--research-depth 3`, then one targeted follow-up if needed.
- Thorough: choose one value from 5 to 8, e.g. `--research-depth 8`, plus targeted primary-source scrapes.
- Exhaustive: multiple focused `research` passes and targeted primary sources, papers, expert views, and contrarian sources.

Avoid re-fetching URLs already covered by `research`, already indexed and
available via `retrieve`, or already scraped in this run.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners by research angle:

- overview and definitions
- technical or implementation details
- market and industry context
- contrarian views, risks, and limitations
- primary sources and official docs

Each researcher should return claims, source URLs, source quality notes, and uncertainty.

## Final Deliverable

Default structure:

```markdown
# Deep Research: [Topic]

## Executive Summary
[2-3 paragraphs]

## Key Findings
[Numbered findings with source links]

## Methodology
[Search queries, source types, inclusion/exclusion criteria, and access dates]

## Source Quality
[Primary/secondary/source-quality notes and confidence]

## Detailed Analysis
[Themes, evidence, and synthesis]

## Contrarian Views And Risks
[Counterarguments, limitations, failure modes]

## Open Questions
[What remains uncertain]

## Limits
[Known gaps, inaccessible sources, uncertainty, and recency caveats]

## Sources
[Every URL used with a one-line note]

## Rerun Inputs
workflow: deep-research
topic: [topic]
depth: [quick/thorough/exhaustive]
output: [markdown/json/brief]
```

## Quality Bar

- Cite sources for factual claims.
- Prefer primary sources when available.
- Flag uncertainty and conflicting evidence.
- Synthesize instead of listing scrape summaries.
- For current Axon capture patterns, see [capture-recipes.md](../../references/capture-recipes.md).
