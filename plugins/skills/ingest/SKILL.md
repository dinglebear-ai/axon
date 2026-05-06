---
name: ingest
description: Use when the user wants to index a GitHub repository, ingest a Reddit subreddit or thread, index a YouTube video or playlist, or import past Claude/Codex/Gemini session transcripts into axon. Triggers on "ingest this repo", "index this GitHub repo", "add this Reddit thread", "ingest subreddit", "index YouTube video", "import my sessions", "ingest GitHub", "index r/", "add this repo to axon". Also use when the user wants to make source code searchable via RAG.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-ingest

Ingests external sources — GitHub repos, Reddit, YouTube, and AI sessions — into Qdrant.

## GitHub

```json
{ "action": "ingest", "source_type": "github", "target": "owner/repo" }
```

Options: `include_source` (default `true`, indexes code with tree-sitter AST chunking), `max_issues`, `max_prs` (default 100; `0` = unlimited).

## Reddit

```json
{ "action": "ingest", "source_type": "reddit", "target": "r/rust" }
{ "action": "ingest", "source_type": "reddit", "target": "https://reddit.com/r/rust/comments/abc123/..." }
```

## YouTube

```json
{ "action": "ingest", "source_type": "youtube", "target": "https://youtube.com/watch?v=abc" }
```

Works on individual videos, playlists, and channels. Indexes transcripts/captions.

## AI sessions

```json
{
  "action": "ingest",
  "source_type": "sessions",
  "sessions": { "claude": true, "codex": true, "gemini": true, "project": "axon" }
}
```

## Lifecycle subactions

```json
{ "action": "ingest", "subaction": "status",  "job_id": "<uuid>" }
{ "action": "ingest", "subaction": "cancel",  "job_id": "<uuid>" }
{ "action": "ingest", "subaction": "list",    "limit": 20 }
```

## CLI fallback

```bash
axon ingest owner/repo
axon ingest r/rust --sort hot --max-posts 200
axon ingest https://youtube.com/watch?v=abc
```

## Credentials

- **GitHub**: `GITHUB_TOKEN` (optional, raises rate limits)
- **Reddit**: `REDDIT_CLIENT_ID` + `REDDIT_CLIENT_SECRET` (required)

Run `{ "action": "doctor" }` to verify credentials.
