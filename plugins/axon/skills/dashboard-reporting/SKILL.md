---
name: dashboard-reporting
description: Use when the user needs dashboard reporting from authorized browser-accessible pages; use external browser automation for login/UI interaction and Axon for public scrape/screenshot captures or exported artifacts.
---

# Axon Dashboard Reporting

Use this to extract visible metrics from dashboards the user can legitimately access.

## Onboarding Interview

Infer dashboard URLs, metrics, date range, and output format from context. If dashboard targets are clear and accessible, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the dashboard URLs, auth/profile requirement, or date range.

## Axon Collection Plan

Use external browser automation for authenticated dashboards and UI interaction.
Use Axon screenshot/scrape artifacts when pages are publicly reachable, after
authorized URLs are discovered, or when a rendered capture helps preserve evidence:

- open each dashboard
- set or verify date range
- extract visible KPI cards, tables, and labels
- click tabs, expand sections, and scroll tables
- use export/download buttons only when appropriate and allowed

If login has expired, ask the user to re-authenticate rather than attempting to bypass access controls.
Only capture dashboards the user is authorized to access. Redact secrets,
customer PII, and sensitive rows unless explicitly requested. Avoid saving
screenshots or exports containing sensitive data when a summarized metric table
is enough. Record date range and source without exposing tokens or session URLs.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners. Split by dashboard platform or metric category. Each researcher should return metrics, units, period, source URL, and caveats.

## Final Deliverable

```markdown
# Dashboard Report

## Summary
[Highlights, alerts, trends]

## Metrics By Dashboard
[Platform, metric, value, unit, change, period]

## Tables Or Exports
[Captured tables/files and what they contain]

## Notes And Caveats
[Auth issues, chart-only data, unavailable metrics]

## Rerun Inputs
workflow: dashboard-reporting
dashboards: [urls]
date_range: [range]
metrics: [list]
output: [json/markdown]
```

## JSON Shape

Use `reportedAt`, `dateRange`, `dashboards[]`, `metrics[]`, `tables[]`,
`exports[]`, and `summary`. Each metric should include `name`, `value`, `unit`,
`period`, `sourceUrl`, `extractedAt`, `confidence`, and `caveats`.

## Quality Bar

- Extract actual numbers, not just chart labels.
- Note when a chart cannot be read precisely.
- Preserve date ranges and source URLs.
- For examples, see [workflow-output-templates.md](../../examples/workflow-output-templates.md).
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
