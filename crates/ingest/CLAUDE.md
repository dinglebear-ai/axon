# crates/ingest — Source Ingestion Handlers
Last Modified: 2026-03-10

Ingests external sources (GitHub, Reddit, YouTube, AI sessions) into Qdrant.

## Module Layout

```
ingest/
├── classify.rs    # classify_target(): auto-detect IngestSource from raw user input
├── github/        # GitHub repo ingestion (code, issues, PRs, wiki)
│   ├── files.rs   # file tree fetch + raw content via reqwest
│   ├── issues.rs  # octocrab paginated issues + PRs
│   ├── meta.rs    # GitHubPayloadParams unified builder → 31 gh_* fields per chunk
│   └── wiki.rs    # git clone --depth=1 subprocess; no wiki = Ok(0)
├── github.rs      # module root + orchestration
├── reddit.rs      # module root — ingest_reddit, ingest_subreddit, ingest_thread
├── reddit/        # Reddit ingest submodules
│   ├── client.rs  # OAuth2 access token + JSON fetch helper
│   ├── comments.rs # recursive comment traversal + CommentWithContext
│   ├── meta.rs    # build_reddit_post_extra_payload (reddit_* fields)
│   └── types.rs   # RedditTarget enum, classify_target, validate_subreddit
├── youtube/       # YouTube ingest submodules
│   ├── meta.rs    # YoutubeVideoMeta struct, parse_youtube_info_json, build_youtube_extra_payload
│   └── vtt.rs     # parse_vtt_to_text + VTT tests
├── youtube.rs     # module root — extract_video_id, is_playlist_or_channel_url,
│                  #   enumerate_playlist_videos, ingest_youtube
└── sessions/      # AI session export parsers
    ├── claude.rs
    ├── codex.rs
    └── gemini.rs
```

## Source-Specific Patterns

### GitHub (`github/`)
- Uses raw `reqwest` for file content fetching; `octocrab` for issues/PRs pagination
- `GITHUB_TOKEN` is **optional** but strongly recommended — unauthenticated rate limit is 60 req/hr; authenticated is 5000 req/hr
- Source code is **included by default** — disable with `--no-source`. Code files use **tree-sitter AST-aware chunking** (Rust, Python, JavaScript, TypeScript, Go, Bash); unsupported languages fall back to 2000-char prose chunking
- Files are fetched tree-first (one API call), then content per file concurrently via `buffer_unordered(16)` — can be slow without token on large repos
- `wiki.rs` runs `git clone --depth=1` as a subprocess — requires `git` in PATH/container. Non-zero exit = no wiki = `Ok(0)` (not an error)
- **Metadata**: `github/meta.rs` builds a **unified** 31-field `gh_*` payload via `GitHubPayloadParams` struct and `build_github_payload()`. All chunk types (file, issue, PR, wiki) share the same field schema — unused fields are null/default. Includes repo-level (owner, stars, forks, topics, pushed_at), file-level (path, language, file_type, is_test, chunking_method), and issue/PR-level (number, state, author, labels, merged_at, is_draft) fields.
- **File classification**: `classify_file_type()` in `crates/vector/ops/input/classify.rs` tags each file as `test`/`config`/`doc`/`source` — stored in `gh_file_type`

### Reddit (`reddit.rs` + `reddit/`)
- Reddit OAuth2 **client credentials** flow (app-only, no user login)
- **Both** `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET` are **required** — command fails immediately if either is missing
- Fetches subreddit posts + recursive comments; depth configurable via `--depth`
- Rate limit: 100 req/min authenticated; uses `reqwest` directly (not spider)
- **Metadata**: `reddit/meta.rs` builds `reddit_*` fields (author, score, num_comments, upvote_ratio, subreddit, domain, is_video, distinguished, gilded, flair) merged into every post's Qdrant payload via `PreparedDoc.extra`

### YouTube (`youtube.rs` + `youtube/`)
- Invokes `yt-dlp` as a **subprocess** (not a library) — `yt-dlp` must be installed and on `$PATH`
- **Single video** (`ingest_youtube`): downloads `.vtt` + `.info.json` → `parse_vtt_to_text()` → embed transcript + description with full metadata payload
- **Playlist / channel** (`ingest_youtube_playlist`): `--flat-playlist` enumeration → N=5 concurrent via `FuturesUnordered` → per-video `ingest_youtube` calls → progress + `completed_urls` persisted to DB on each completion
- `is_playlist_or_channel_url()` detects `@handle`, `/c/`, `/channel/`, `/user/`, `?list=` — routes to playlist pipeline automatically
- `extract_video_id()` handles full URLs, short URLs (`youtu.be/`), path patterns, and bare IDs
- **Resume**: `completed_urls` in `result_json` JSONB lets a restarted job skip already-done videos
- **429 retry**: 3 attempts with 10s → 20s → 40s backoff per video; non-429 errors skip the video
- **Metadata**: `YoutubeVideoMeta` (in `youtube/meta.rs`) captures title, channel, channel_url, uploader_id, upload_date, duration, view_count, like_count, tags, categories — merged into every chunk's Qdrant payload via `PreparedDoc.extra`
- No API key needed; yt-dlp handles auth for publicly accessible videos

### Sessions (`sessions/`)
- Parses exported conversation files from Claude (`.json`), Codex (`.md`), Gemini (`.json`)
- Each parser (`claude.rs`, `codex.rs`, `gemini.rs`) extracts message pairs → flat text chunks
- Called by `crates/cli/commands/sessions.rs` — synchronous (no AMQP), like `ask`/`query`

## Testing

```bash
cargo test ingest         # all ingest unit tests
cargo test classify       # classify_target() auto-detection (17 tests)
cargo test parse_vtt      # VTT subtitle parsing
cargo test extract_video  # YouTube video ID extraction
cargo test parse_github   # GitHub repo name/URL parsing
cargo test session        # session export format parsers
cargo test -- --nocapture # show parsed output
```

All ingest unit tests run without live services (pure logic: parsing, classification, ID extraction). Tests for `ingest_github`, `ingest_reddit`, `ingest_youtube` that hit real APIs require credentials set in env.

## Embedding Pattern

All ingest sources use the unified `embed_prepared_docs` pipeline via `PreparedDoc`. The legacy per-function API (`embed_text_with_metadata`, `embed_text_with_extra_payload`, `embed_code_with_metadata`) has been removed — there is now a single entry point for all embedding.

### Canonical Pattern

```rust
use crate::crates::vector::ops::tei::{PreparedDoc, embed_prepared_docs};
use crate::crates::vector::ops::input::chunk_text;       // prose: 2000-char with 200-char overlap
use crate::crates::vector::ops::input::code::chunk_code;  // tree-sitter AST-aware chunking

let doc = PreparedDoc {
    url: url.to_string(),
    domain,                          // extracted from URL host
    chunks,                          // pre-chunked via chunk_text() or chunk_code()
    source_type: "github".to_string(), // "github"/"reddit"/"youtube"/"sessions"/"refresh"/"embed"
    content_type: "markdown",        // or "text"
    title: Some("...".to_string()),  // optional
    extra: Some(serde_json::json!({  // source-specific gh_*/reddit_*/yt_* fields
        "gh_owner": "rust-lang",
    })),
};
let summary = embed_prepared_docs(cfg, vec![doc], None).await?;
```

### Pipeline behavior
1. Documents processed concurrently (`AXON_EMBED_DOC_CONCURRENCY`)
2. Per-doc TEI embedding with auto-split on 413 and retry on 429/503
3. **Upsert-first** — deterministic UUID v5 point IDs overwrite existing chunks
4. **Stale-tail cleanup** — orphan chunks with `chunk_index >= new_count` deleted after upsert
5. Individual doc failures are logged and skipped (reported via `EmbedSummary.docs_failed`)

### Chunking
- **Prose** (`chunk_text`): 2000-char chunks with 200-char overlap
- **Code** (`chunk_code`): tree-sitter AST-aware (Rust, Python, JS, TS, Go, Bash); falls back to `chunk_text` for unsupported extensions
- Callers must chunk content **before** building `PreparedDoc` — the pipeline expects pre-chunked `chunks: Vec<String>`

## ingest_jobs Schema
`axon_ingest_jobs` differs from other job tables:
- Uses `source_type TEXT` (`github`/`reddit`/`youtube`) + `target TEXT` (repo name, subreddit, video URL)
- Does **NOT** have `url` or `urls_json` columns
- `worker_lane.rs` reads `AXON_INGEST_LANES` (default 2) to run parallel lanes

## Known Gaps

| Gap | Status |
|-----|--------|
| YouTube age-restricted / private videos | `yt-dlp` exits non-zero; error is a per-video skip warning in playlist mode, job failure in single-video mode. No friendly message. |
| YouTube manual captions | Only `--write-auto-sub` is passed; `--write-subs` (manual captions) is not requested. Videos with manual but no auto-generated captions will fail. |

## yt-dlp Requirement

`yt-dlp` **must be installed and on `$PATH`**. The `youtube` command will fail at runtime with a cryptic process error if it's missing:
```
No such file or directory (os error 2)
```
Install: `pip install yt-dlp` or `brew install yt-dlp`. Verify: `yt-dlp --version`.

## Adding a New Ingest Source
1. Add parser in `crates/ingest/<source>.rs`
2. Add `CommandKind::<Source>` + CLI arg to `config.rs`
3. Add command handler in `crates/cli/commands/<source>.rs`
4. Add `source_type` variant handling in `ingest_jobs.rs` worker dispatch
5. Add env vars to `.env.example`
6. Add s6 worker lane entry if the source is job-queue-backed
