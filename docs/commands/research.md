# axon research
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Web research pipeline: Tavily search plus one ACP-backed synthesis call over returned snippets. Runs synchronously and prints extracted source previews plus a synthesized summary.

## Synopsis

```bash
axon research <query> [FLAGS]
axon research --query "<query>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<query>` | Research query text (or use `--query`) |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `TAVILY_API_KEY` | Tavily API key for source discovery. |
| `AXON_ACP_ADAPTER_CMD` | ACP adapter command (e.g. `codex`) used for synthesis. |
| `OPENAI_BASE_URL` | OpenAI-compatible base URL passed to the ACP adapter (e.g. `http://host/v1`). |
| `OPENAI_MODEL` | Model name passed to the ACP adapter for synthesis. |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional words). |
| `--limit <n>` | `10` | Maximum Tavily results processed. |
| `--search-time-range <range>` | — | Filter Tavily results by time range: `day`, `week`, `month`, `year`. |
| `--research-depth <n>` | — | Crawl depth limit for the research pass. |
| `--openai-base-url <url>` | env/default | Override LLM base URL passed to ACP adapter. |
| `--openai-model <name>` | env/default | Override LLM model name passed to ACP adapter. |

## Examples

```bash
# Basic research
axon research "Rust async cancellation patterns"

# Use --query and limit
axon research --query "Qdrant HNSW tuning" --limit 5

# Override LLM endpoint
axon research "Spider.rs rendering tradeoffs" --openai-base-url http://localhost:11434/v1 --openai-model llama3
```

## Pipeline

1. Tavily search fetches ranked results.
2. Each result contributes URL, title, and snippet as extracted evidence.
3. A single LLM completion synthesizes those snippets into a summary.

## Behavior Notes

- Both `TAVILY_API_KEY` and `AXON_ACP_ADAPTER_CMD` are validated at startup; the command errors immediately if either is missing or empty.
- `--search-time-range` is applied to the Tavily search step before synthesis.
- With `--json`, output is strict JSON on stdout.
- `research` does not enqueue jobs and does not auto-embed results into Qdrant.
