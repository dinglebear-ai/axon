# axon search
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Web search via Tavily. Returns ranked results (title, URL, snippet), then auto-enqueues one bounded crawl job per result URL so the hits are indexed into Qdrant. Runs synchronously.

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

In human mode, `search` prints:
- Numbered result position
- Title
- URL
- Snippet (if present)
- A summary of the auto-queued crawl jobs (and any rejected URLs)

With `--json`, the payload includes: `query`, `limit`, `offset`, `search_time_range`, `results`, `auto_crawl_status`, `crawl_jobs`, and `crawl_jobs_rejected`.

## Behavior Notes

- `search` runs synchronously (the Tavily search and crawl-job enqueue both happen inline).
- After returning results, `search` enqueues one bounded crawl job per result URL (`search_and_crawl` in `src/services/search_crawl.rs`). The crawl jobs themselves run asynchronously via the in-process worker pool.
- If results were found but no URLs could be queued for crawl, `search` exits with an error reporting the first rejection reason.
- `--wait` is not honored by `search` itself; the enqueued crawl jobs follow the normal async lifecycle (inspect them with `axon crawl status`/`list`).
- With `--json`, output is strict JSON on stdout.
