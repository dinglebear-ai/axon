---
name: competitive-intel
description: Monitor competitor pricing, features, changelogs, dashboards, and product changes with Axon. Use for recurring competitive intelligence, pricing tier extraction, feature change tracking, or structured competitor alerts.
---

# Axon Competitive Intel

Use this for monitoring competitors over time. This is not the broad competitor analysis workflow.

## Onboarding Interview

Infer competitors, focus, cadence, and output format from context. If competitors are clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the competitor list, focus area, or whether authenticated pages/profiles are required.

## Axon Collection Plan

For each competitor, use Axon surfaces by cadence:

- One-off: `axon scrape` or `axon extract` for current pages; use `axon diff`
  when a prior URL/version exists.
- Recurring: create a URL watch with `axon watch create`, verify with
  `axon watch exec`, and review history with `axon watch history`.
- Alerts: treat these as report sections unless a notification path is
  separately configured.

Collect:

- pricing pages, annual/monthly toggles, expanded feature tables
- feature and product pages
- changelogs, blogs, release notes, docs updates
- authenticated dashboards only when the user has legitimate access, source
  terms permit collection, and no auth, CAPTCHA, paywall, rate-limit, or terms
  boundary is bypassed

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners. A natural split is one competitor per researcher or one focus area per researcher.

Each researcher should return pricing tiers, features, recent changes, source URLs, and confidence notes.

## Final Deliverable

```markdown
# Competitive Intel: [Competitors]

## Alerts
[Notable pricing, feature, or positioning changes]

## Per-Competitor Breakdown
[Pricing tiers, feature inventory, recent changes]

## Cross-Competitor Comparison
[Pricing table, feature matrix, key differentiators]

## Suggested Follow-Ups
[What to monitor next]

## Sources
[URLs visited]

## Rerun Inputs
workflow: competitive-intel
competitors: [list]
focus: [all/pricing/features/changelog]
cadence: [one-off/weekly/monthly]
```

## JSON Shape

When structured output is requested, include `generatedAt`, `baselineDate`,
`observedAt`, `competitors`, `pricing`, `recentChanges`, `features`,
`changedFields`, `unchangedFields`, `confidence`, `sourceSnapshots`, `watchId`,
`failedSources`, and `sources`.

## Quality Bar

- Extract real plan names, limits, and dates when available.
- Note contact-sales or gated details instead of guessing.
- Preserve sources for diffing future runs.
- Do not bypass auth, CAPTCHA, paywalls, rate limits, or source terms.
- For current Axon capture patterns, see [capture-recipes.md](../../references/capture-recipes.md).
