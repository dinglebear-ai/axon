# src/ingest — Source Ingestion Handlers
Last Modified: 2026-07-08

Phase 12 clean break (issue #298): the GitHub/GitLab/Gitea/generic-git/Reddit/
YouTube/RSS provider orchestration, clients, and embedders that used to live
here were deleted outright — not staged behind a compat shim. Only **AI
session-export ingest** still executes. `classify_target` and its pure
target-string parsers survive because `axon refresh` still needs to
reclassify previously-ingested origins (of any provider) from their stored
`seed_url`; the legacy per-family job runner
(`crates/axon-jobs/src/workers/runners/ingest.rs`) returns a clean error for
every non-session `IngestSource` instead of executing it.

## Module Layout

```
ingest/
├── classify.rs         # classify_target(): auto-detect IngestSource from raw user input
├── target_parse.rs      # pure target-string parsing/normalization (github/gitlab/gitea/
│                         # generic-git/reddit/youtube) — parsing only, no client/embed logic
├── progress.rs          # Progress reporting helpers shared across ingest sources
├── orchestrate.rs        # map_ingest_result/ingest_payload + session-export orchestration
│   └── sessions_prepared.rs  # prepared-sessions sidecar ingest
├── sessions.rs          # module root for the AI-session parsers (the only live provider)
└── sessions/            # AI session export parsers
    ├── claude.rs
    ├── codex.rs
    └── gemini.rs
```

## What's still live

Only `sessions.rs`/`sessions/` executes real ingestion. `orchestrate.rs`
exposes `ingest_sessions`/`ingest_sessions_with_progress` and
`ingest_sessions_prepared_with_progress`; both are called by
`crates/axon-jobs/src/workers/runners/ingest.rs::execute_ingest_source`
(`IngestSource::Sessions`/`IngestSource::PreparedSessions`) and by
`crates/axon-cli/src/commands/sessions.rs` for `--wait true` sync runs.

## What's classification-only (`target_parse.rs`)

`classify_target()` in `classify.rs` still routes raw target strings
(`owner/repo`, `gitlab.com/g/p`, `gitea:host/o/r`, `git:https://...`,
`r/subreddit`, a YouTube URL/handle, an RSS/Atom feed URL) to the matching
`axon_api::ingest::IngestSource` variant, and `target_parse.rs` holds the pure
string parsers/normalizers those variants need (`parse_github_repo`,
`normalize_gitlab_target`, `normalize_gitea_target`,
`normalize_generic_git_target`, `classify_reddit_target`,
`classify_youtube_target`, etc.) — no HTTP clients, no cloning, no embedding.
This exists solely so `axon refresh` can classify an origin it finds in
Qdrant's `seed_url` facet well enough to attempt re-enqueue; the resulting
job will fail at execution time for every source type except sessions.

## Adding a New Ingest Source

The #298 target replaces all of this with source-request-backed adapters —
see `docs/pipeline-unification/sources/new-source-contract.md`. Do not add a
new standalone provider ingest family here; if session-parity ingestion for a
new provider is needed, it belongs in the unified source pipeline
(`axon-services/src/*_source.rs`), not in this crate.
