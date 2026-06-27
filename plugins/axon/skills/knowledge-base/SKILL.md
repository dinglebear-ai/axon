---
name: knowledge-base
description: Use when crawling docs, ingesting source families, building or refreshing a Qdrant-backed RAG corpus, capturing docs sections, or producing LLM-ready markdown.
---

# Axon Knowledge Base

Use this to turn URLs or topics into organized LLM-ready content.

## Onboarding Interview

Infer the source, goal, depth, and output location from context. If the source and goal are clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the source URL/topic, whether the output is reference/RAG/training/docs, or training format if training is requested.

## Axon Collection Plan

Choose the Axon surface by source shape:

| Need | Axon surface |
|---|---|
| Discover URLs only | `axon map <url>` |
| Capture a docs site or section as artifacts | `axon crawl <url> --output-dir <dir>` |
| Fetch selected pages | `axon scrape <url> --output-dir <dir>` |
| Discover topic sources | `axon search` or `axon research` |
| Ingest repos, RSS, Reddit, YouTube, or sessions | `axon ingest` / `axon sessions` |
| Refresh existing indexed corpus | `axon refresh [filter] --yes` |
| Schedule freshness | `axon crawl|scrape|embed|ingest ... --fresh <Nd>` |
| Reuse indexed content | `axon query`, `axon ask`, `axon retrieve`, `axon sources` |

Choose an explicit workflow output directory and pass it with `--output-dir` or
`--output` for commands that write files. Do not treat repo-local `.axon/` paths
as Axon's internal data directory.

For finished knowledge-base deliverables, run async Axon commands in blocking mode: `axon crawl ... --wait true`, `axon ingest ... --wait true`, and `axon embed ... --wait true`. If you intentionally enqueue instead, report the job ID and the exact status command to run next.

## Parallel Work

If appropriate, use sub-agents or equivalent parallel task runners:

- one docs section per researcher
- official docs, tutorials, community discussions, and references by source type
- source scraping vs chunk generation vs manifest generation

## Output Modes

- Reference: generated markdown files, `index.md`, and `sources.json`.
- RAG: scraped markdown/HTML/JSON files, embedded Qdrant collection, `sources`,
  `retrieve`, and optional generated manifest files.
- Training: scraped source files plus optional agent-generated JSONL/metadata.
- Docs corpus: curated markdown artifacts, source index, and table of contents.

## Final Deliverable

```markdown
# Knowledge Base: [Source]

## Summary
[What was collected and why]

## Output Structure
[Files/directories created]

## Coverage
[Sections, source types, counts]

## Usage Notes
[How to use in RAG, docs, training, or agent context]

## Sources
[URLs collected]

## Rerun Inputs
workflow: knowledge-base
source: [url/topic]
goal: [reference/rag/train/docs]
depth: [quick/thorough/exhaustive]
output_dir: [explicit path]
```

## Quality Bar

- Preserve code examples and formatting.
- Remove boilerplate navigation where possible.
- Include source URLs in frontmatter or metadata.
