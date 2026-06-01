# axon evaluate
Last Modified: 2026-06-01

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

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TEI_URL` | TEI embeddings base URL (retrieval and judge reference). |
| `QDRANT_URL` | Qdrant base URL. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for all evaluate LLM calls. |

`evaluate` uses Qdrant + TEI retrieval and Gemini headless judging.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Question text (alternative to positional argument). |
| `--diagnostics` | `false` | Print retrieval diagnostics alongside the evaluation. |
| `--responses-mode <mode>` | `side-by-side` | Human-output rendering for `rag_answer` vs `baseline_answer`: `inline`, `side-by-side`, or `events`. |
| `--retrieval-ab` | `false` | Replace the no-context baseline with a second RAG run that has hybrid retrieval disabled. The judge then compares hybrid-RAG vs dense-only-RAG. |
| `--collection <name>` | `axon` | Qdrant collection to retrieve from. Also settable via `AXON_COLLECTION`. |
| `--since <date>` | — | Filter retrieved context to content indexed on or after this date. Accepts `7d`, `30d`, `1w`, `YYYY-MM-DD`, or RFC3339. |
| `--before <date>` | — | Filter retrieved context to content indexed on or before this date. Same formats as `--since`. |
| `--json` | `false` | Machine-readable JSON output (overrides `--responses-mode`). |

Note: `evaluate` runs synchronously and does not enqueue jobs.

## Examples

```bash
# Default human output (side-by-side rendering)
axon evaluate "How does auto-switch choose Chrome fallback?"

# Using --query
axon evaluate --query "What does AXON_DOMAINS_DETAILED change?"

# Machine-readable JSON
axon evaluate "How does ask citation gating work?" --json | jq .

# Hybrid vs dense-only RAG comparison
axon evaluate "tokio cancellation patterns" --retrieval-ab
```

## Output

With `--json`, output is a pretty-printed JSON object containing:
- `rag_answer`
- `baseline_answer`
- `analysis_answer`
- `scores` (`status`, per-axis `axes`, totals, and `winner`; `parse_failed` is explicit)
- `crawl_suggestions` (present when judge scoring indicates RAG underperformed baseline)
- `crawl_enqueue_outcomes` (url + job_id or enqueue error, when suggestions are generated)
- `timing_ms` (retrieval/context/rag_llm/baseline_llm/research/judge/total)

## Notes

- Without `--json`, evaluate prints a human-readable answer comparison whose layout is controlled by `--responses-mode` (`side-by-side` by default; `inline` for stacked output; `events` for a single `evaluate_complete` JSON event line).
- If streaming fails for any LLM phase, evaluate falls back to non-streaming for that phase.
- Judge reference retrieval is best-effort; evaluate continues even if reference gathering fails.
- When judge scoring indicates RAG underperformed baseline, suggested crawl sources are auto-enqueued as crawl jobs immediately after generation.
