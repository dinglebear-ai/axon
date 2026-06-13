# src/ingest — Source Ingestion Handlers
Last Modified: 2026-06-09

Ingests external sources (GitHub, GitLab, Gitea/Forgejo, generic Git, Reddit, YouTube, AI sessions) into Qdrant.

## Module Layout

```
ingest/
├── classify.rs    # classify_target(): auto-detect IngestSource from raw user input
├── progress.rs    # Progress reporting helpers shared across ingest sources
├── subprocess.rs  # Subprocess launch helpers (used by youtube + git clone paths)
├── github.rs      # module root + orchestration
├── github/        # GitHub repo ingestion (code, issues, PRs, wiki)
│   ├── files.rs   # git clone --depth=1 + file traversal + embed_files()
│   ├── issues.rs  # octocrab paginated issues + PRs
│   ├── meta.rs    # GitHubPayloadParams unified builder → git_*/code_* fields per chunk
│   └── wiki.rs    # git clone --depth=1 subprocess; no wiki = Ok(0)
├── gitlab.rs      # module root + orchestration (gitlab.com + self-hosted)
├── gitlab/        # GitLab repo ingestion (metadata, files, issues, MRs, wiki)
│   ├── client.rs  # build_gitlab_client(), fetch_project(), fetch_paginated()
│   ├── embed.rs   # embed_metadata/issues/merge_requests/wiki + gitlab_payload() builder
│   ├── files.rs   # git clone --depth=1 + file traversal + embed_files()
│   └── types.rs   # GitLabTarget, parse_gitlab_target(), normalize_gitlab_target()
├── gitea.rs       # Gitea/Forgejo repo ingestion (metadata, files, issues, PRs)
├── generic_git.rs # explicit HTTPS git clone ingest for repository files
├── reddit.rs      # module root — ingest_reddit, ingest_subreddit, ingest_thread
├── reddit/        # Reddit ingest submodules
│   ├── client.rs  # OAuth2 access token + JSON fetch helper
│   ├── comments.rs # recursive comment traversal + CommentWithContext
│   ├── meta.rs    # build_reddit_post_extra_payload (reddit_* fields)
│   └── types.rs   # RedditTarget enum, classify_target, validate_subreddit
├── youtube.rs     # module root — extract_video_id, is_playlist_or_channel_url, enumerate_playlist_videos, ingest_youtube
├── youtube/       # YouTube ingest submodules
│   ├── meta.rs    # YoutubeVideoMeta struct, parse_youtube_info_json, build_youtube_extra_payload
│   └── vtt.rs     # parse_vtt_to_text + VTT tests
├── sessions.rs    # module root for the AI-session parsers
└── sessions/      # AI session export parsers
    ├── claude.rs
    ├── codex.rs
    └── gemini.rs
```

## Source-Specific Patterns

### GitLab (`gitlab.rs` + `gitlab/`)
- Uses raw `reqwest` for all API calls — no third-party GitLab SDK
- `GITLAB_TOKEN` is **optional** but strongly recommended — unauthenticated rate limit is much lower; token is sent as `PRIVATE-TOKEN` header
- Supports **gitlab.com and any self-hosted GitLab instance** — host is parsed from the URL; no hardcoded hostname
- **Target formats accepted by `parse_gitlab_target()`:**
  - Full HTTPS URL: `https://gitlab.com/group/project`
  - `gitlab:` prefix for self-hosted: `gitlab:gitlab.example.com/group/project.git`
  - Nested namespaces: `https://gitlab.com/group/subgroup/project/-/issues/1` (the `/-/…` suffix is stripped)
  - `.git` suffix is stripped; path segments after the `-` marker are discarded
  - Must have ≥2 path segments (host + at minimum `group/project`) — rejects bare group URLs
- **5 sequential phases** run by `ingest_gitlab()`, each independently error-tolerant:
  1. **metadata** — repo description, visibility, stars, forks
  2. **files** — `git clone --depth=1` subprocess; reuses `is_indexable_doc_path` and `is_indexable_source_path` from `src/ingest/github`; token injected as `Authorization: Basic base64("oauth2:<token>")`; requires `git` in PATH
  3. **issues** — paginated REST via `/projects/:id/issues`, sorted by `updated_at desc`; skipped if `issues_enabled == false`
  4. **merge_requests** — paginated REST via `/projects/:id/merge_requests`, sorted by `updated_at desc`; skipped if `merge_requests_enabled == false`
  5. **wiki** — fetched with `with_content=1` via `/projects/:id/wikis`; skipped if `wiki_enabled == false`
- **403/404 graceful degradation:** `is_missing_or_forbidden()` returns `Ok(0)` for any phase that 403s or 404s — private features just silently produce no chunks
- **Pagination:** `fetch_paginated()` in `client.rs` follows `x-next-page` response header, `per_page=100`; respects `max_items` cap (reuses `cfg.github_max_issues` / `cfg.github_max_prs`)
- **Payload schema:** `source_type = "gitlab"`. Every chunk carries `provider`, `host`, `namespace_path`, `project`, `content_kind` (`"repo_metadata"` / `"file"` / `"issue"` / `"merge_request"` / `"wiki"`), `default_branch`, `visibility`, `last_activity_at`, plus a `gitlab` nested object with type-specific fields
- **File planning** — `gitlab/files.rs` builds `SourceDocument::try_new_file(SourceOrigin::GitFile, ...)` values with provider metadata. The shared source-doc planner owns `chunk_file`/line metadata and emits one file-level `PreparedDoc` with per-chunk `chunk_extra`.

### Gitea/Forgejo (`gitea.rs`)
- Uses the Gitea-compatible REST API for repository metadata, issues, and pull requests.
- `GITEA_TOKEN` is optional for public repositories and is sent as `Authorization: token <token>` when present.
- Known public hosts (`gitea.com`, `codeberg.org`) auto-classify from URL; self-hosted instances should use `gitea:<host>/<owner>/<repo>` or `forgejo:<host>/<owner>/<repo>`.
- File ingest delegates to `ingest_git_repository` in `generic_git.rs`, which now builds `SourceDocument` values and lets the source-doc planner attach tree-sitter/prose chunk metadata. Preserves `source_type = "gitea"` and `provider = "gitea"` in Qdrant payloads.

### Generic Git (`generic_git.rs`)
- Explicit-only target form: `git:https://host/path/repo.git`.
- HTTPS-only by design; SSH and local filesystem paths are rejected.
- Indexes repository docs by default and source files when source inclusion is enabled. It does not call provider APIs, so it does not ingest issues, PRs, wiki pages, releases, or repository metadata beyond clone-derived file metadata.
- File ingest builds one `SourceDocument` per file with canonical git/code doc metadata. The source-doc planner performs tree-sitter/prose chunking and emits one file-level `PreparedDoc` with per-chunk `code_*`/`symbol_*` metadata.

### GitHub (`github/`)
- Uses `git clone --depth=1` subprocess for source files and wiki clone; `octocrab` for issues/PRs pagination
- `GITHUB_TOKEN` is **optional** but strongly recommended — unauthenticated rate limit is 60 req/hr; authenticated is 5000 req/hr
- Source code is **included by default** — disable with `--no-source`. Code files use **tree-sitter AST-aware chunking** (Rust, Python, JavaScript, TypeScript, Go, Bash); unsupported languages fall back to 2000-char prose chunking
- Files are ingested via `clone_repo()` (`git clone --depth=1` subprocess), then walked and embedded — requires `git` in PATH/container
- `wiki.rs` runs `git clone --depth=1` as a subprocess — requires `git` in PATH/container. Non-zero exit = no wiki = `Ok(0)` (not an error)
- **Metadata**: `github/meta.rs` builds canonical `git_*` and `code_*` payload fields via `GitHubPayloadParams` and `build_github_payload()`. GitHub no longer emits `gh_*` duplicates in payload schema v7. Includes repo-level (`git_repo_*`), file-level (`code_file_*`, `code_line_*`, `code_chunking_method`), symbol-level (`symbol_*`), and issue/PR-level (`git_number`, `git_state`, `git_author`, labels, merge/draft fields) metadata.
- **File classification**: `classify_file_type()` in `src/vector/ops/input/classify.rs` tags each file as `test`/`config`/`doc`/`source` — stored in `code_file_type`

### Reddit (`reddit.rs` + `reddit/`)
- Reddit OAuth2 **client credentials** flow (app-only, no user login)
- **Both** `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET` are **required** — command fails immediately if either is missing
- Fetches subreddit posts + recursive comments; depth configurable via `--depth`
- Rate limit: 100 req/min authenticated; uses `reqwest` directly (not spider)
- **Metadata**: `reddit/meta.rs` builds `reddit_*` fields (author, score, num_comments, upvote_ratio, subreddit, domain, is_video, distinguished, gilded, flair) merged into every post's Qdrant payload via `PreparedDoc.extra`

### YouTube (`youtube.rs` + `youtube/`)
- Invokes `yt-dlp` as a **subprocess** (not a library) — `yt-dlp` must be installed and on `$PATH`
- **Single video** (`ingest_youtube`): downloads `.vtt` + `.info.json` → `parse_vtt_to_text()` → embed transcript + description with full metadata payload
- **Playlist / channel** (`ingest_youtube_playlist`): `--flat-playlist` enumeration (capped at 500 videos via `MAX_PLAYLIST_VIDEOS`) → sequential `for` loop of per-video `ingest_youtube` calls → progress reported after each video; failed videos are logged as warnings and skipped (job continues)
- `is_playlist_or_channel_url()` detects `@handle`, `/c/`, `/channel/`, `/user/`, `?list=` — routes to playlist pipeline automatically
- `extract_video_id()` handles full URLs, short URLs (`youtu.be/`), path patterns, and bare IDs
- **Resume**: `completed_urls` in `result_json` JSONB lets a restarted job skip already-done videos
- **429 retry**: 3 attempts with 10s → 20s → 40s backoff per video; non-429 errors skip the video
- **Metadata**: `YoutubeVideoMeta` (in `youtube/meta.rs`) captures title, channel, channel_url, uploader_id, upload_date, duration, view_count, like_count, tags, categories — merged into every chunk's Qdrant payload via `PreparedDoc.extra`
- No API key needed; yt-dlp handles auth for publicly accessible videos

### Sessions (`sessions/`)
- Parses exported conversation files from Claude (`.jsonl`), Codex (`.jsonl`), and Gemini (`.json`)
- Each parser (`claude.rs`, `codex.rs`, `gemini.rs`) extracts message pairs into flat text chunks
- Called by `src/cli/commands/sessions.rs`; async submissions use the SQLite job runtime, while `--wait true` runs through the services ingest path with in-process workers

## Testing

```bash
cargo test ingest         # all ingest unit tests
cargo test classify       # classify_target() auto-detection
cargo test parse_vtt      # VTT subtitle parsing
cargo test extract_video  # YouTube video ID extraction
cargo test parse_github   # GitHub repo name/URL parsing
cargo test gitlab         # GitLab target parsing and classification
cargo test gitea          # Gitea/Forgejo target parsing and classification
cargo test generic_git    # generic HTTPS Git target parsing and classification
cargo test session        # session export format parsers
cargo test -- --nocapture # show parsed output
```

All ingest unit tests run without live services (pure logic: parsing, classification, ID extraction). Tests for `ingest_github`, `ingest_gitlab`, `ingest_gitea`, `ingest_generic_git`, `ingest_reddit`, `ingest_youtube` that hit real APIs require credentials set in env and/or running Qdrant + TEI.

## Embedding Pattern

All ingest sources use the unified `embed_prepared_docs` pipeline via planner-created `PreparedDoc` values. Ingest builders produce `SourceDocument` values or call source-doc helpers. Only the source-doc planner calls `chunk_file`, `chunk_markdown`, or `chunk_text`. `PreparedDoc` is post-chunk and embed-ready.

### Canonical Pattern

```rust
use crate::vector::ops::{
    SourceDocument, SourceOrigin, embed_prepared_docs, prepare_source_document,
};

let url = "https://github.com/rust-lang/rust/blob/main/src/lib.rs".to_string();
let source = SourceDocument::try_new_file(
    SourceOrigin::GitFile,
    url,
    "src/lib.rs".to_string(),
    "rs".to_string(),
    content,
    "github",
    Some("src/lib.rs".to_string()),
    Some(serde_json::json!({
        "provider": "github",
        "git_owner": "rust-lang",
        "git_repo": "rust",
        "git_content_kind": "file",
        "code_file_path": "src/lib.rs",
    })),
)?;
let doc = prepare_source_document(source).await?;
let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
```

### Pipeline behavior
1. Documents processed concurrently (`AXON_EMBED_DOC_CONCURRENCY`)
2. Per-doc TEI embedding with auto-split on 413 and retry on 429/503
3. **Upsert-first** — deterministic UUID v5 point IDs overwrite existing chunks
4. **Stale-tail cleanup** — orphan chunks with `chunk_index >= new_count` deleted after upsert
5. Individual doc failures are logged and skipped (reported via `EmbedSummary.docs_failed`)

### Chunking
- Ingest builders must not chunk before planning. Build `SourceDocument` or use `prepare_plain_text_source`.
- `SourceOrigin::GitFile` and `SourceOrigin::LocalFile` are the only origins that may use file chunking.
- Crawl manifests and scrape results always use markdown/plain-text planning, even when a URL ends in `.rs`.
- Provider metadata (`git_*`, `code_*`, `reddit_*`, `yt_*`, session fields) is passed as doc-level `extra`; planner-owned normalized fields (`chunk_content_kind`, `chunk_locator`, `source_range`, `chunking_fallback`, `code_chunk_source`) are generated by the planner only.

## ingest_jobs Schema
`axon_ingest_jobs` differs from other job tables:
- Uses `source_type TEXT` (`github`/`reddit`/`youtube`/`sessions`) + `target TEXT` (repo name, subreddit, video URL, session target)
- Does **NOT** have `url` or `urls_json` columns
- Ingest worker lifecycle is owned by the SQLite worker subsystem (`src/jobs/workers.rs`); the legacy `worker_lane.rs` was removed with the old queue runtime. `AXON_INGEST_LANES` is wired through config and clamped to 1-16.

## Known Gaps

| Gap | Status |
|-----|--------|
| YouTube age-restricted / private videos | `yt-dlp` exits non-zero; error is a per-video skip warning in playlist mode, job failure in single-video mode. No friendly message. |
| YouTube manual captions | Only `--write-auto-sub` is passed; `--write-subs` (manual captions) is not requested. Videos with manual but no auto-generated captions will fail. |
| GitHub file stream resilience | `flush_batch` errors are logged and counted (not propagated via `?`). A single TEI/Qdrant failure discards that batch and continues with remaining files. Batch timeout: 120s. |
| Ingest job hang detection | Per-job heartbeat (30s touch, `src/jobs/workers/heartbeat.rs`) + periodic watchdog (60s sweep, `src/jobs/workers.rs`) reclaim jobs whose `updated_at` exceeds `watchdog_stale_timeout_secs + watchdog_confirm_secs` (default 360s). Reclaimed rows are reset to `pending` (not `failed`). |

## yt-dlp Requirement

`yt-dlp` **must be installed and on `$PATH`**. The `youtube` command will fail at runtime with a cryptic process error if it's missing:
```
No such file or directory (os error 2)
```
Install: `pip install yt-dlp` or `brew install yt-dlp`. Verify: `yt-dlp --version`.

## Adding a New Ingest Source
1. Add parser in `src/ingest/<source>.rs`
2. Extend `classify_target()` in `src/ingest/classify.rs` to recognize the new source
3. Add a per-source variant in the relevant ingest service entry point (`src/services/ingest.rs`)
4. Add `source_type` variant handling in the SQLite ingest worker (`src/jobs/workers.rs` and the ingest payload schema)
5. Add env vars to `.env.example`
