# Qdrant Payload Schema Contract

Status: active
Last updated: 2026-06-13

This document is the authoritative reference for fields stored in Qdrant point payloads.
Code must conform to this contract; if the code diverges, update the code and this document
together in the same commit.

---

## Universal Fields

Every point in every collection carries these fields, regardless of source.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `url` | keyword | yes | Canonical source URL. Default point ID = UUID v5(`NAMESPACE_URL`, `"<url>:<chunk_index>"` bytes). Some stable record sources, such as memory, provide explicit point IDs. |
| `domain` | keyword | yes | Hostname only (`github.com`, `reddit.com`). |
| `seed_url` | keyword | yes | Origin that started this chunk's acquisition: the crawl start URL (crawl path) or ingest target (ingest path), distinct from the per-page `url`. Falls back to the doc's own `url` for direct `embed`/`scrape`. Faceted by `axon refresh`. Added in schema v5; absent on older points. |
| `source_type` | keyword | yes | See Source Types below. |
| `content_type` | keyword | no | `"markdown"` or `"text"`. |
| `chunk_index` | integer | yes | 0-based position within the document. |
| `chunk_text` | raw string | no | The stored text chunk. Never truncated. |
| `scraped_at` | datetime | yes | RFC3339 timestamp at embed time. |
| `payload_schema_version` | integer | yes | Schema version at embed time. Pre-lu6a points lack this field (implicit v1). Current: `8`. |

### Conditional Universal Fields

Present only when the condition is met. Absence is intentional — do not write null placeholders for these.

| Field | Qdrant type | Indexed | When present |
|-------|-------------|---------|--------------|
| `title` | raw string | no | Source has a title (ingest paths, most verticals). Absent for generic crawl/embed. |
| `extractor_name` | keyword | yes | Vertical extractor produced this point (`"github_repo"`, `"crates_io"`, etc.). Absent for crawl/embed. |
| `structured_kind` | keyword | no | Structured-data pass found JSON-LD/Next.js/SvelteKit: `"jsonld"`, `"next_data"`, `"sveltekit"`. |
| `structured_type` | raw string | no | Schema.org type when `structured_kind` is present (`"Article"`, `"Product"`, …). |
| `structured_id` | raw string | no | Schema.org `@id` when present. |
| `structured_blob` | raw JSON | no | Full raw structured-data JSON object. Not indexed; use `structured_kind` for filtering. |
| `chunk_content_kind` | keyword | yes | Planner classification for this chunk: `"code"`, `"markdown"`, or `"plain_text"`. Added in schema v8. |
| `chunk_locator` | raw string | no | Stable locator for the chunk within the source, e.g. `src/lib.rs#L10-L34` or `<url>#chunk-2048`. Added in schema v8. |
| `source_range` | raw JSON | no | Object with `line_start`, `line_end`, `byte_start`, and `byte_end` for the chunk. Added in schema v8. |
| `chunking_fallback` | keyword | no | Present when the source-doc planner used a safe fallback such as plain-text markdown handling. Added in schema v8. |
| `code_chunk_source` | keyword | no | File planner source for chunk metadata: `"tree_sitter"`, `"markdown"`, or `"prose"`. Added in schema v8. |

---

## Source Types

The `source_type` field identifies the ingestion path. Values are stable and must not be renamed.

| Value | Path | Notes |
|-------|------|-------|
| `"crawl"` | Spider crawl engine | Default for crawled pages. |
| `"embed"` | `axon embed` command | Local files/dirs embedded directly. |
| `"scrape"` | `axon scrape` command | Single-URL scrape + embed. |
| `"github"` | GitHub ingest | `src/ingest/github/` |
| `"gitlab"` | GitLab ingest | `src/ingest/gitlab/` |
| `"gitea"` | Gitea/Forgejo ingest | `src/ingest/gitea.rs` |
| `"git"` | Generic HTTPS git ingest | `src/ingest/generic_git.rs` |
| `"reddit"` | Reddit ingest | `src/ingest/reddit/` |
| `"youtube"` | YouTube ingest | `src/ingest/youtube/` |
| `"sessions"` | AI session export ingest — Gemini | `sessions/gemini.rs` writes this directly |
| `"claude_session"` | Claude session export | `sessions/claude.rs` |
| `"codex_session"` | Codex session export | `sessions/codex.rs` |
| `"memory"` | Persistent agent memory | `services::memory::remember()` writes atomic memory documents into the dedicated memory collection. |

---

## Git Provider Fields

All git-backed ingest sources (`github`, `gitlab`, `gitea`, `git`) emit these flat fields
in addition to their source-type-specific fields. See `src/ingest/git_payload.rs`.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `provider` | keyword | yes | `"github"` \| `"gitlab"` \| `"gitea"` \| `"git"` |
| `git_host` | keyword | yes | `"github.com"`, `"gitlab.com"`, `"codeberg.org"`, … |
| `git_owner` | keyword | yes | Org/user/namespace. For GitLab: namespace path minus final segment. Null for generic git. |
| `git_repo` | keyword | yes | Repository name (final path segment). |
| `git_content_kind` | keyword | yes | `"file"` \| `"issue"` \| `"pr"` \| `"release"` \| `"wiki"` \| `"repo_metadata"` |
| `git_branch` | keyword | no | Default or cloned branch. |
| `git_default_branch` | keyword | yes | Repository default branch when known. |
| `git_repo_description` | raw string | no | Repository description when known. |
| `git_repo_pushed_at` | raw string | no | Last pushed timestamp when known. |
| `git_repo_is_private` | bool | yes | Whether the repo is private when known. |
| `git_repo_stars` | integer | yes | Stargazer count at ingest time when available. |
| `git_repo_forks` | integer | yes | Fork count at ingest time when available. |
| `git_repo_open_issues` | integer | yes | Open issue count at ingest time when available. |
| `git_repo_language` | keyword | yes | Primary repo language when available. |
| `git_repo_topics` | keyword[] | yes | Repository topics/tags when available. |
| `git_repo_is_fork` | bool | yes | Whether the repo is a fork when available. |
| `git_repo_is_archived` | bool | yes | Whether the repo is archived when available. |
| `git_state` | keyword | yes | `"open"` \| `"closed"` \| `"merged"` \| null. |
| `git_number` | integer | yes | Issue or PR number. Null for non-issue/PR content. |
| `git_author` | keyword | yes | Author login/username. |
| `git_labels` | keyword[] | no | Labels array. |
| `git_comment_count` | integer | yes | Issue/PR comment count when available. |
| `git_is_pr` | bool | yes | Whether an issue-like item is a PR. |
| `git_is_draft` | bool | yes | PR draft status. |
| `git_merged_at` | raw string | no | ISO8601 merge timestamp. |
| `git_created_at` | raw string | no | ISO8601 creation timestamp. |
| `git_updated_at` | raw string | no | ISO8601 update timestamp. |
| `git_file_path` | keyword | yes | Relative file path for `git_content_kind = "file"`. Indexed in `payload_indexes.rs`; `git_file_language` is also indexed for language-scoped file queries. |
| `git_file_language` | keyword | yes | File language/extension for file chunks. |
| `git_meta` | raw JSON | no | Provider-specific extras that do not generalize. Not indexed. |

### Code Search Fields

Git-backed file chunks also emit provider-neutral `code_*` and symbol fields. These are the
code-search payload fields used by query ranking and result output.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `code_file_path` | keyword | yes | Relative file path for source/doc file chunks. Mirrors `git_file_path` for git providers. |
| `code_language` | keyword | yes | File language/extension for code-ranking and filters. |
| `code_file_type` | keyword | yes | `"source"` \| `"test"` \| `"config"` \| `"doc"`. |
| `code_is_test` | bool | yes | Test-file classification used for code-search demotion. |
| `code_file_size_bytes` | integer | yes | File size in bytes when known. |
| `code_line_start` | integer | yes | First line of the chunk (1-indexed, inclusive). |
| `code_line_end` | integer | yes | Last line of the chunk (1-indexed, inclusive). |
| `code_chunking_method` | keyword | yes | `"tree_sitter"` for symbol-aware chunks or `"prose"` for fallback chunks. |
| `symbol_name` | keyword | no | Extracted declaration/symbol name when available. Added in schema v6. |
| `symbol_kind` | keyword | yes | `"function"`, `"method"`, `"struct"`, `"enum"`, `"trait"`, `"impl"`, `"const"`, `"static"`, `"type"`, `"mod"`, `"other"`. Added in schema v6. |
| `symbol_extraction_status` | keyword | no | `"ok"`, `"unsupported"`, `"skipped_large"`, `"none_found"`, or `"prose"`. Added in schema v6. |

### Local Code Search Fields

Local `code-search` vectors use `source_type = "local_code"` and add these fields.
Absolute project roots are not stored in Qdrant; they stay in private SQLite
code-index state only. The derived project key is scoped to the canonical checkout
root, collection, embedder, and local index version. Generic `query`, `ask`, and
`retrieve` surfaces exclude `local_code`; `code-search` filters by project key and
committed generation. Changed refreshes write complete generation snapshots and
track previous-generation cleanup debt in SQLite until Qdrant deletion succeeds.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `local_project_key` | keyword | yes | Stable private project key derived from repository origin plus checkout/config identity. |
| `local_project_display` | keyword | no | Non-sensitive display label, usually the Git root directory name. |
| `local_file_hash` | keyword | no | SHA-256 content hash for the repository-relative file. |
| `local_index_version` | integer | yes | Local code index schema version. |
| `local_generation` | integer | yes | Local-code generation; `code-search` only queries the committed generation. |
| `code_file_path` | keyword | yes | Repository-relative path. |
| `code_path_prefixes` | keyword[] | yes | Prefix buckets used for exact path-prefix filtering. |

GitHub no longer emits `gh_*` duplicate fields in payload schema v7. Re-index cleanly after
upgrading if a collection still contains old `gh_*` points.

---

## Reddit Ingest Fields

Points from `source_type = "reddit"` carry these fields (from `src/ingest/reddit/meta.rs`).

| Field | Type | Indexed | Notes |
|-------|------|---------|-------|
| `reddit_author` | string | no | Post author login (`[deleted]` when removed) |
| `reddit_created_utc` | integer | no | Unix timestamp (float cast to u64) |
| `reddit_score` | integer | no | Net upvotes |
| `reddit_num_comments` | integer | no | |
| `reddit_upvote_ratio` | float | no | 0.0–1.0 |
| `reddit_subreddit` | string | yes | e.g. `"rust"` (without the `r/` prefix) |
| `reddit_domain` | string | no | Domain of linked content |
| `reddit_is_video` | bool | no | |
| `reddit_distinguished` | string\|null | no | `"moderator"`, `"admin"`, or absent |
| `reddit_gilded` | integer | no | Number of gold awards |
| `reddit_flair` | string\|null | no | Link flair text |

The reddit **vertical extractor** (not ingest) uses `extractor_name = "reddit"` and `source_type = "scrape"`.
It stores a `structured_blob` with raw post JSON but does **not** emit the flat `reddit_*` fields above.

---

## YouTube Ingest Fields

Points from `source_type = "youtube"` carry these fields (from `src/ingest/youtube/meta.rs`).

| Field | Type | Indexed | Notes |
|-------|------|---------|-------|
| `yt_video_id` | string | no | 11-character YouTube video ID |
| `yt_thumbnail` | string | no | Thumbnail URL |
| `yt_channel` | string | yes | Channel display name |
| `yt_channel_url` | string | no | Channel page URL |
| `yt_uploader_id` | string | no | Channel handle or user ID |
| `yt_upload_date` | string | no | `YYYYMMDD` format |
| `yt_duration` | string | no | Human-readable duration (e.g. `"12:34"`) |
| `yt_view_count` | integer\|null | no | |
| `yt_like_count` | integer\|null | no | |
| `yt_tags` | string[] | no | Video tags |
| `yt_categories` | string[] | no | Video categories |

YouTube ingest embeds two `PreparedDoc`s per video: one for the VTT transcript
(`url = "https://youtube.com/watch?v=<id>"`) and one for the description
(`url = "https://youtube.com/watch?v=<id>?section=description"`).

---

## Memory Fields

Persistent memory uses `source_type = "memory"` and stores content in the dedicated
memory collection (`axon_memory` by default, or `AXON_MEMORY_COLLECTION`). SQLite remains
the metadata/graph mirror. Memory documents are atomic: one memory record becomes one
Qdrant chunk with the same deterministic UUID used by SQLite, and the source-doc planner
adds `chunk_content_kind = "plain_text"`, `chunk_locator = "memory://<id>#chunk-0"`, and
`source_range`.

| Field | Type | Indexed | Notes |
|-------|------|---------|-------|
| `type` | string | no | Memory node type: `fact`, `decision`, etc. |
| `title` | string | no | Memory title, derived from the body when omitted. |
| `body` | string | no | Redacted memory body. |
| `project` | string | no | Project scope when known. |
| `repo` | string | no | Repository scope when known. |
| `file` | string | no | File scope when supplied. |
| `workspace` | string | no | Runtime workspace path when detected. |
| `git_branch` | string | no | Runtime git branch when detected. |
| `git_commit` | string | no | Runtime git commit when detected. |
| `git_dirty` | bool | no | Runtime dirty-worktree flag when detected. |
| `cwd` | string | no | Runtime current working directory when detected. |
| `confidence` | float | no | Caller-supplied confidence, default `1.0`. |
| `status` | string | no | `active` or `superseded`. Search excludes superseded memories. |
| `created_at` | integer | no | Unix timestamp mirrored from SQLite metadata. |
| `updated_at` | integer | no | Unix timestamp mirrored from SQLite metadata. |
| `last_seen_at` | integer | no | Unix timestamp used by recall/list flows. |
| `access_count` | integer | no | Recall access count. |

---

## Vertical Extractor Fields

Points produced by vertical extractors carry `extractor_name` plus a set of extractor-specific
flat fields. The full per-extractor schema is defined in
[`docs/architecture/specs/vertical-extractor-metadata.md`](../architecture/specs/vertical-extractor-metadata.md).

### Indexed vertical fields

| Field | Qdrant type | Extractors |
|-------|-------------|------------|
| `pkg_registry` | keyword | npm, pypi, crates_io, docs_rs |
| `pkg_name` | keyword | npm, pypi, crates_io, docs_rs |
| `pkg_language` | keyword | npm, pypi, crates_io, docs_rs |
| `pkg_license` | keyword | npm, pypi, crates_io |
| `pkg_author` | keyword | npm, pypi |
| `hf_task` | keyword | huggingface_model |
| `hf_library` | keyword | huggingface_model |
| `so_question_id` | integer | stackoverflow |
| `so_is_answered` | keyword | stackoverflow |
| `hn_type` | keyword | hackernews |
| `hn_author` | keyword | hackernews |
| `arxiv_id` | keyword | arxiv |
| `devto_author` | keyword | dev_to |

---

## Payload Schema Versioning

`payload_schema_version` is an integer stamped on every point at embed time.

| Version | Introduced | Changes |
|---------|------------|---------|
| 1 | (implicit) | All points before lu6a. No version field. |
| 2 | axon_rust-lu6a | Added `payload_schema_version`, `extractor_name`, `structured_*` fields. |
| 3 | 2026-05-21 | Added canonical git_* provider fields (git_host, git_owner, git_repo, git_content_kind, etc.) and vertical extractor extra payload fields. |
| 4 | 2026-05-21 | Promoted gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork, gh_is_archived, gh_file_type, gh_line_start, gh_line_end from git_meta blob to indexed top-level fields. Removed these keys from git_meta. |
| 5 | 2026-05-16 | Added indexed top-level `seed_url` origin tracking for `axon refresh`. |
| 6 | 2026-06-08 | Added code chunk `symbol_name`/`symbol_kind` metadata, `symbol_extraction_status`, and restored `code_chunking_method` writes for GitHub file chunks. |
| 7 | 2026-06-08 | Clean-break git/code schema: replaced new `gh_*` writes with canonical `git_*`, `code_*`, and symbol fields. |
| 8 | 2026-06-13 | Added normalized source-doc planner fields: `chunk_content_kind`, `chunk_locator`, `source_range`, `chunking_fallback`, and `code_chunk_source`; documented atomic `memory` source documents. |

Points without `payload_schema_version` are treated as version 1. Retrieval applies no version
filter by default — all points are queryable. Use
`VectorSearchRequest::with_payload_schema_version_min(Some(N))` to scope to version-aware fields.

---

## Collection Schema and VectorMode

Collections are created with **named vectors** (`dense` + `bm42` sparse) for hybrid RRF search.
Legacy collections with a single unnamed dense vector are `VectorMode::Unnamed` and use
cosine-only `/points/search`. The mode is detected once per process via `ensure_collection()` /
`get_or_fetch_vector_mode()` and cached in `COLLECTION_MODES` (`LazyLock<RwLock<HashMap>>`).

| VectorMode | Vector layout | Search path |
|------------|---------------|-------------|
| `Named` | `dense` (float32) + `bm42` (sparse, IDF modifier) | `/points/query` with RRF prefetch |
| `Unnamed` | single unnamed float32 vector | `/points/search` cosine only |

New collections are always created as `Named`. Upgrade legacy collections with `axon migrate`.

HNSW config for new collections: `m = 32`, `ef_construct = 256`.
Quantization: scalar int8, quantile 0.99, always_ram = false.

---

## Point Lifecycle

### Upsert

Points are upserted in batches of `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 1024, max 4096)
using `PUT /collections/{name}/points?wait=true`. Each batch retries up to 3 times with
exponential backoff (500ms, 1s, 2s). Point IDs are deterministic. Most sources derive IDs
from `(url, chunk_index)`, while stable record sources may provide explicit IDs; memory uses
the memory UUID directly. Upserting the same point ID overwrites the existing point.

### Stale-tail cleanup

When a document is re-embedded with fewer chunks than before (e.g. page content shrank),
orphan points with `chunk_index >= new_chunk_count` are deleted after a successful upsert
via `qdrant_delete_stale_tail()`. Deletion uses `wait=false` (async) — consistency is
guaranteed by the preceding `wait=true` upsert. This prevents phantom chunks from stale
versions of a page from appearing in search results.

### Delete by URL

`qdrant_delete_by_url_filter()` deletes all points matching a given `url` keyword filter.
Used by maintenance operations (deduplication, explicit removal). Uses `wait=true`.

---

## Design Rules

1. **Absent beats null (target rule; see exception below).** Do not write `"field": null`
   for optional fields that aren't applicable. Qdrant equality filters on absent fields
   produce no results, same as `null`, but absent fields don't bloat the payload.
   **Exception:** `build_git_payload()` in `src/ingest/git_payload.rs` currently serializes
   `None` as JSON `null` because `serde_json::json!()` macro has no `skip_if_none` option and
   the struct is not `#[serde(skip_serializing_if)]`-annotated. New code should follow this
   rule; a cleanup pass to remove null writes from `build_git_payload()` is tracked as a
   follow-up improvement.

2. **Flat beats nested for indexed fields.** Fields you want to filter or facet on must be flat
   top-level keys. Nested blobs (`git_meta`, `structured_blob`, `gitlab: {...}`) are stored for
   reference but cannot be efficiently filtered.

3. **Arrays are stored as Qdrant keyword arrays.** Qdrant matches `keyword` arrays with
   `values_count` or `match any` filters. Index array fields as `"keyword"` type; Qdrant handles
   the array semantics.

4. **Prefix namespacing is mandatory.** Every source-specific field must carry the source prefix
   (`git_*`, `yt_*`, `reddit_*`, `npm_*`, `hf_*`, etc.). Universal fields have no prefix.
   This prevents collisions and makes source identification trivial.

5. **Indexes are cumulative.** `ensure_payload_indexes()` is idempotent — safe to call on every
   embed. When adding a new indexed field, add it to `payload_indexes.rs` and this document in
   the same commit.

6. **Stable field names.** Renaming an indexed field requires re-indexing all points. Prefer
   additive changes. If renaming is necessary, keep the old field as a deprecated alias until
   re-index is confirmed.
