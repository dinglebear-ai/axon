# Qdrant Payload Schema Contract

Status: active
Last updated: 2026-05-21

This document is the authoritative reference for fields stored in Qdrant point payloads.
Code must conform to this contract; if the code diverges, update the code and this document
together in the same commit.

---

## Universal Fields

Every point in every collection carries these fields, regardless of source.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `url` | keyword | yes | Canonical source URL. Point ID = UUID v5(`NAMESPACE_URL`, `"<url>:<chunk_index>"` bytes). |
| `domain` | keyword | yes | Hostname only (`github.com`, `reddit.com`). |
| `source_type` | keyword | yes | See Source Types below. |
| `content_type` | keyword | no | `"markdown"` or `"text"`. |
| `chunk_index` | integer | yes | 0-based position within the document. |
| `chunk_text` | raw string | no | The stored text chunk. Never truncated. |
| `scraped_at` | datetime | yes | RFC3339 timestamp at embed time. |
| `payload_schema_version` | integer | yes | Schema version at embed time. Pre-lu6a points lack this field (implicit v1). Current: `4`. |

### Conditional Universal Fields

Present only when the condition is met. Absence is intentional — do not write null placeholders for these.

| Field | Qdrant type | Indexed | When present |
|-------|-------------|---------|--------------|
| `title` | raw string | no | Source has a title (ingest paths, most verticals). Absent for generic crawl/embed. |
| `extractor_name` | keyword | yes | Vertical extractor produced this point (`"github_repo"`, `"crates_io"`, etc.). Absent for crawl/embed. |
| `chunking_method` | keyword | yes | Code chunk strategy: `"tree_sitter"` or `"prose"`. Absent when not a code chunk. |
| `structured_kind` | keyword | no | Structured-data pass found JSON-LD/Next.js/SvelteKit: `"jsonld"`, `"next_data"`, `"sveltekit"`. |
| `structured_type` | raw string | no | Schema.org type when `structured_kind` is present (`"Article"`, `"Product"`, …). |
| `structured_id` | raw string | no | Schema.org `@id` when present. |
| `structured_blob` | raw JSON | no | Full raw structured-data JSON object. Not indexed; use `structured_kind` for filtering. |

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
| `git_state` | keyword | yes | `"open"` \| `"closed"` \| `"merged"` \| null. |
| `git_number` | integer | yes | Issue or PR number. Null for non-issue/PR content. |
| `git_author` | keyword | yes | Author login/username. |
| `git_labels` | keyword[] | no | Labels array. |
| `git_is_draft` | bool | no | PR draft status. |
| `git_merged_at` | raw string | no | ISO8601 merge timestamp. |
| `git_created_at` | raw string | no | ISO8601 creation timestamp. |
| `git_updated_at` | raw string | no | ISO8601 update timestamp. |
| `git_file_path` | keyword | no | Relative file path for `git_content_kind = "file"`. Not currently indexed — use `git_file_language` for filterable file queries. |
| `git_file_language` | keyword | yes | File language/extension for file chunks. |
| `git_meta` | raw JSON | no | Provider-specific extras (stars, visibility, clone_url, …). Not indexed. |

### GitHub-specific fields (top-level, indexed)

These fields carry GitHub-specific metadata with no `git_*` equivalent. They are **not** deprecated —
they are the canonical place to query GitHub-only data. All are indexed for Qdrant filtering.

| Field | Qdrant type | Indexed | Notes |
|-------|-------------|---------|-------|
| `gh_language` | keyword | yes | Primary repo language (e.g. `"Rust"`, `"Python"`). |
| `gh_file_type` | keyword | yes | File classification: `"source"` \| `"test"` \| `"config"` \| `"doc"`. From `classify_file_type()`. |
| `gh_topics` | keyword[] | yes | GitHub topics array (e.g. `["cli", "rag"]`). |
| `gh_is_fork` | bool | yes | Whether the repo is a fork. |
| `gh_is_archived` | bool | yes | Whether the repo is archived. |
| `gh_stars` | integer | yes | Stargazer count at ingest time. |
| `gh_forks` | integer | yes | Fork count at ingest time. |
| `gh_line_start` | integer | yes | First line of the code chunk (1-indexed, inclusive). For code attribution. |
| `gh_line_end` | integer | yes | Last line of the code chunk (1-indexed, inclusive). |

### GitHub backwards-compat fields (deprecated)

GitHub ingest also emits additional flat `gh_*` fields that duplicate `git_*` canonical fields
for backwards compatibility with existing indexed points. **New code should query `git_*` fields.**
The `gh_*` duplicates will be removed after a full re-index.

| `gh_*` field | Duplicates | Indexed |
|---|---|---|
| `gh_repo` | `git_repo` | no |
| `gh_owner` | `git_owner` | no |
| `gh_content_kind` | `git_content_kind` | no |
| `gh_branch` | `git_branch` | no |
| `gh_state` | `git_state` | no |
| `gh_issue_number` | `git_number` | no |
| `gh_author` | `git_author` | no |
| `gh_labels` | `git_labels` | no |
| `gh_is_draft` | `git_is_draft` | no |
| `gh_merged_at` | `git_merged_at` | no |
| `gh_created_at` | `git_created_at` | no |
| `gh_updated_at` | `git_updated_at` | no |
| `gh_file_path` | `git_file_path` | no |
| `gh_file_language` | `git_file_language` | yes (keyword) |
| `gh_default_branch` | `git_branch` | no |
| `gh_repo_description` | *(no git_* equivalent — in git_meta)* | no |
| `gh_pushed_at` | *(no git_* equivalent — in git_meta)* | no |
| `gh_is_private` | *(no git_* equivalent — in git_meta)* | no |
| `gh_open_issues` | *(no git_* equivalent — in git_meta)* | no |

**`git_meta` blob contents (not indexed):** `open_issues`, `is_private`, `default_branch`,
`repo_description`, `pushed_at`, `gh_is_test`, `gh_file_size_bytes`, `gh_comment_count`, `gh_is_pr`.
These are available for reference but cannot be efficiently filtered in Qdrant.

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

## Vertical Extractor Fields

Points produced by vertical extractors carry `extractor_name` plus a set of extractor-specific
flat fields. The full per-extractor schema is defined in
[`docs/specs/vertical-extractor-metadata.md`](../specs/vertical-extractor-metadata.md).

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
Quantization: scalar int8, quantile 0.99, always_ram = true.

---

## Point Lifecycle

### Upsert

Points are upserted in batches of `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 256, max 4096)
using `PUT /collections/{name}/points?wait=true`. Each batch retries up to 3 times with
exponential backoff (500ms, 1s, 2s). Point IDs are deterministic — upserting the same
`(url, chunk_index)` overwrites the existing point.

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
