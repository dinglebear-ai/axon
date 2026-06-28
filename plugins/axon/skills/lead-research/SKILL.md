---
name: lead-research
description: Produce pre-meeting lead intelligence briefs with Axon. Use when the user needs company research, person research, recent news, talking points, pain points, or outreach preparation before a sales call, partnership meeting, investor conversation, or customer interview.
---

# Axon Lead Research

Use this to create a concise, actionable pre-meeting brief.

## Onboarding Interview

Infer the company, person, meeting context, and desired brief depth from context. If the company is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the company/person to research or the meeting context.

## Axon Collection Plan

Use `axon research` for first-pass multi-source synthesis, `axon search` for
source discovery, `axon scrape` for high-value URLs, and `axon ask`/`query` over
indexed content for synthesis checks.

Gather:

- company website, about, product, pricing, careers, team, and customer pages
- recent news, funding, launches, hiring, partnerships, and press
- public person profiles, talks, posts, interviews, and role/background
- relevant industry context and likely business challenges

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- Company Profile researcher
- Recent News and Activity researcher
- Person researcher
- Industry/Pain Point researcher

Each researcher should return source URLs and only evidence-backed claims.

Use only work-relevant public information. Do not include private contact
details, family/home data, protected characteristics, health/financial
inferences, or sensitive personal traits. Label hypotheses as hypotheses and
cite the evidence.

## Final Deliverable

```markdown
# Lead Brief: [Company]

## Company Overview
[What they do, stage/size signals, products, customers]

## Recent Activity
[News, launches, funding, hiring, partnerships; include lastCheckedAt and sources]

## Key People
[Relevant people and public background]

## Talking Points
[5-7 specific conversation starters]

## Likely Pain Points
[Evidence-backed hypotheses, labeled as hypotheses]

## Outreach Angle
[Suggested positioning or next step]

## Sources
[Inline citations or per-section source bullets, plus URLs used]

## Rerun Inputs
workflow: lead-research
company: [name/url]
person: [optional]
context: [meeting context]
```

## Quality Bar

- Keep it concise and useful before a meeting.
- Do not fabricate personal details.
- Clearly separate facts from inferred pain points.
- Keep person research work-relevant and avoid private or sensitive data.
