---
name: knowledge-base
description: Build or refresh a Qdrant-backed Axon knowledge base from web, local, git, registry, feed, Reddit, YouTube, or session sources.
---

# Axon Knowledge Base

Use Axon's unified source pipeline to turn URLs, repositories, local files,
feeds, communities, videos, package metadata, or AI sessions into an indexed
corpus with optional saved acquisition artifacts.

## Collection plan

| Need | Axon surface |
|---|---|
| Discover URLs only | `axon map <url>` |
| Capture one page | `axon scrape <url> --wait true` |
| Capture a site or docs section | `axon source <url> --scope site --wait true` |
| Index a local directory or repository | `axon source <path-or-url> --wait true` |
| Index Reddit, YouTube, feeds, packages, or sessions | `axon source <selector> --wait true` |
| Discover topic sources | `axon search` or `axon research` |
| Schedule freshness | `axon watch create <source> --every-seconds N` |
| Reuse indexed content | `axon query`, `axon ask`, `axon retrieve` |

Examples:

```bash
axon source https://docs.example.com   --scope site   --max-pages 200   --wait true   --output-dir .axon/docs

axon source https://github.com/org/repo --wait true
axon source /home/user/project --wait true
axon source r/rust --wait true
axon source https://youtube.com/watch?v=... --wait true
axon source pkg:crates/serde --wait true
axon source session:codex:/home/user/.codex/sessions/... --wait true
```

Embedding and publication are enabled by default. Use `--skip-embed` only for a
capture-only deliverable.

## MCP source examples

```json
{ "action": "source", "source": "https://docs.example.com", "scope": "site" }
{ "action": "source", "source": "https://github.com/org/repo" }
{ "action": "source", "source": "/home/user/project" }
```

Use CLI source flags when the workflow needs output files, crawl budgets,
render controls, WARC, selectors, custom headers, or automation scripts.

## Completion

Run source work with `--wait true` when the deliverable requires a finished
corpus. When intentionally detached, report the job ID and use the unified jobs
surface:

```bash
axon jobs get <job-id>
axon jobs events <job-id>
```

## Output modes

- **Reference**: curated Markdown/HTML/JSON acquisition output and source index
- **RAG**: committed vectors plus optional acquisition artifacts
- **Training**: collected source files plus separately produced training data
- **Docs corpus**: bounded, organized documentation capture with provenance

## Final deliverable

Report:

- sources and selectors used
- collection and output directory
- source job IDs and terminal status
- counts, warnings, and degraded stages
- how to query or ask against the resulting corpus
- the exact source or watch command needed to refresh it
