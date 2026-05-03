# axon ingest
Last Modified: 2026-03-25

Ingest external sources (GitHub, Reddit, YouTube) into Qdrant. Source type is auto-detected from the target — no need to choose the right command.

> For implementation details and troubleshooting see [`docs/ingest/ingest.md`](../ingest/ingest.md).
>
> Per-source deep-dives: [`docs/ingest/github.md`](../ingest/github.md) · [`docs/ingest/reddit.md`](../ingest/reddit.md) · [`docs/ingest/youtube.md`](../ingest/youtube.md)

## Synopsis

```bash
axon ingest <TARGET> [FLAGS]
axon ingest <SUBCOMMAND> [ARGS]
```

## Auto-detection Rules

The source type is inferred from `<TARGET>` in this order:

| Input pattern | Detected as |
|--------------|-------------|
| `r/subreddit` or `reddit.com/*` | Reddit |
| `@handle` | YouTube (expanded to `youtube.com/@handle`) |
| `youtube.com/*`, `youtu.be/*` | YouTube |
| Bare 11-character video ID | YouTube |
| `github.com/owner/repo` | GitHub |
| `owner/repo` slug | GitHub |

## Arguments

| Argument | Description |
|----------|-------------|
| `<TARGET>` | GitHub slug (`owner/repo`), YouTube URL / `@handle`, or Reddit subreddit (`r/name`) or URL |

## Required Environment Variables

| Variable | Required for | Description |
|----------|-------------|-------------|
| `GITHUB_TOKEN` | GitHub (optional) | Raises API rate limit from 60 to 5000 req/hr |
| `REDDIT_CLIENT_ID` | Reddit | OAuth2 app credentials |
| `REDDIT_CLIENT_SECRET` | Reddit | OAuth2 app credentials |

## Prerequisites

### External dependencies

| Dependency | Required for | Install |
|-----------|-------------|---------|
| `yt-dlp` | YouTube targets | `pip install yt-dlp` or `brew install yt-dlp` or `pipx install yt-dlp` |

`yt-dlp` must be on `PATH` before running any `axon ingest` command with a YouTube target. All other targets have no external binary requirements.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | Block until ingestion completes; otherwise enqueue async job. |
| `--collection <name>` | `cortex` | Target Qdrant collection. |
| `--json` | `false` | Machine-readable output. |

Lite mode preserves fire-and-forget semantics for `--wait false`: `ingest` enqueues the job and exits without draining unrelated ingest rows. Use `--wait true` to run ingestion synchronously and block until it finishes.

### GitHub-specific flags

| Flag | Default | Description |
|------|---------|-------------|
| `--no-source` | `false` | Skip source-code file indexing. Source code is included by default with tree-sitter AST-aware chunking for supported languages (Rust, Python, JavaScript, TypeScript, Go, Bash). |
| `--include-source` | `false` | Explicitly include source code files (redundant when source is already on by default; useful to make intent explicit). |
| `--max-issues <n>` | `100` | Maximum issues to fetch per repository (0 = unlimited). |
| `--max-prs <n>` | `100` | Maximum pull requests to fetch per repository (0 = unlimited). |

### Reddit-specific flags

| Flag | Default | Description |
|------|---------|-------------|
| `--sort <sort>` | `hot` | Post sort order: `hot`, `top`, `new`, `rising`. |
| `--time <range>` | `day` | Time range for `top` sort: `hour`, `day`, `week`, `month`, `year`, `all`. |
| `--max-posts <n>` | `25` | Maximum posts to fetch (0 = unlimited). |
| `--min-score <n>` | `0` | Minimum score threshold for posts and comments. |
| `--depth <n>` | `2` | Comment traversal depth. |
| `--scrape-links` | off | Scrape content of linked URLs in link posts. Presence flag — include to enable. |

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

## Examples

```bash
# GitHub: slug form (auto-detected)
axon ingest rust-lang/rust

# GitHub: URL form (auto-detected)
axon ingest https://github.com/anthropics/claude-code --wait true

# GitHub: source code is included by default (AST-aware chunking)
axon ingest tokio-rs/tokio --wait true

# GitHub: skip source code, ingest only docs/issues/PRs/wiki
axon ingest tokio-rs/tokio --no-source --wait true

# YouTube: video URL (auto-detected)
axon ingest "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true

# YouTube: playlist URL (enumerates videos and ingests each transcript)
axon ingest "https://www.youtube.com/playlist?list=PL1234567890abcdef" --wait true

# YouTube: channel @handle (auto-expanded to full URL, then enumerated)
axon ingest @SpaceinvaderOne

# YouTube: bare 11-character video ID
axon ingest dQw4w9WgXcQ --wait true

# Reddit: subreddit prefix form (auto-detected)
axon ingest r/unraid --sort top --time week

# Reddit: full URL (auto-detected)
axon ingest "https://www.reddit.com/r/rust/" --wait true

# Job control
axon ingest list
axon ingest status 550e8400-e29b-41d4-a716-446655440000
axon ingest cancel 550e8400-e29b-41d4-a716-446655440000
axon ingest recover
axon ingest clear --yes
```

## Notes

- Reddit-specific flags (`--sort`, `--time`, etc.) are silently ignored for GitHub and YouTube targets.
- `--no-source` is silently ignored for Reddit and YouTube targets.
- `axon sessions` is not routed through `axon ingest` — sessions take no URL/target and have format-specific flags.
