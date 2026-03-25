# axon evaluate
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 23:05:00 | 03/03/2026 EST

Evaluate RAG quality versus a baseline. The command generates:
1) RAG answer (with retrieved context)
2) baseline answer (no retrieved context)
3) judge analysis comparing both answers against retrieved reference material.

## Synopsis

```bash
axon evaluate <question> [FLAGS]
axon evaluate --query "<question>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<question>` | Evaluation question (positional, or via `--query`). |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `AXON_PG_URL` | Required by global config parsing (all commands). |
| `AXON_REDIS_URL` | Required by global config parsing (all commands). |
| `AXON_AMQP_URL` | Required by global config parsing (all commands). |
| `TEI_URL` | TEI embeddings base URL (retrieval and judge reference). |
| `QDRANT_URL` | Qdrant base URL. |
| `AXON_ACP_ADAPTER_CMD` | ACP adapter command (e.g. `codex`). Required for all LLM calls (RAG, baseline, judge). |
| `OPENAI_MODEL` | Model name passed to the ACP adapter for all evaluate LLM calls. |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Question text (alternative to positional argument). |
| `--collection <name>` | `cortex` | Qdrant collection to retrieve from. |

Note: `evaluate` runs synchronously and does not enqueue jobs. Output is always JSON.

## Examples

```bash
# Basic evaluate run (always outputs JSON)
axon evaluate "How does auto-switch choose Chrome fallback?"

# Using --query
axon evaluate --query "What does AXON_DOMAINS_DETAILED change?"

# Pipe to jq for readable output
axon evaluate "How does ask citation gating work?" | jq .
```

## Output

Output is always a pretty-printed JSON object containing:
- `rag_answer`
- `baseline_answer`
- `analysis_answer`
- `crawl_suggestions` (present when judge scoring indicates RAG underperformed baseline)
- `crawl_enqueue_outcomes` (url + job_id or enqueue error, when suggestions are generated)
- `timing_ms` (retrieval/context/rag_llm/baseline_llm/research/judge/total)

## Notes

- Output is always JSON — there is no human-readable text mode for this command.
- If streaming fails for any LLM phase, evaluate falls back to non-streaming for that phase.
- Judge reference retrieval is best-effort; evaluate continues even if reference gathering fails.
- When judge scoring indicates RAG underperformed baseline, suggested crawl sources are auto-enqueued as crawl jobs immediately after generation.
