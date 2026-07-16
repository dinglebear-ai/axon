# Re-index Guide: Schema v3-v8 Payload Upgrade

This guide explains what changed in Qdrant payload schema versions 3 through 8, who needs to
re-index existing points, and how to do it efficiently and safely.

The current schema version is **8** (`PAYLOAD_SCHEMA_VERSION = 8` in
`src/vector/ops/qdrant/utils.rs`). Every point written by `axon` today carries
`payload_schema_version = 8`. Versions 3 and 4 were introduced on 2026-05-21, v5 added
origin tracking, v6 added code-symbol metadata for GitHub file chunks, and v7 is the
clean-break git/code payload schema (provider-neutral `git_*`/`code_*` fields, no `gh_*`).
Schema v8 adds normalized source-doc planner metadata (`chunk_content_kind`,
`chunk_locator`, `source_range`, `chunking_fallback`, and `code_chunk_source`) and routes
memory records through the same pre-chunk planning boundary.

---

## What Changed in Schema v3

Schema v3 was introduced on 2026-05-21. Points written before that date are version 1 or 2
and do not have the new fields.

### New: canonical `git_*` provider fields

All git-backed ingest sources (`github`, `gitlab`, `gitea`, `git`) now emit a uniform set
of flat, indexed fields. Previously, GitHub ingest used ad-hoc `gh_*` fields; GitLab,
Gitea, and generic git had their own inconsistent layouts.

| Field | Indexed | Example values |
|-------|---------|----------------|
| `provider` | yes | `"github"` \| `"gitlab"` \| `"gitea"` \| `"git"` |
| `git_host` | yes | `"github.com"`, `"gitlab.com"`, `"codeberg.org"` |
| `git_owner` | yes | org/user/namespace |
| `git_repo` | yes | repository name |
| `git_content_kind` | yes | `"file"` \| `"issue"` \| `"pr"` \| `"release"` \| `"wiki"` \| `"repo_metadata"` |
| `git_state` | yes | `"open"` \| `"closed"` \| `"merged"` \| absent |
| `git_number` | yes | issue or PR number |
| `git_author` | yes | login/username |
| `git_file_language` | yes | file language/extension for file chunks |
| `git_branch` | no | default or cloned branch |
| `git_labels` | no | labels array |
| `git_is_draft` | no | PR draft status |
| `git_created_at` | no | ISO8601 creation timestamp |
| `git_updated_at` | no | ISO8601 update timestamp |
| `git_merged_at` | no | ISO8601 merge timestamp |
| `git_file_path` | no | relative file path for file chunks |
| `git_meta` | no | provider-specific extras (not indexed) |

Queries that were previously GitHub-only (`gh_*`) can now target all git providers
uniformly. For example, filtering `git_content_kind = "issue"` returns issues from
GitHub, GitLab, and Gitea in a single search.

The legacy `gh_*` fields on existing GitHub points are backwards-compatible aliases and
will be removed after a confirmed full re-index.

### New: vertical extractor `extra` payload fields

Vertical extractors (`github_repo`, `npm`, `pypi`, `crates_io`, `huggingface_model`,
`stackoverflow`, `hackernews`, `arxiv`, `dev_to`, `reddit`, etc.) now emit additional
indexed fields that enable source-specific filtering:

| Field | Extractors |
|-------|------------|
| `pkg_registry`, `pkg_name`, `pkg_language`, `pkg_license`, `pkg_author` | npm, pypi, crates_io, docs_rs |
| `hf_task`, `hf_library` | huggingface_model |
| `so_question_id`, `so_is_answered` | stackoverflow |
| `hn_type`, `hn_author` | hackernews |
| `arxiv_id` | arxiv |
| `devto_author` | dev_to |

These fields are absent on points written before schema v3.

---

## What Changed in Schema v4

Schema v4 (also 2026-05-21) **promoted** a set of GitHub repo fields out of the
non-indexed `git_meta` blob and into indexed top-level payload keys, and removed those keys
from `git_meta`. The promoted, now-indexed fields are:

| Field | Qdrant type | Notes |
|-------|-------------|-------|
| `gh_stars` | integer | Stargazer count at ingest time |
| `gh_forks` | integer | Fork count at ingest time |
| `gh_language` | keyword | Primary repo language |
| `gh_topics` | keyword[] | GitHub topics array |
| `gh_is_fork` | bool | Whether the repo is a fork |
| `gh_is_archived` | bool | Whether the repo is archived |
| `gh_file_type` | keyword | `"source"` \| `"test"` \| `"config"` \| `"doc"` |
| `gh_line_start` | integer | First line of a code chunk (1-indexed) |
| `gh_line_end` | integer | Last line of a code chunk (1-indexed) |

On v1–v3 GitHub points these values were either absent or buried inside the unindexed
`git_meta` blob, so you could not filter or facet on them. v4 makes them queryable. See the
full contract in [`docs/reference/qdrant-payload-schema.md`](../reference/qdrant-payload-schema.md)
— GitHub-specific fields.

These fields are written only by the GitHub ingest path, so only GitHub points benefit from
a v4 re-index; GitLab/Gitea/generic-git/vertical/crawl points gain nothing new at v4 beyond
the version stamp.

## What Changed in Schema v5

Schema v5 added the indexed top-level `seed_url` field. This is the origin that started
the chunk's acquisition: the site/page URL for web sources, the source target for git/feed/
media/social/session sources, or the document URL for single-page scrapes. Source refresh
and watch operations use this field to reconnect indexed chunks to their originating source.

Points indexed before v5 do not participate in refresh origin discovery until they are
re-indexed.

## What Changed in Schema v6

Schema v6 adds code declaration metadata for GitHub file chunks:

| Field | Indexed | Notes |
|-------|---------|-------|
| `code_chunking_method` | yes | Write of `"tree_sitter"` or `"prose"` for GitHub file chunks. |
| `symbol_name` | no | Declaration name when known, e.g. `"Response::parse"`. Stored for retrieval, not indexed. |
| `symbol_kind` | yes | Low-cardinality declaration kind such as `"function"`, `"method"`, `"struct"`, `"const"`, or `"type"`. |
| `symbol_extraction_status` | no | File-level status that explains missing symbol fields: `"ok"`, `"unsupported"`, `"skipped_large"`, `"none_found"`, or `"prose"`. |

GitHub code chunks also changed boundaries because doc comments, duplicate capture cleanup,
qualified names, tiny declaration merging, and oversized-declaration header injection all
operate before embedding. A successful full GitHub re-ingest now embeds the current file set
first, then removes stale repo file URLs that were not recreated. Partial `--no-source` ingests
skip repo-level stale cleanup so an intentionally partial refresh does not delete existing
source-code chunks.

## What Changed in Schema v7

Schema v7 is a clean break for git-backed file chunks: new points write provider-neutral
`git_*` and `code_*` fields and no longer emit the legacy `gh_*` duplicate keys. The `code_*`
family (`code_file_path`, `code_language`, `code_file_type`, `code_is_test`, `code_line_start`,
`code_line_end`, `code_chunking_method`) is the canonical home for file/code metadata across
all git providers. Retrieval reads `code_*` with a `git_*` fallback, so points written before
v7 still rank and display correctly — but they retain the old `gh_*` keys until re-ingested.

Re-ingest GitHub repositories to gain the canonical `code_*` keys and drop the `gh_*`
duplicates; no other source type changes at v7.

## What Changed in Schema v8

Schema v8 records the output of the normalized source-doc planner on every newly embedded
chunk:

| Field | Indexed | Notes |
|-------|---------|-------|
| `chunk_content_kind` | yes | `"code"`, `"markdown"`, or `"plain_text"` for the actual chunk. |
| `chunk_locator` | no | Stable locator such as `src/lib.rs#L10-L34` or `<url>#chunk-2048`. |
| `source_range` | no | JSON object with line and byte start/end offsets. |
| `chunking_fallback` | no | Present when markdown planning fell back to safe plain-text chunking. |
| `code_chunk_source` | no | File planner method: `"tree_sitter"`, `"markdown"`, or `"prose"`. |

This is an enrichment-only change for most retrieval. Re-index sources when you want
locator/range metadata in retrieved chunks, want to filter by `chunk_content_kind`, or want
local/git file chunks to reflect the current one-file-`PreparedDoc` planner shape. Memory
records written at v8 use `source_type = "memory"` and an explicit stable point ID matching
the SQLite memory UUID.

---

## Who Needs to Re-index

Re-indexing is **optional but recommended** for anyone who wants to filter or facet on
the new v3 fields against data that was indexed before 2026-05-21.

You specifically benefit from re-indexing if you:

- Query GitHub, GitLab, Gitea, or generic git ingest points and want to filter by
  `git_content_kind`, `git_file_language`, `git_author`, `git_state`, or `git_owner`.
- Use source refresh/watch operations and want older sources to be discoverable via `seed_url`.
- Query GitHub code and want to filter by `symbol_kind` or inspect `symbol_name` /
  `code_chunking_method` in retrieved chunks.
- Need normalized chunk locators/ranges (`chunk_locator`, `source_range`) or want to filter
  mixed collections by `chunk_content_kind`.
- Query vertical extractor points (npm, PyPI, Crates.io, HuggingFace, etc.) and want to
  filter by package or model metadata fields.
- Run any query that would return mixed results and you want to scope to a specific
  provider or content kind.

If you only do semantic similarity search (`axon query` / `axon ask`) without payload
filters, existing points are unaffected and re-indexing has no impact on result quality.

---

## How to Identify Stale Points

### Check the version distribution

```bash
axon sources --by-schema-version
```

This scrolls the entire collection and counts points per `payload_schema_version`. Output
example:

```
Payload schema version breakdown
  v1 (chunks: 2841200)
  v2 (chunks: 612400)
  v3 (chunks: 200000)
  v4 (chunks: 140000)
```

The breakdown is a `BTreeMap<u32, usize>` keyed by version (points lacking the field are
tallied under `v1`); `--json` emits the same counts under `schema_version_breakdown`. This is
an O(N) full scroll — on a 3.79M-point collection it takes a few minutes. Run it once to get
your bearings, not on every startup.

### Check a specific source type

```bash
# Inspect stored chunks for a known GitHub source URL
axon retrieve https://github.com/org/repo --json | head
```

If the returned payloads lack `git_content_kind` (or, for the v4 fields, `gh_stars` /
`gh_file_type`), those points predate the corresponding schema version and would benefit from
re-ingest. (`axon query` does not take a payload `--filter`; use `retrieve` by URL to inspect a
specific source, or `sources --by-schema-version` for the collection-wide breakdown.)

---

## Re-indexing by Source Type

Re-ingesting a source overwrites existing points via Qdrant's upsert semantics when the
source URL and chunk index are stable. For GitHub file chunks, line ranges are part of the
stored URL (`#Lstart-Lend`), so boundary-shifting chunker changes require special cleanup.
Axon embeds the current file set first, then deletes only stale GitHub `file` points — repo
file URLs that were indexed previously but not re-embedded this run — via an async
(`wait=false`) URL-scoped delete, scoped by `provider=github`, `git_owner`, `git_repo`, and
`git_content_kind=file`.

Embedding before the stale delete means an interrupted ingest never empties the repo's file
corpus: the previous points remain until a later run supersedes them. Stale cleanup is also
skipped entirely after any read/embed failure and on partial `--no-source` ingests, so a
partial run never deletes valid source-code chunks.

### Crawl and embed points

Re-index the original site URL:

```bash
axon https://example.com/docs --scope site --wait true
```

Or for single pages:

```bash
axon scrape https://example.com/page
```

Site-scoped source indexing embeds every page it visits. Points for pages that no longer
exist are cleaned up through source/prune cleanup debt; use `axon prune dedupe` only when
you explicitly want a collection dedupe pass.

### GitHub repositories

```bash
axon https://github.com/org/repo --wait true
```

This re-ingests source files, issues, PRs, releases, and wiki pages. Each re-embedded
point carries the full v3 `git_*` field set and replaces the corresponding v1/v2 point.
For file chunks, Axon writes the current file set first, then removes only stale prior GitHub
file points (those not re-embedded this run) with the async delete described above.
Issues, PRs, releases, wiki pages, and repo metadata are not deleted by that file-point
cleanup.

`axon migrate` does not backfill v6 symbol metadata because it only transforms existing
Qdrant points and computes sparse vectors; it does not re-read source code or run the
chunker. Use `axon https://github.com/org/repo --wait true` for symbol backfill.

### GitLab projects

```bash
axon https://gitlab.com/group/project --wait true
```

Same semantics as GitHub. v3 `git_*` fields are written by the GitLab ingest path
(`src/ingest/gitlab/`).

### Gitea / Forgejo / generic git

```bash
# Gitea/Forgejo
axon https://codeberg.org/owner/repo --wait true

# Generic HTTPS git (bare clone, source files only)
axon git:https://git.example.com/owner/repo.git --wait true
```

### Reddit subreddits and threads

```bash
axon r/rust --wait true
axon https://www.reddit.com/r/rust/comments/abc123/post_title/ --wait true
```

Reddit ingest writes flat `reddit_*` fields. These fields are unchanged by v8, but
re-ingesting is the only way to get the current `payload_schema_version` and normalized
chunk metadata stamped on those points.

### YouTube videos, playlists, and channels

```bash
axon https://www.youtube.com/watch?v=VIDEO_ID --wait true
axon https://www.youtube.com/channel/CHANNEL_ID --wait true
```

YouTube ingest writes flat `yt_*` fields. Same situation as Reddit: source-specific fields
are unchanged, but re-ingest updates the schema version stamp and normalized chunk metadata.

### AI session exports

```bash
axon sessions --wait true
```

Sessions are re-indexed from the export files in your configured sessions directory.
Session points (`claude_session`, `codex_session`, `sessions`) have no git-specific fields,
so re-indexing these is low priority unless you want current schema stamps and planner
chunk metadata.

---

## Prioritization

Re-indexing takes time — on a 3.79M-point collection, a full re-index is a multi-hour
background operation. Focus on the highest-value sources first.

| Priority | Source | Reason |
|----------|--------|--------|
| **Highest** | GitHub / GitLab repos you actively query by content kind, file language, or author | `git_content_kind`, `git_file_language`, `git_state` filters require v3; GitHub repo facets (`gh_stars`, `gh_forks`, `gh_language`, `gh_topics`, `gh_is_fork`, `gh_is_archived`, `gh_file_type`) require v4 |
| **High** | npm / PyPI / Crates.io / HuggingFace verticals | Package metadata filters (`pkg_name`, `pkg_language`, `hf_task`) require v3 |
| **Medium** | Reddit / Hacker News ingest | Per-community faceting; fields unchanged but version stamp is wrong |
| **Low** | Generic crawl/embed points | Re-crawl only if content has changed or you need v8 chunk locator/range metadata |
| **Low** | AI session exports | No git-specific fields; re-run only for current schema stamps and planner metadata |

Run re-indexing with `--wait false` (the default) to enqueue jobs in the background and
continue working:

```bash
axon https://github.com/org/repo-a
axon https://github.com/org/repo-b
axon https://github.com/org/repo-c
axon status  # monitor job queue
```

---

## Upsert Semantics: No Orphan Cleanup Needed

Point IDs in axon are deterministic. Most sources use UUID v5 hashes derived from `url`
and `chunk_index`; stable record sources may provide explicit point IDs, and memory uses the
memory UUID directly. Re-ingesting the same source produces the same point IDs when source
URLs/chunk indexes or explicit IDs are stable, so Qdrant upsert overwrites the old payload in
place.

This means:

- **No duplicates.** A re-ingest of `github.com/org/repo` overwrites every existing
  chunk for that repo. You will not end up with two copies.
- **No manual delete step.** There is no need to delete old points before re-ingesting.
- **Stale-tail cleanup is automatic.** If a file that previously had 10 chunks now has 6
  (because content was removed), the 4 orphan chunks (`chunk_index 6–9`) are deleted
  automatically after a successful upsert. This is handled by `qdrant_delete_stale_tail()`
  in the embed pipeline.
- **`axon prune dedupe` is optional.** Run it after site re-indexing if you suspect
  near-duplicate chunks from page reorganizations, but it is not required for correctness.

---

## Estimated Scope

The user collection has approximately **3.79M points** as of 2026-05-21.

| Scenario | Estimated time |
|----------|---------------|
| Single GitHub repo (small, ~500 files, ~50 issues) | 2–5 minutes |
| Single GitHub repo (large, ~5,000 files, ~2,000 issues) | 20–40 minutes |
| Full crawl of a mid-size docs site (~2,000 pages) | 10–30 minutes |
| All priority GitHub/GitLab repos (10–20 repos) | 2–8 hours (background) |
| Full collection re-index (all sources) | Multi-day background task |

The full collection re-index is not blocking and does not need to be done all at once.
Newly indexed points are immediately queryable with v3 filters; old points continue to
serve semantic search results without interruption. The collection is always live — there
is no maintenance window or read blackout during re-indexing.

---

## Quick-Start Checklist

```bash
# 1. See your current indexed sources
axon sources --json

# 2. Re-index your highest-priority GitHub repos (background jobs)
axon https://github.com/org/repo-1
axon https://github.com/org/repo-2

# 3. Monitor progress
axon status

# 4. Verify refreshed source rows are appearing
axon sources --json

# 5. Optionally clean up near-duplicates after crawl re-indexing
axon prune dedupe
```

---

## Reference

- Payload contract: [`docs/reference/qdrant-payload-schema.md`](../reference/qdrant-payload-schema.md)
- Vector payload schema: `docs/reference/sources/vector-payload.schema.json`
- Git provider field builders: `crates/axon-adapters/` and `crates/axon-services/src/*_source/`
- Collection vector mode upgrade (unnamed → named): [`README.md`](../../README.md) — `axon migrate`
