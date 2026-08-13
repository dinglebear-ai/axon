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

All indexing goes through the one unified source command — `axon <source>`
(equivalently `axon source <source>`), narrowed with `--scope` when the family
default is wrong. There is no `axon crawl`, `axon ingest`, `axon embed`, or
`axon refresh`.

| Need | Axon surface |
|---|---|
| Discover URLs only | `axon map <url>` |
| Capture a docs site or section as artifacts | `axon <url> --scope site --output-dir <dir>` |
| Fetch selected pages | `axon scrape <url> --output-dir <dir>` |
| Discover topic sources | `axon search` or `axon research` |
| Index repos, feeds, Reddit, YouTube, packages, local paths | `axon <source>` |
| Index AI sessions | `axon sessions`, or `axon "session:<provider>:<path>"` |
| Refresh an existing source | re-run `axon <source>` — the ledger diffs manifests and only re-prepares what changed |
| Schedule freshness | `axon watch create "<source>" --every-seconds <N>` |
| Reuse indexed content | `axon query`, `axon ask`, `axon retrieve`, `axon sources` |

Choose an explicit workflow output directory and pass it with `--output-dir` or
`--output` for commands that write files. Do not treat repo-local `.axon/` paths
as Axon's internal data directory.

For finished knowledge-base deliverables, run indexing in blocking mode:
`axon <source> ... --wait true`. Without `--wait true` the CLI enqueues and
returns a job id — if you intentionally enqueue, report the job id and the exact
follow-up command (`axon jobs get <job-id>`).

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
