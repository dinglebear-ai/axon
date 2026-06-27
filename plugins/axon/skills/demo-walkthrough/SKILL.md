---
name: demo-walkthrough
description: Use when the user needs a product walkthrough from authorized browser automation, with Axon used for public page scrape/screenshot evidence or captured URLs.
---

# Axon Demo Walkthrough

Use this to document a product experience step by step.

## Onboarding Interview

Infer the product URL, flow focus, and output format from context. If the URL is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the URL, desired flow focus, or credentials/constraints for protected areas.

## Axon Collection Plan

Use external browser automation to open the product and navigate key flows.
Capture desktop and mobile viewport screenshots at each step, record
URL/action/observed result, and note console/network errors. Use Axon
scrape/screenshot for public pages, captured URLs, or exported artifacts when
useful evidence can be captured without interaction.

Do not submit real credentials, purchases, or irreversible actions unless the user explicitly instructs and has permission.
For authenticated flows, use only user-authorized sessions and avoid saving
credentials or sensitive session URLs in artifacts.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- Homepage and Marketing
- Signup and Onboarding
- Pricing and Plans
- Docs and Developer Experience
- Dashboard and Core Product
- Help and Support

Each walker should return screens visited, actions taken, observations, friction, and source URLs.

## Final Deliverable

```markdown
# Product Walkthrough: [Product]

## Product Overview
[What the product does]

## Flow Walkthroughs
### [Flow Name]
1. [Screen/Page] - what appears and what action is available
   Evidence: [screenshot path, URL, viewport, action, observed result]
2. [Next Screen] - what changes
   Evidence: [screenshot path, URL, viewport, action, observed result]

## Key Findings
[First impression, standout patterns, friction points]

## Recommendations
[UX/product improvements]

## Pages Visited
[URLs]

## Evidence
[Screenshot paths, viewport coverage, console/network notes, auth state]

## Rerun Inputs
workflow: demo-walkthrough
url: [url]
focus: [full/signup/pricing/docs/dashboard]
```

## Quality Bar

- Be specific about screens, CTAs, forms, and transitions.
- Separate observation from opinion.
- Preserve every page visited.
- For examples, see [workflow-output-templates.md](../../examples/workflow-output-templates.md).
- For Axon/browser routing, see [capture-recipes.md](../../references/capture-recipes.md).
