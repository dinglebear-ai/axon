# axon research
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Web research pipeline: Tavily search plus one Gemini headless synthesis call over returned snippets. Runs synchronously and prints extracted source previews plus a synthesized summary.

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
| `TAVILY_API_KEY` | Tavily API key for source discovery. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for synthesis. |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional words). |
| `--limit <n>` | `10` | Maximum Tavily results processed. |
| `--search-time-range <range>` | — | Filter Tavily results by time range: `day`, `week`, `month`, `year`. |
| `--research-depth <n>` | — | Number of sources the LLM synthesizes over. When set, it overrides `--limit`; when unset, falls back to `--limit` (default `10`). Capped with `--offset` at 100 (the Tavily window). |

The Gemini model used for synthesis is controlled by the
`AXON_HEADLESS_GEMINI_MODEL` env var (see [`docs/guides/configuration.md`](../../guides/configuration.md)).
The legacy `--openai-model` flag and `OPENAI_*` env vars were removed in 3.0.0.

## Examples

```bash
# Basic research
axon research "Rust async cancellation patterns"

# Use --query and limit
axon research --query "Qdrant HNSW tuning" --limit 5

# Override the Gemini model via env var
AXON_HEADLESS_GEMINI_MODEL=gemini-3.1-pro-preview \
  axon research "Spider.rs rendering tradeoffs"
```

## Pipeline

1. Tavily search fetches ranked results.
2. Each result contributes URL, title, and snippet as extracted evidence.
3. A single LLM completion synthesizes those snippets into a summary.

## Behavior Notes

- `TAVILY_API_KEY` is validated at startup; Gemini headless is used for synthesis.
- `--search-time-range` is applied to the Tavily search step before synthesis.
- The synthesis prompt asks for plain text, not JSON. The service still accepts legacy `{"summary":"..."}` model responses and unwraps the `summary` field for compatibility.
- With `--json`, stdout is strict command JSON. The `summary` field inside that payload is a string containing the plain-text synthesis.
- `research` does not enqueue jobs and does not auto-embed results into Qdrant.
- Progress logs redact the full user query by default and identify it by length/hash. Set `AXON_LOG_FULL_QUERIES=1` or `AXON_LOG_LEVEL=debug` only when full-query logging is intentional.
