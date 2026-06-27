---
name: workflows
description: Route outcome-focused Axon requests only when no more specific bundled workflow skill is already selected.
---

# Axon Workflows

Use this when the user asks which Axon workflow to use, or when the user wants a finished Axon-powered deliverable and no more specific bundled workflow skill is already selected.

## Choose The Workflow

- Use [website-design-clone](../website-design-clone/SKILL.md) to extract a website's colors, fonts, spacing, components, and layout patterns into an agent-ready `DESIGN.md`.
- Use [deep-research](../deep-research/SKILL.md) for sourced multi-source research reports.
- Use [seo-audit](../seo-audit/SKILL.md) for site structure, on-page SEO, keyword, and SERP audits.
- Use [lead-research](../lead-research/SKILL.md) for pre-meeting company/person intelligence briefs.
- Use [qa](../qa/SKILL.md) for live-site QA testing and bug reports.
- Use [competitive-intel](../competitive-intel/SKILL.md) for recurring pricing, feature, and changelog monitoring.
- Use [company-directories](../company-directories/SKILL.md) for directory extraction into company lists.
- Use [dashboard-reporting](../dashboard-reporting/SKILL.md) for dashboard metrics extraction.
- Use [knowledge-base](../knowledge-base/SKILL.md) for LLM-ready docs, RAG chunks, training data, or docs mirrors.
- Use [knowledge-ingest](../knowledge-ingest/SKILL.md) for auth-gated or JS-heavy docs portal ingestion.
- Use [lead-gen](../lead-gen/SKILL.md) for prospect list generation.
- Use [market-research](../market-research/SKILL.md) for market, financial, and industry research.
- Use [research-papers](../research-papers/SKILL.md) for literature reviews from papers, PDFs, and whitepapers.
- Use [demo-walkthrough](../demo-walkthrough/SKILL.md) for product flow walkthroughs and UX teardown reports.
- Use [shop](../shop/SKILL.md) for product research and shopping recommendations.

If no existing workflow fits, use this generic process and produce a reusable pattern that could become a new skill.

## Required Intake

Infer the workflow, inputs, audience, and output format from the user's request and surrounding context. If enough is clear, start immediately.

Ask at most 1-3 concise clarifying questions only when a missing input would block the work, such as:

- the URL, company, topic, or source to analyze
- the desired deliverable or output format
- a constraint that would materially change the workflow

Use the host agent's normal way to ask clarifying questions. Do not depend on a harness-specific function name.

## Default Process

1. Identify the workflow and final artifact; ask only if a missing input blocks execution.
2. Collect web evidence with Axon through the CLI or equivalent Axon tool surface.
3. Save or cite source evidence so the final claims are traceable.
4. Run independent research units in parallel when available.
5. Synthesize findings into the requested deliverable.
6. Include a short "rerun inputs" block when the workflow could be automated.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners for independent units such as:

- one competitor per researcher
- one URL or page per researcher
- one source category per researcher
- one analysis dimension per reviewer

Keep the handoff generic: provide the unit of work, source URLs or search terms, expected extracted fields, and output format.

## Deliverable Standards

Every workflow should return:

- a concise executive summary
- the evidence base used
- the analysis or artifact requested by the user
- recommendations or next actions when useful
- automation inputs for reruns

For authoring new workflow skills, see [workflow-authoring.md](../../references/workflow-authoring.md).
For current Axon capture patterns, see [capture-recipes.md](../../references/capture-recipes.md).
