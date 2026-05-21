# Vertical Metadata Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mark all implementation-status rows as "done" in the vertical extractor metadata spec, add two missing Qdrant keyword indexes (`reddit_subreddit`, `yt_channel`), and update the payload contract doc to reflect those indexes.

**Architecture:** Two independent, additive changes — (1) a docs-only status table update in `docs/specs/vertical-extractor-metadata.md`, and (2) a code + docs update that appends two strings to `KEYWORD_INDEX_FIELDS` in `src/vector/ops/tei/qdrant_store/payload_indexes.rs` and marks the two fields as indexed in `docs/contracts/qdrant-payload-schema.md`.

**Tech Stack:** Rust (edit only — no new types), Markdown docs.

---

## Files

| File | Change |
|------|--------|
| `docs/specs/vertical-extractor-metadata.md` | Update Implementation Status table: every "pending" → "done" |
| `src/vector/ops/tei/qdrant_store/payload_indexes.rs` | Add `"reddit_subreddit"` and `"yt_channel"` to `KEYWORD_INDEX_FIELDS` |
| `docs/contracts/qdrant-payload-schema.md` | Mark `reddit_subreddit` and `yt_channel` as indexed in their respective tables |

---

### Task 1: Update implementation status table

**Files:**
- Modify: `docs/specs/vertical-extractor-metadata.md` (lines 324–345, the Implementation Status table)

- [ ] **Step 1: Open the file and locate the table**

  The table starts at the `## Implementation Status` heading near the bottom of the file. The current content is:

  ```markdown
  ## Implementation Status

  | Component | Status |
  |-----------|--------|
  | `extra` field on `ScrapedDoc` | pending |
  | `extra` on `ScrapeResult` | pending |
  | Scrape CLI PreparedDoc path | pending |
  | GitHub verticals (`git_*`) | pending (foundation in git_payload.rs exists) |
  | npm | pending |
  | pypi | pending |
  | crates_io | pending |
  | docs_rs | pending |
  | docker_hub | pending |
  | huggingface_model | pending |
  | dev_to | pending |
  | shopify | pending |
  | hackernews | pending |
  | stackoverflow | pending |
  | arxiv | pending |
  | amazon | pending |
  | ebay | pending |
  | Payload indexes | pending |
  ```

- [ ] **Step 2: Replace every "pending" row with "done"**

  Replace the entire Implementation Status section at the bottom of
  `docs/specs/vertical-extractor-metadata.md` with:

  ```markdown
  ## Implementation Status

  | Component | Status |
  |-----------|--------|
  | `extra` field on `ScrapedDoc` | done |
  | `extra` on `ScrapeResult` | done |
  | Scrape CLI PreparedDoc path | done |
  | GitHub verticals (`git_*`) | done |
  | npm | done |
  | pypi | done |
  | crates_io | done |
  | docs_rs | done |
  | docker_hub | done |
  | huggingface_model | done |
  | dev_to | done |
  | shopify | done |
  | hackernews | done |
  | stackoverflow | done |
  | arxiv | done |
  | amazon | done |
  | ebay | done |
  | Payload indexes | done |
  ```

- [ ] **Step 3: Commit**

  ```bash
  rtk git add docs/specs/vertical-extractor-metadata.md
  rtk git commit -m "docs: mark all vertical-extractor-metadata items as done (PR #117 shipped)"
  ```

---

### Task 2: Add reddit_subreddit and yt_channel to KEYWORD_INDEX_FIELDS

**Files:**
- Modify: `src/vector/ops/tei/qdrant_store/payload_indexes.rs` (the `KEYWORD_INDEX_FIELDS` const, lines 11–40)

- [ ] **Step 1: Locate the insertion point**

  The `KEYWORD_INDEX_FIELDS` const currently ends with:

  ```rust
      "arxiv_id",
      "devto_author",
  ];
  ```

  The two new fields belong in the vertical-extractor section, after the existing vertical fields
  and before the closing `];`.

- [ ] **Step 2: Add the two new fields**

  Edit `src/vector/ops/tei/qdrant_store/payload_indexes.rs` — replace the closing lines of
  `KEYWORD_INDEX_FIELDS`:

  ```rust
      "arxiv_id",
      "devto_author",
  ];
  ```

  with:

  ```rust
      "arxiv_id",
      "devto_author",
      // Ingest source fields promoted to indexes for per-source filtering.
      "reddit_subreddit",
      "yt_channel",
  ];
  ```

- [ ] **Step 3: Verify the file compiles**

  ```bash
  rtk cargo check 2>&1 | tail -20
  ```

  Expected: `Finished` with no errors. The change is additive — no type changes, no new
  imports needed.

- [ ] **Step 4: Commit**

  ```bash
  rtk git add src/vector/ops/tei/qdrant_store/payload_indexes.rs
  rtk git commit -m "feat: index reddit_subreddit and yt_channel as keyword payload fields"
  ```

---

### Task 3: Update qdrant-payload-schema.md to reflect new indexes

**Files:**
- Modify: `docs/contracts/qdrant-payload-schema.md`

  Two tables need updating:
  1. **Reddit Ingest Fields** — `reddit_subreddit` row currently has no "Indexed" column; the
     table uses `Field | Type | Notes` columns. Add an `Indexed` column **or** note "indexed"
     in the Notes column of the `reddit_subreddit` row.
  2. **YouTube Ingest Fields** — same pattern for `yt_channel`.

- [ ] **Step 1: Update the Reddit Ingest Fields table**

  The current Reddit table header is `| Field | Type | Notes |` with no Indexed column. The
  preamble says "None are currently Qdrant-indexed". Both must change.

  Locate this block in `docs/contracts/qdrant-payload-schema.md`:

  ```markdown
  ## Reddit Ingest Fields

  Points from `source_type = "reddit"` carry these fields (from `src/ingest/reddit/meta.rs`).
  None are currently Qdrant-indexed; add indexes here and in `payload_indexes.rs` together.

  | Field | Type | Notes |
  |-------|------|-------|
  | `reddit_author` | string | Post author login (`[deleted]` when removed) |
  | `reddit_created_utc` | integer | Unix timestamp (float cast to u64) |
  | `reddit_score` | integer | Net upvotes |
  | `reddit_num_comments` | integer | |
  | `reddit_upvote_ratio` | float | 0.0–1.0 |
  | `reddit_subreddit` | string | e.g. `"rust"` (without the `r/` prefix) |
  | `reddit_domain` | string | Domain of linked content |
  | `reddit_is_video` | bool | |
  | `reddit_distinguished` | string\|null | `"moderator"`, `"admin"`, or absent |
  | `reddit_gilded` | integer | Number of gold awards |
  | `reddit_flair` | string\|null | Link flair text |
  ```

  Replace it with:

  ```markdown
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
  ```

- [ ] **Step 2: Update the YouTube Ingest Fields table**

  Locate this block:

  ```markdown
  ## YouTube Ingest Fields

  Points from `source_type = "youtube"` carry these fields (from `src/ingest/youtube/meta.rs`).
  None are currently Qdrant-indexed.

  | Field | Type | Notes |
  |-------|------|-------|
  | `yt_video_id` | string | 11-character YouTube video ID |
  | `yt_thumbnail` | string | Thumbnail URL |
  | `yt_channel` | string | Channel display name |
  | `yt_channel_url` | string | Channel page URL |
  | `yt_uploader_id` | string | Channel handle or user ID |
  | `yt_upload_date` | string | `YYYYMMDD` format |
  | `yt_duration` | string | Human-readable duration (e.g. `"12:34"`) |
  | `yt_view_count` | integer\|null | |
  | `yt_like_count` | integer\|null | |
  | `yt_tags` | string[] | Video tags |
  | `yt_categories` | string[] | Video categories |
  ```

  Replace it with:

  ```markdown
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
  ```

- [ ] **Step 3: Commit**

  ```bash
  rtk git add docs/contracts/qdrant-payload-schema.md
  rtk git commit -m "docs: mark reddit_subreddit and yt_channel as indexed in payload schema contract"
  ```

---

## Self-Review

**Spec coverage:**
- Status table update: Task 1 covers all 18 rows.
- Payload index additions: Task 2 adds both `reddit_subreddit` and `yt_channel` to `KEYWORD_INDEX_FIELDS`.
- Contract doc updates: Task 3 updates both Reddit and YouTube tables with an Indexed column and yes/no values.

**Placeholder scan:** No TBDs or "implement later" strings. All edits show exact before/after content.

**Type consistency:** No new types introduced. String literals in `KEYWORD_INDEX_FIELDS` match the field names in the contract doc.
