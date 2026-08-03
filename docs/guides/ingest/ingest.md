---
title: "Ingest System"
created: 2026-03-09
updated: 2026-07-30
---

# Ingest System
Last Modified: 2026-03-10

> CLI reference (flags, subcommands, examples): [`docs/reference/actions/sources.md`](../../reference/actions/sources.md)

The unified source command ingests external sources — GitHub repositories,
GitLab projects, Gitea/Forgejo repositories, generic HTTPS Git repositories,
RSS/Atom/JSON feeds, Reddit subreddits/threads, YouTube videos/playlists/
channels, and AI session exports — into Qdrant. Source type is auto-detected
from the target argument where possible.

Use `axon <source>` or `axon source <source>` for all source-family ingestion.
The legacy `axon ingest <source>` entrypoint was removed in the #298 clean
break.

## Ingest Docs Index

| Doc | Scope |
|-----|-------|
| [`docs/guides/ingest/ingest.md`](ingest.md) | Shared source-job, dependency, and configuration model. |
| [`docs/guides/ingest/github.md`](github.md) | GitHub repository ingestion. |
| [`docs/guides/ingest/gitlab.md`](gitlab.md) | GitLab project ingestion. |
| Gitea/Forgejo, generic Git, RSS/Atom/JSON feeds | See `docs/reference/actions/sources.md` for target forms and shared flags. |
| [`docs/guides/ingest/reddit.md`](reddit.md) | Reddit subreddit and thread ingestion. |
| [`docs/guides/ingest/youtube.md`](youtube.md) | YouTube video, playlist, and channel ingestion. |
| [`docs/guides/ingest/sessions.md`](sessions.md) | AI session export ingestion. |

Command-only operational notes live in `docs/reference/actions/*.md`; do not
add one-sentence `docs/guides/ingest/` stubs for non-source commands.

## Durable state

Source operations are persisted in the unified SQLite `jobs` model with typed
fields such as `kind`, `intent`, `status`, `phase`, `request_json`, and
`error_json`, plus attempt, stage, event, heartbeat, artifact, reservation, and
configuration-snapshot records. See
[`docs/reference/job-lifecycle.md`](../../reference/job-lifecycle.md) and the
generated [database schema](../../reference/runtime/database-schema.md).

Source identity and publication state do not belong to `axon-jobs`. The
`axon-ledger` migrations own sources, generations, manifests, items, document
status, publication state, leases, and cleanup debt. One job id crosses the
source runner from acquisition through publication and cleanup.

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
