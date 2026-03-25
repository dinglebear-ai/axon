# axon search
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Web search via Tavily. Returns ranked results (title, URL, snippet) and runs synchronously.

## Synopsis

```bash
axon search <query> [FLAGS]
axon search --query "<query>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<query>` | Search query text (or use `--query`) |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `TAVILY_API_KEY` | Tavily API key used by `spider_agent` search client. |

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Query text (alternative to positional words). |
| `--limit <n>` | `10` | Number of results to print. |
| `--search-time-range <range>` | — | Optional Tavily time filter: `day`, `week`, `month`, `year`. Invalid values are rejected by clap at parse time. |

## Examples

```bash
# Positional query
axon search "rust async channels"

# --query form
axon search --query "qdrant indexing best practices"

# Limit results + time range
axon search "tokio task cancellation" --limit 5 --search-time-range month
```

## Output

`search` prints:
- Numbered result position
- Title
- URL
- Snippet (if present)

## Behavior Notes

- `search` is synchronous and does not use the AMQP job queue.
- `--wait` has no effect for this command.
- With `--json`, output is strict JSON on stdout.
- `search` does not enqueue crawl jobs.
