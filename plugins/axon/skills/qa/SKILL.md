---
name: qa
description: QA test a live website with Axon discovery/content evidence plus browser automation when interaction is required. Use when the user wants exploratory QA, form testing, navigation/link checks, responsive checks, performance observations, bug reports, or a pre-launch quality review.
---

# Axon QA

Use this to test a live site and return a unified QA report.

## Onboarding Interview

Infer the URL, QA focus, and output format from context. If the target URL is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the URL, the focus area, or credentials/constraints for protected flows.

## Axon Collection Plan

Use `axon map` to discover pages, `axon scrape` for content and link evidence,
`axon screenshot` for rendered state, and `axon endpoints <url> --include-bundles true --capture-network` for API, bundle, and network-surface evidence. Use `axon crawl --render-mode chrome --automation-script ... --wait true` for repeatable click/scroll/load-more capture before escalating to a live browser tool.

For forms, responsive checks, console errors, and detailed reproduction evidence, use Webwright/Playwright or the host browser automation tool after Axon has collected the URL/content/API evidence.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- Full: Navigation and Links, Forms and Interactions, Content and Visual, Error States.
- Forms: Form Discovery, Happy Path, Edge Cases, Validation.
- Navigation: Sitemap, Nav Testing, Link Checker, Routing.
- Responsive: Desktop, Tablet, Mobile, Interaction.
- Performance: Page Load, Asset Audit, Content Efficiency, Comparison.

Each tester should return severity, URL, description, evidence, and reproduction steps.

## Final Deliverable

```markdown
# QA Report: [Site]

## Summary
- Health score: [x/10]
- Pages tested: [count]
- Issues found: [critical/major/minor]

## Critical Issues
[C-1] URL | Description | Steps to reproduce | Expected vs actual | screenshot path | browser/viewport | console/network evidence | auth state

## Major Issues
[M-1] URL | Description | Steps to reproduce | screenshot path | browser/viewport

## Minor Issues
[m-1] URL | Description

## Positive Observations
[What works well]

## Pages Tested
[URLs]

## Agent/Test Summary
[Who tested what]

## Rerun Inputs
workflow: qa
url: [url]
focus: [full/forms/navigation/responsive/performance]
```

## Quality Bar

- Include reproduction steps for functional issues.
- Do not report speculative bugs without evidence.
- Deduplicate findings across testers.
- For examples, see [workflow-output-templates.md](../../examples/workflow-output-templates.md).
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
