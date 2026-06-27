---
name: research-papers
description: Use when the user wants a literature review, paper summary, research landscape, or sourced synthesis from papers, reports, abstracts, or converted PDFs.
---

# Axon Research Papers

Use this to create a sourced literature review.

## Onboarding Interview

Infer the topic, source constraints, target count, and output format from context. If the topic is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the topic, target paper count, or required venue/date/method constraints.

## Axon Collection Plan

Use Axon research/search for discovery, scrape/summarize for individual sources,
ingest/embed for larger corpora, and ask/query for synthesis over indexed
content. Prefer primary sources, official HTML/abstract pages, PDFs that Axon
can fetch as readable markdown, or user-provided converted text artifacts.

Core tools:

- `axon research "<topic> papers literature review"` for broad
  discovery plus cited synthesis.
- `axon search "<paper/topic query>" --json` to gather candidate URLs.
- `axon scrape <url>` or `axon summarize <url>` for one
  paper, report, abstract page, HTML page, or readable PDF landing page.
- `axon ingest <repo-or-feed>` or `axon embed <path-or-url>`
  when the user provides a corpus to index.
- `axon ask "<question>"` and `axon query "<topic>"` after
  indexing to synthesize or inspect retrieved chunks.

Match the approach to the query:

- Single named paper: search/scrape the canonical paper page, then summarize or
  ask targeted questions over the scraped/indexed content.
- Paper by description, method, or topic family: search multiple framings, keep
  strong anchors, and follow citations or related-work links when available.
- Enumeration queries, such as papers that do a task or benchmark a method:
  search multiple framings, expand several strong anchors, and re-seed from
  newly found relevant papers.
- Papers that use or exhibit a property: start from the defining paper or
  strongest anchor, follow similar/citing/reference pages when available, and
  verify the property in the paper body before keeping the candidate.
- Superlatives and leaderboards: use general web search or scrape to find the
  ranking, then map top entries back to papers with paper search.
- Author, organization, venue, date, or methodology constraints: verify metadata
  and body text before keeping a candidate.
- If a direct PDF fails, scrape the abstract/HTML landing page, locate an
  accessible HTML version, or ask for/provide converted local text before
  embedding.

Target source types:

- academic papers from arXiv, university sites, ACM/IEEE pages where accessible
- industry reports and whitepapers
- company research blogs
- technical articles and conference summaries

Principles:

- When in doubt, include the relevant paper family rather than only the single
  best result.
- Follow references, citations, and related-work sections to avoid stopping at
  one strong hit.
- Verify load-bearing constraints in the source text; do not summarize every
  candidate in depth.
- Drop only clearly off-topic papers.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- Academic Papers researcher
- Industry Reports researcher
- Technical Articles researcher
- Synthesis and citation reviewer

## Final Deliverable

```markdown
# Literature Review: [Topic]

## Abstract
[2-3 paragraph summary]

## Key Papers
[Title, authors, year, venue, DOI/arXiv ID, peer-review status, source URL, key findings, methodology, relevance, fully-read vs abstract-only]

## Themes And Consensus
[What sources agree on]

## Open Questions And Debates
[Disagreements and unresolved questions]

## Emerging Trends
[Recent developments]

## Sources
[Organized by paper/report/article]

## Inclusion Criteria
[Date range, venues/source types, exclusion rules, and related-work/citation notes]

## Rerun Inputs
workflow: research-papers
topic: [topic]
target_count: [number]
output: [markdown/brief]
```

## Quality Bar

- Every major claim should trace to a source.
- Note inaccessible or failed PDFs.
- Distinguish peer-reviewed work from blogs and vendor reports.
- For current Axon capture patterns, see [capture-recipes.md](../../references/capture-recipes.md).
