---
name: market-research
description: Use for informational market, industry, company, earnings, or public financial research with Axon, not personalized investment advice.
---

# Axon Market Research

Use this for sourced market and financial research.

## Onboarding Interview

Infer the market/company, data focus, timeframe, and output format from context. If the research target is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the market/company, required data focus, or timeframe/geography.

## Axon Collection Plan

Use `axon search`/`research` for discovery, `axon scrape`/`summarize` for
narrative pages, `axon extract` for structured pricing, financial, filing, and
metric tables, and `axon ask`/`query` after indexing. Use separate browser
automation only when charts, tabs, period selectors, or financial portals
require interaction.

Common sources include company investor relations pages, SEC filings, financial portals, earnings releases, industry reports, and news.

Use `axon research "<topic>" --research-depth N` for the first-pass cited synthesis when a market question is broad. Use `axon extract ... --wait true --json` for finished metric tables, and pass `--respect-robots true` for public-site crawls unless there is an explicit authorized reason not to.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- company financials
- market metrics
- industry trends
- recent news and analyst commentary
- source validation

## Final Deliverable

```markdown
# Market Research: [Market]

## Market Overview
[Industry description, size, growth, key players]

## Company Profiles
[Financial summary, market metrics, recent developments]

## Comparison Tables
[Metric, value, unit, period, currency, source URL, extraction confidence]

## Trends And Outlook
[Industry trends, forecasts, risks]

## Sources
[URLs and data extracted]

## Rerun Inputs
workflow: market-research
query: [market/company]
companies: [list]
data_points: [all/financial/metrics/trends]
output: [json/markdown]
```

## Quality Bar

- Cross-reference key numbers when possible.
- Note conflicting data across sources.
- Include period, unit, currency, source date, and observed date for every metric.
- Separate reported facts, analyst estimates, and model-derived synthesis.
- Do not provide personalized investment, trading, tax, legal, or financial advice.
- For current Axon capture patterns, see [capture-recipes.md](../../references/capture-recipes.md).
