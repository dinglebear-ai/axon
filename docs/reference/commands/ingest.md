# axon ingest
Last Modified: 2026-03-25

Ingest external sources (GitHub, GitLab, Gitea/Forgejo, generic Git, Reddit, YouTube) into Qdrant. Source type is auto-detected from the target where possible.

> For implementation details and troubleshooting see [`docs/guides/ingest/ingest.md`](../../guides/ingest/ingest.md).
>
> Per-source deep-dives: [`docs/guides/ingest/github.md`](../../guides/ingest/github.md) · [`docs/guides/ingest/gitlab.md`](../../guides/ingest/gitlab.md) · [`docs/guides/ingest/reddit.md`](../../guides/ingest/reddit.md) · [`docs/guides/ingest/youtube.md`](../../guides/ingest/youtube.md)

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
| `gitlab.com/group/project` or `gitlab.com/group/subgroup/project` | GitLab |
| `gitlab:<host>/<group>/<project>` | GitLab |
| `gitea.com/owner/repo` or `codeberg.org/owner/repo` | Gitea/Forgejo |
| `gitea:<host>/<owner>/<repo>` or `forgejo:<host>/<owner>/<repo>` | Gitea/Forgejo |
| `git:https://host/path/repo.git` | Generic Git HTTPS clone |
| `github.com/owner/repo` | GitHub |
| `owner/repo` slug | GitHub |

## Arguments

| Argument | Description |
|----------|-------------|
| `<TARGET>` | GitHub slug (`owner/repo`), GitLab URL or `gitlab:` target, Gitea/Forgejo URL or prefix target, `git:` HTTPS clone URL, YouTube URL / `@handle`, or Reddit subreddit (`r/name`) or URL |

## Required Environment Variables

| Variable | Required for | Description |
|----------|-------------|-------------|
| `GITHUB_TOKEN` | GitHub (optional) | Raises API rate limit from 60 to 5000 req/hr |
| `GITLAB_TOKEN` | GitLab (optional) | Authenticates private projects and raises API limits |
| `GITEA_TOKEN` | Gitea/Forgejo (optional) | Authenticates API requests to Gitea-compatible servers |
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
| `--collection <name>` | `axon` | Target Qdrant collection. |
| `--json` | `false` | Machine-readable output. |

With `--wait false`, `ingest` writes a SQLite job row and exits without draining
unrelated ingest rows. Use `--wait true` to run ingestion synchronously and block
until it finishes.

### GitHub/GitLab/Gitea flags

| Flag | Default | Description |
|------|---------|-------------|
| `--no-source` | `false` | Skip source-code file indexing. Source code is included by default. Applies to all Git providers (GitHub, GitLab, Gitea/Forgejo, generic Git). |
| `--include-source` | `false` | **No-op.** Source code is already included by default; this flag is accepted for backward compatibility but changes nothing. Use `--no-source` to opt out. |
| `--max-issues <n>` | `100` | Maximum issues to fetch per repository/project (0 = unlimited). |
| `--max-prs <n>` | `100` | Maximum pull requests or merge requests to fetch per repository/project (0 = unlimited). Applies to GitHub pull requests, GitLab merge requests, and Gitea/Forgejo pull requests. |

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

# Enqueue through the canonical server
AXON_SERVER_URL=http://127.0.0.1:8001 axon ingest rust-lang/rust --json
```

## Notes

- In server mode (`AXON_SERVER_URL`), ingest submit and lifecycle subcommands call `axon serve`; `--wait true` polls server job state and does not spawn host-local workers. Use `--local` to force local execution.
- Reddit-specific flags (`--sort`, `--time`, etc.) are silently ignored for GitHub and YouTube targets.
- `--no-source` is silently ignored for Reddit and YouTube targets.
- `axon sessions` is not routed through `axon ingest` — sessions take no URL/target and have format-specific flags.
