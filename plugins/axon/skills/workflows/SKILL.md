---
name: workflows
description: Use when routing an outcome-focused Axon request and no more specific bundled workflow skill is selected.
---

# Axon Workflows

Use this when the user asks which Axon workflow to use, or when the user wants a finished Axon-powered deliverable and no more specific bundled workflow skill is already selected.

## Choose The Workflow

Load a named sibling skill only when it is installed; otherwise use the generic process below.

- Use `website-design-clone` to extract a website's colors, fonts, spacing, components, and layout patterns into an agent-ready `DESIGN.md`.
- Use `deep-research` for sourced multi-source research reports.
- Use `seo-audit` for site structure, on-page SEO, keyword, and SERP audits.
- Use `lead-research` for pre-meeting company/person intelligence briefs.
- Use `qa` for live-site QA testing and bug reports.
- Use `competitive-intel` for recurring pricing, feature, and changelog monitoring.
- Use `company-directories` for directory extraction into company lists.
- Use `dashboard-reporting` for dashboard metrics extraction.
- Use `knowledge-base` for LLM-ready docs, RAG chunks, training data, or docs mirrors.
- Use `knowledge-ingest` for auth-gated or JS-heavy docs portal ingestion.
- Use `lead-gen` for prospect list generation.
- Use `market-research` for market, financial, and industry research.
- Use `research-papers` for literature reviews from papers, PDFs, and whitepapers.
- Use `demo-walkthrough` for product flow walkthroughs and UX teardown reports.
- Use `shop` for product research and shopping recommendations.

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
