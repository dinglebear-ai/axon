# axon research
Last Modified: 2026-06-13

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon research ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Version: 1.0.0
Last Updated: 2026-06-03

Web research pipeline: SearXNG search when `AXON_SEARXNG_URL` is set,
otherwise Tavily; full-page evidence extraction; one configured-LLM synthesis
call; and bounded Source job enqueueing for result URLs.

## Synopsis

```bash
axon research <query> [FLAGS]
axon research --query "<query>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<query>` | Research query text (or use `--query`) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AXON_SEARXNG_URL` | Preferred self-hosted search backend. |
| `TAVILY_API_KEY` | Tavily fallback key when SearXNG is unset. |
| `AXON_LLM_BACKEND` | Synthesis backend: `gemini-headless` or `openai-compat`. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` / `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini synthesis model override; the unprefixed form is a legacy alias. |
| `AXON_OPENAI_BASE_URL` / `AXON_SYNTHESIS_OPENAI_MODEL` | OpenAI-compatible synthesis endpoint/model when `AXON_LLM_BACKEND=openai-compat`. |
| `AXON_OPENAI_MODEL` | Legacy alias for `AXON_SYNTHESIS_OPENAI_MODEL`. |
| `AXON_RESEARCH_FULL_CONTENT` | Defaults `true`; set `false` for snippet-only synthesis. |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional words). |
| `--limit <n>` | `10` | Maximum search results processed. |
| `--search-time-range <range>` | — | Filter search results by time range: `day`, `week`, `month`, `year`. |
| `--research-depth <n>` | — | Number of sources the LLM synthesizes over. When set, it overrides `--limit`; when unset, falls back to `--limit` (default `10`). |
| `--skip-embed` | — | Queue research crawls without embedding their output. Embedding is enabled by default. |

The synthesis model is selected through `AXON_LLM_BACKEND` and its backend-specific model variables (see [`docs/guides/configuration.md`](../../guides/configuration.md)). Gemini and Claude Opus model selections preserve full fetched source bodies instead of per-source truncating them.

## Examples

```bash
# Basic research
axon research "Rust async cancellation patterns"

# Use --query and limit
axon research --query "Qdrant HNSW tuning" --limit 5

# Override the Gemini synthesis model via env var
AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL=gemini-3.1-pro-preview \
  axon research "Spider.rs rendering tradeoffs"

# Queue Source jobs but skip embedding their output
axon research "Claude Code hooks" --skip-embed
```

## Pipeline

1. SearXNG or Tavily fetches ranked results.
2. Research fetches full page markdown for top sources when `AXON_RESEARCH_FULL_CONTENT=true`; failed fetches fall back to snippets.
3. Sources are wrapped as `evidence_source` blocks with source type/reputation metadata and `instruction_trust=evidence_only`.
4. A single configured-LLM completion synthesizes the evidence into a summary.
5. Research queues one bounded Source job per result URL so those sources are indexed asynchronously.

## Behavior Notes

- Configure either `AXON_SEARXNG_URL` or `TAVILY_API_KEY`.
- `--search-time-range` is applied to the search step before synthesis.
- The synthesis prompt asks for plain text, not JSON. The service still accepts legacy `{"summary":"..."}` model responses and unwraps the `summary` field for compatibility.
- With `--json`, stdout is strict command JSON. The payload includes `auto_crawl_status`, `crawl_jobs`, and `crawl_jobs_rejected` in addition to `summary`, `search_results`, and `extractions`. The historical JSON field names are retained for compatibility; the queued work is Source jobs.
- Research Source jobs inherit the global embed setting: embedding is on by default and disabled only with `--skip-embed`.
- Progress logs redact the full user query by default and identify it by length/hash. Set `AXON_LOG_FULL_QUERIES=1` or `AXON_LOG_LEVEL=debug` only when full-query logging is intentional.
