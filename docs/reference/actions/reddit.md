# axon reddit (removed — use `axon <source>`)

Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon <source>` |
| REST | `POST /v1/sources` |
| MCP | `{ "action": "source" }` |
| Service | `services::source::* via SourceRequest` |

Parity notes: Compatibility source page. Use the unified source action for CLI, REST, and MCP.
<!-- END GENERATED ACTION SURFACES -->


> **This command has been replaced.** Use the unified source command instead.
>
> `axon <source>` auto-detects the source type. Reddit subreddit prefixes
> (`r/name`) and URLs are recognized automatically.

> For implementation details and troubleshooting see [`docs/guides/ingest/reddit.md`](../../guides/ingest/reddit.md).

## Synopsis

```bash
axon <TARGET> [FLAGS]
axon jobs <SUBCOMMAND> [ARGS]
```

Replace `axon reddit` with `axon <source>`; inspect async work with `axon jobs`.

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
| `--wait <bool>` | `false` | Block until indexing completes |
| `--collection <name>` | `axon` | Target Qdrant collection |
| `--json` | `false` | Machine-readable output |

## Job Subcommands

```bash
axon jobs status <job_id>   # show one source job
axon jobs cancel <job_id>   # cancel a pending/running job
axon jobs list              # list recent jobs
```

## Required Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `REDDIT_CLIENT_ID` | Yes | OAuth2 app client ID |
| `REDDIT_CLIENT_SECRET` | Yes | OAuth2 app client secret |

## Examples

```bash
# Subreddit — prefix form
axon r/unraid

# Subreddit — with sort and time range
axon r/rust --sort top --time week --wait true

# Full Reddit URL
axon "https://www.reddit.com/r/homelab/" --wait true

# Thread URL
axon "https://www.reddit.com/r/unraid/comments/abc123/title/" --wait true

# Job control
axon jobs list
axon jobs status 550e8400-e29b-41d4-a716-446655440000
axon jobs cancel 550e8400-e29b-41d4-a716-446655440000
```

## Migration

```bash
# Before
axon reddit r/unraid
axon reddit r/unraid --sort top --time week --wait true

# After
axon r/unraid
axon r/unraid --sort top --time week --wait true
```
