# axon reddit (removed — use `axon ingest`)

Last Modified: 2026-03-09

> **This command has been replaced.** Use [`axon ingest`](ingest.md) instead.
>
> `axon ingest` auto-detects the source type. Reddit subreddit prefixes (`r/name`) and URLs are recognized automatically.

> For implementation details and troubleshooting see [`docs/guides/ingest/reddit.md`](../../guides/ingest/reddit.md).

## Synopsis

```bash
axon ingest <TARGET> [FLAGS]
axon ingest <SUBCOMMAND> [ARGS]
```

Replace `axon reddit` with `axon ingest` — flags and behavior are identical.

## Arguments

| Argument | Description |
|----------|-------------|
| `<TARGET>` | Subreddit prefix (`r/name`), full Reddit URL, or thread URL |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--sort <sort>` | `hot` | Post sort order: `hot`, `top`, `new`, `rising` |
| `--time <range>` | `day` | Time range for `top` sort: `hour`, `day`, `week`, `month`, `year`, `all` |
| `--max-posts <n>` | `25` | Maximum posts to fetch (0 = unlimited) |
| `--min-score <n>` | `0` | Minimum score threshold for posts and comments |
| `--depth <n>` | `2` | Comment traversal depth |
| `--scrape-links` | off | Scrape content of linked URLs in link posts. Presence flag — include to enable. |
| `--wait <bool>` | `false` | Block until ingestion completes |
| `--collection <name>` | `axon` | Target Qdrant collection |
| `--json` | `false` | Machine-readable output |

## Job Subcommands

```bash
axon ingest status <job_id>   # show one ingest job
axon ingest cancel <job_id>   # cancel a pending/running job
axon ingest errors <job_id>   # show job error text
axon ingest list              # list recent ingest jobs (last 50)
axon ingest cleanup           # remove failed/canceled + old completed jobs
axon ingest clear             # delete all ingest jobs and purge the queue
axon ingest recover           # reclaim stale/interrupted jobs
axon ingest worker            # run ingest worker inline (blocking)
```

## Required Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `REDDIT_CLIENT_ID` | Yes | OAuth2 app client ID |
| `REDDIT_CLIENT_SECRET` | Yes | OAuth2 app client secret |

## Examples

```bash
# Subreddit — prefix form
axon ingest r/unraid

# Subreddit — with sort and time range
axon ingest r/rust --sort top --time week --wait true

# Full Reddit URL
axon ingest "https://www.reddit.com/r/homelab/" --wait true

# Thread URL
axon ingest "https://www.reddit.com/r/unraid/comments/abc123/title/" --wait true

# Job control
axon ingest list
axon ingest status 550e8400-e29b-41d4-a716-446655440000
axon ingest cancel 550e8400-e29b-41d4-a716-446655440000
```

## Migration

```bash
# Before
axon reddit r/unraid
axon reddit r/unraid --sort top --time week --wait true

# After
axon ingest r/unraid
axon ingest r/unraid --sort top --time week --wait true
```
