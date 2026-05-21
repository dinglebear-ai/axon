# Ingest System
Last Modified: 2026-03-10

> CLI reference (flags, subcommands, examples): [`docs/commands/ingest.md`](../commands/ingest.md)

The `axon ingest` command ingests external sources — GitHub repositories, GitLab projects, Gitea/Forgejo repositories, generic HTTPS Git repositories, Reddit subreddits/threads, and YouTube videos/playlists/channels — into Qdrant. Source type is auto-detected from the target argument where possible.

## Ingest Docs Index

| Doc | Scope |
|-----|-------|
| [`docs/ingest/ingest.md`](ingest.md) | Shared ingest job schema, dependencies, and environment variables. |
| [`docs/ingest/github.md`](github.md) | GitHub repository ingestion. |
| [`docs/ingest/gitlab.md`](gitlab.md) | GitLab project ingestion. |
| Gitea/Forgejo and generic Git | See `docs/commands/ingest.md` for target forms and shared flags. |
| [`docs/ingest/reddit.md`](reddit.md) | Reddit subreddit and thread ingestion. |
| [`docs/ingest/youtube.md`](youtube.md) | YouTube video, playlist, and channel ingestion. |
| [`docs/ingest/sessions.md`](sessions.md) | AI session export ingestion. |

Command-only operational notes live in `docs/commands/*.md`; do not add one-sentence `docs/ingest/` stubs for non-ingest commands.

## Storage Schema

Ingest jobs are persisted in the SQLite `axon_ingest_jobs` table. The table is created by migrations under `src/jobs/migrations/` and used through the SQLite job runtime.

| Column | Type | Description |
|--------|------|-------------|
| `id` | `TEXT` | Job identifier |
| `source_type` | `TEXT` | One of `github`, `gitlab`, `gitea`, `git`, `reddit`, `youtube` |
| `target` | `TEXT` | The original target string (slug, URL, handle, etc.) |
| `status` | `TEXT` | `pending`, `running`, `completed`, `failed`, or `canceled` |
| `config_json` | `TEXT` | Serialized job configuration (flags at submission time) |
| `result_json` | `TEXT` | Serialized result (chunk counts, errors, etc.) |
| `error_text` | `TEXT` | Human-readable error message on failure |
| `created_at` | `INTEGER` | Milliseconds since epoch when the job was enqueued |
| `updated_at` | `INTEGER` | Last status update / heartbeat |
| `started_at` | `INTEGER` | When a worker claimed the job |
| `finished_at` | `INTEGER` | When the job reached a terminal state |

Indexes on status/source fields speed up worker claim and list queries.

## External Dependencies

| Dependency | Required for | Notes |
|-----------|-------------|-------|
| `yt-dlp` | YouTube targets | Must be on `PATH`. Install: `pip install yt-dlp` or `brew install yt-dlp` or `pipx install yt-dlp` |

## Common Environment Variables

| Variable | Required for | Description |
|----------|-------------|-------------|
| `TEI_URL` | All targets | TEI embedding service endpoint |
| `AXON_COLLECTION` | All targets | Qdrant collection name (default: `axon`) |
| `GITHUB_TOKEN` | GitHub (optional) | Raises GitHub API rate limit from 60 to 5000 req/hr |
| `GITLAB_TOKEN` | GitLab (optional) | Authenticates private projects and raises API limits |
| `GITEA_TOKEN` | Gitea/Forgejo (optional) | Authenticates Gitea-compatible API requests |
| `REDDIT_CLIENT_ID` | Reddit | OAuth2 app client ID |
| `REDDIT_CLIENT_SECRET` | Reddit | OAuth2 app client secret |
