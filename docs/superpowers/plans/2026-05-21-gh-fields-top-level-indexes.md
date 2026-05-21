# GitHub gh_* Fields — Promote from git_meta to Top-Level + Add Qdrant Indexes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote nine high-value GitHub-specific fields out of the unindexed `git_meta` JSON blob and into flat top-level Qdrant payload keys, then add keyword and integer Qdrant payload indexes so those fields are filterable and facetable.

**Architecture:** `build_github_payload()` already emits the flat `gh_*` keys (added as backwards-compat aliases); the bug is that the same data is *also* redundantly stored inside `git_meta` — but more critically the `git_meta` blob is the *only* place some variant of these fields exists in the final Qdrant document structure check. The fix is: (1) stop duplicating the promoted fields into `git_meta`, keeping `git_meta` for truly unqueryable extras, (2) verify the flat keys are already being emitted correctly by the existing `obj.insert(...)` calls, and (3) register the missing Qdrant indexes. The tests and schema contract doc are updated in the same pass.

**Tech Stack:** Rust (`serde_json::json!`, `serde_json::Value`), Qdrant REST API (`PUT /collections/{name}/index`), cargo test.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/ingest/github/meta.rs` | Modify | Remove promoted fields from `git_meta`; keep remaining extras there |
| `src/ingest/github/meta_tests.rs` | Modify | Assert promoted fields appear as top-level keys; assert they are NOT inside `git_meta` |
| `src/vector/ops/tei/qdrant_store/payload_indexes.rs` | Modify | Add keyword indexes for `gh_language`, `gh_file_type`, `gh_topics`, `gh_is_fork`, `gh_is_archived`; add integer indexes for `gh_stars`, `gh_forks`, `gh_line_start`, `gh_line_end` |
| `docs/contracts/qdrant-payload-schema.md` | Modify | Document which `gh_*` fields are top-level indexed (GitHub-specific, not deprecated) vs. true duplicates of `git_*` equivalents (deprecated) |

---

## Task 1: Understand the current `git_meta` contents and what to keep

**Files:**
- Read: `src/ingest/github/meta.rs`

This is an orientation step — no code changes. The goal is to confirm exactly what stays in `git_meta` after the promoted fields are removed.

Current `git_meta` in `build_github_payload()`:
```json
{
  "stars":              params.stars,
  "forks":              params.forks,
  "open_issues":        params.open_issues,
  "language":           params.language,
  "topics":             params.topics,
  "is_fork":            params.is_fork,
  "is_archived":        params.is_archived,
  "is_private":         params.is_private,
  "default_branch":     params.default_branch,
  "repo_description":   params.repo_description,
  "pushed_at":          params.pushed_at,
  "gh_file_type":       params.file_type,
  "gh_is_test":         params.is_test,
  "gh_file_size_bytes": params.file_size_bytes,
  "gh_line_start":      params.gh_line_start,
  "gh_line_end":        params.gh_line_end,
  "gh_comment_count":   params.comment_count,
  "gh_is_pr":           params.is_pr,
}
```

Fields to **promote** (remove from `git_meta`, already emitted as flat top-level keys):
- `stars` → `gh_stars` (integer)
- `forks` → `gh_forks` (integer)
- `language` → `gh_language` (keyword)
- `topics` → `gh_topics` (keyword[])
- `is_fork` → `gh_is_fork` (bool stored as keyword "true"/"false" in Qdrant, or native bool — see Task 3)
- `is_archived` → `gh_is_archived` (bool)
- `gh_file_type` → `gh_file_type` (keyword) — note: key name in git_meta already has the `gh_` prefix
- `gh_line_start` → `gh_line_start` (integer)
- `gh_line_end` → `gh_line_end` (integer)

Fields to **keep in `git_meta`** (lower priority, not being indexed now):
- `open_issues` — useful but lower query priority
- `is_private` — already emitted as `gh_is_private` flat key
- `default_branch` — already emitted as `gh_default_branch`
- `repo_description` — already emitted as `gh_repo_description`
- `pushed_at` — already emitted as `gh_pushed_at`
- `gh_is_test` — keep in meta for now (lower priority)
- `gh_file_size_bytes` — keep in meta for now
- `gh_comment_count` — keep in meta for now
- `gh_is_pr` — keep in meta for now

- [ ] **Step 1.1: Confirm the existing flat `gh_*` inserts in `meta.rs`**

Run:
```bash
grep -n "gh_stars\|gh_forks\|gh_language\|gh_topics\|gh_is_fork\|gh_is_archived\|gh_file_type\|gh_line_start\|gh_line_end" \
  /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata/src/ingest/github/meta.rs
```

Expected: Lines showing `obj.insert("gh_stars"`, `obj.insert("gh_forks"`, `obj.insert("gh_language"`, etc. — all nine promoted fields already present as flat inserts.

- [ ] **Step 1.2: Confirm the test currently checks these as top-level keys**

Run:
```bash
grep -n "gh_stars\|gh_forks\|gh_language\|gh_file_type\|gh_line_start\|gh_line_end" \
  /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata/src/ingest/github/meta_tests.rs
```

Expected: The existing `payload_repo_metadata_null_for_file_chunks` test checks `payload["gh_stars"]` etc. at top level — confirming these assertions already exist.

---

## Task 2: Write the failing tests first (TDD)

**Files:**
- Modify: `src/ingest/github/meta_tests.rs`

Before touching `meta.rs`, write tests that will pass once the plan is complete. Specifically, add assertions that:
1. Promoted fields appear at the **top level** of the payload (already true — but add explicit assertions)
2. Promoted fields do **NOT** appear inside `git_meta` (this is what's currently broken — they're currently in both places)

- [ ] **Step 2.1: Add a test verifying promoted fields are NOT in git_meta**

Open `src/ingest/github/meta_tests.rs` and add the following test after the last existing test:

```rust
#[test]
fn promoted_fields_not_in_git_meta_blob() {
    // gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork, gh_is_archived,
    // gh_file_type, gh_line_start, gh_line_end must live at the TOP LEVEL of the
    // payload — not inside git_meta — so Qdrant can index and filter them.
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: "file".into(),
        stars: Some(42),
        forks: Some(7),
        language: Some("Rust".into()),
        topics: Some(vec!["cli".into(), "rag".into()]),
        is_fork: Some(false),
        is_archived: Some(false),
        file_type: Some("source".into()),
        gh_line_start: Some(10),
        gh_line_end: Some(50),
        ..Default::default()
    };
    let payload = build_github_payload(&params);

    // Top-level assertions — these must exist as flat keys.
    assert_eq!(payload["gh_stars"], 42, "gh_stars must be a top-level key");
    assert_eq!(payload["gh_forks"], 7, "gh_forks must be a top-level key");
    assert_eq!(payload["gh_language"], "Rust", "gh_language must be a top-level key");
    assert_eq!(
        payload["gh_topics"],
        json!(["cli", "rag"]),
        "gh_topics must be a top-level key"
    );
    assert_eq!(payload["gh_is_fork"], false, "gh_is_fork must be a top-level key");
    assert_eq!(payload["gh_is_archived"], false, "gh_is_archived must be a top-level key");
    assert_eq!(payload["gh_file_type"], "source", "gh_file_type must be a top-level key");
    assert_eq!(payload["gh_line_start"], 10, "gh_line_start must be a top-level key");
    assert_eq!(payload["gh_line_end"], 50, "gh_line_end must be a top-level key");

    // git_meta assertions — promoted fields must NOT be duplicated there.
    let meta = &payload["git_meta"];
    assert!(
        meta["stars"].is_null() || !meta.is_object(),
        "stars must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["forks"].is_null() || !meta.is_object(),
        "forks must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["language"].is_null() || !meta.is_object(),
        "language must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["topics"].is_null() || !meta.is_object(),
        "topics must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["is_fork"].is_null() || !meta.is_object(),
        "is_fork must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["is_archived"].is_null() || !meta.is_object(),
        "is_archived must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["gh_file_type"].is_null() || !meta.is_object(),
        "gh_file_type must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["gh_line_start"].is_null() || !meta.is_object(),
        "gh_line_start must not be stored in git_meta (found: {meta})"
    );
    assert!(
        meta["gh_line_end"].is_null() || !meta.is_object(),
        "gh_line_end must not be stored in git_meta (found: {meta})"
    );
}
```

- [ ] **Step 2.2: Run the new test to confirm it fails**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo test promoted_fields_not_in_git_meta_blob -- --nocapture 2>&1 | tail -20
```

Expected: FAIL — the test panics because `meta["stars"]` is currently non-null (the field IS in `git_meta`).

---

## Task 3: Remove promoted fields from `git_meta` in `build_github_payload()`

**Files:**
- Modify: `src/ingest/github/meta.rs`

The `meta:` block passed to `build_git_payload()` is what populates `git_meta`. Remove the nine promoted fields from it and keep only the lower-priority extras.

- [ ] **Step 3.1: Edit `build_github_payload()` — shrink the `meta` block**

In `src/ingest/github/meta.rs`, find the `meta: Some(json!({...}))` block (lines ~99–118) and replace it with the reduced version that only contains the fields NOT being promoted:

```rust
        meta: Some(json!({
            "open_issues":        params.open_issues,
            "is_private":         params.is_private,
            "default_branch":     params.default_branch,
            "repo_description":   params.repo_description,
            "pushed_at":          params.pushed_at,
            "gh_is_test":         params.is_test,
            "gh_file_size_bytes": params.file_size_bytes,
            "gh_comment_count":   params.comment_count,
            "gh_is_pr":           params.is_pr,
        })),
```

The promoted fields removed from `git_meta` are:
- `"stars"` (→ top-level `gh_stars`)
- `"forks"` (→ top-level `gh_forks`)
- `"language"` (→ top-level `gh_language`)
- `"topics"` (→ top-level `gh_topics`)
- `"is_fork"` (→ top-level `gh_is_fork`)
- `"is_archived"` (→ top-level `gh_is_archived`)
- `"gh_file_type"` (→ top-level `gh_file_type`)
- `"gh_line_start"` (→ top-level `gh_line_start`)
- `"gh_line_end"` (→ top-level `gh_line_end`)

Note: `default_branch`, `repo_description`, `pushed_at`, `is_private` remain in `git_meta` because they are already emitted as their own flat `gh_*` keys and the duplication in `git_meta` is harmless for non-indexed fields — but they are lower-priority and don't need indexing now.

- [ ] **Step 3.2: Verify the existing flat `obj.insert` calls still cover all nine promoted fields**

Scan lines 123–157 of `meta.rs` to confirm each of these `obj.insert` calls is present and unchanged:
```
obj.insert("gh_stars",       json!(params.stars));
obj.insert("gh_forks",       json!(params.forks));
obj.insert("gh_language",    json!(params.language));
obj.insert("gh_topics",      json!(params.topics));
obj.insert("gh_is_fork",     json!(params.is_fork));
obj.insert("gh_is_archived", json!(params.is_archived));
obj.insert("gh_file_type",   json!(params.file_type));
obj.insert("gh_line_start",  json!(params.gh_line_start));
obj.insert("gh_line_end",    json!(params.gh_line_end));
```

Run:
```bash
grep -n "gh_stars\|gh_forks\|gh_language\|gh_topics\|gh_is_fork\|gh_is_archived\|gh_file_type\|gh_line_start\|gh_line_end" \
  /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata/src/ingest/github/meta.rs
```

Expected: Nine lines, one per field, each an `obj.insert(...)` call. None inside the `json!({...})` meta block.

- [ ] **Step 3.3: Run all meta tests to verify they pass**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo test ingest::github::tests -- --nocapture 2>&1 | tail -30
```

Expected: ALL tests pass, including the new `promoted_fields_not_in_git_meta_blob` test. The existing `payload_has_gh_and_git_keys` test checks for 32 `gh_*` keys — confirm the count hasn't changed (the flat inserts still emit all 32).

- [ ] **Step 3.4: Commit**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk git add src/ingest/github/meta.rs src/ingest/github/meta_tests.rs && \
  rtk git commit -m "fix(ingest/github): remove promoted gh_* fields from git_meta blob

gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork, gh_is_archived,
gh_file_type, gh_line_start, gh_line_end were stored in both git_meta (unindexed
blob) and as flat top-level gh_* keys. Remove the git_meta copies so Qdrant
can index and filter the flat keys without redundancy.

Adds test: promoted_fields_not_in_git_meta_blob"
```

---

## Task 4: Add Qdrant payload indexes for the promoted fields

**Files:**
- Modify: `src/vector/ops/tei/qdrant_store/payload_indexes.rs`

Add keyword indexes for five fields and integer indexes for four fields.

- [ ] **Step 4.1: Add keyword index entries**

In `payload_indexes.rs`, find `KEYWORD_INDEX_FIELDS` (line ~11) and add the five new entries after `"gh_file_language"`:

```rust
const KEYWORD_INDEX_FIELDS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    "gh_file_language",
    // GitHub-specific indexed fields (top-level, not deprecated).
    "gh_language",
    "gh_file_type",
    "gh_topics",
    // NOTE: gh_is_fork and gh_is_archived use Qdrant's native "bool" index type,
    // not "keyword" — see push_non_keyword_indexes(). Do NOT add them here.
    "chunking_method",
    "extractor_name",
    // ... rest unchanged
```

The complete updated constant (preserving all existing entries, adding the five new ones directly after `"gh_file_language"`):

```rust
const KEYWORD_INDEX_FIELDS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    "gh_file_language",
    // GitHub-specific indexed fields (top-level, not deprecated).
    "gh_language",
    "gh_file_type",
    "gh_topics",
    "gh_is_fork",
    "gh_is_archived",
    "chunking_method",
    "extractor_name",
    // Shared git provider schema (all git-backed ingest sources).
    "provider",
    "git_host",
    "git_owner",
    "git_repo",
    "git_content_kind",
    "git_state",
    "git_author",
    "git_file_language",
    // Vertical extractor fields.
    "pkg_registry",
    "pkg_name",
    "pkg_language",
    "pkg_license",
    "pkg_author",
    "hf_task",
    "hf_library",
    "so_is_answered",
    "hn_type",
    "hn_author",
    "arxiv_id",
    "devto_author",
];
```

- [ ] **Step 4.2: Add integer index entries for the four numeric fields**

In `push_non_keyword_indexes()` (line ~80), find the `integer_fields` array and add four new entries:

```rust
fn push_non_keyword_indexes<'a>(futures: &mut Vec<IndexFut<'a>>, index_url: &str) {
    let integer_fields = [
        ("chunk_index", index_url.to_string()),
        ("git_number", index_url.to_string()),
        ("so_question_id", index_url.to_string()),
        ("payload_schema_version", index_url.to_string()),
        // GitHub-specific integer indexes (top-level, not deprecated).
        ("gh_stars", index_url.to_string()),
        ("gh_forks", index_url.to_string()),
        ("gh_line_start", index_url.to_string()),
        ("gh_line_end", index_url.to_string()),
    ];
    // ... rest of function unchanged
```

- [ ] **Step 4.3: Update the `futures` Vec capacity hint**

The `Vec::with_capacity` call in `ensure_payload_indexes()` (line ~54) should be bumped to reflect the new total. Currently it's `KEYWORD_INDEX_FIELDS.len() + 5` (5 = 4 integer fields + 1 datetime field). Adding 5 keyword + 4 integer = 9 new fields. New value: `KEYWORD_INDEX_FIELDS.len() + 9`:

```rust
let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(KEYWORD_INDEX_FIELDS.len() + 9);
```

- [ ] **Step 4.4: Run `cargo check` to verify no compile errors**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo check 2>&1 | tail -20
```

Expected: `Finished checking` — no errors.

- [ ] **Step 4.5: Run all vector/tei tests**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo test tei -- --nocapture 2>&1 | tail -20
```

Expected: All TEI tests pass.

- [ ] **Step 4.6: Commit**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk git add src/vector/ops/tei/qdrant_store/payload_indexes.rs && \
  rtk git commit -m "feat(vector): add Qdrant indexes for promoted gh_* fields

Add keyword indexes: gh_language, gh_file_type, gh_topics, gh_is_fork, gh_is_archived
Add integer indexes: gh_stars, gh_forks, gh_line_start, gh_line_end

These fields are now flat top-level keys in the GitHub ingest payload and
need Qdrant indexes so they are filterable and facetable."
```

---

## Task 5: Update the Qdrant payload schema contract doc

**Files:**
- Modify: `docs/contracts/qdrant-payload-schema.md`

The "GitHub backwards-compat fields" section currently says all `gh_*` fields are deprecated aliases for `git_*` fields. That's wrong — the nine promoted fields are GitHub-specific (no `git_*` equivalent) and are not deprecated. Update the section to draw this distinction clearly.

- [ ] **Step 5.1: Edit the GitHub backwards-compat section**

Find the section "### GitHub backwards-compat fields" (around line 89). Replace it with:

```markdown
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

GitHub ingest also emits additional flat `gh_*` fields that duplicate `git_*` canonical fields,
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
```

- [ ] **Step 5.2: Update the Payload Schema Version table**

Add a version 4 entry to the versioning table (around line 178):

```markdown
| 4 | 2026-05-21 | Promoted gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork, gh_is_archived, gh_file_type, gh_line_start, gh_line_end from git_meta blob to indexed top-level fields. Removed these keys from git_meta. |
```

Also update the `payload_schema_version` row in the Universal Fields table from `3` to `4`:
```markdown
| `payload_schema_version` | integer | yes | Schema version at embed time. Pre-lu6a points lack this field (implicit v1). Current: `4`. |
```

- [ ] **Step 5.3: Bump `PAYLOAD_SCHEMA_VERSION` constant in the codebase**

Find the schema version constant and bump it to 4:

```bash
grep -rn "PAYLOAD_SCHEMA_VERSION" /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata/src/ | head -10
```

Locate the definition (likely in `src/vector/ops/qdrant/utils.rs` or similar) and update it from `3` to `4`:

```rust
pub const PAYLOAD_SCHEMA_VERSION: u32 = 4;
```

- [ ] **Step 5.4: Run `cargo check` to verify the version bump compiles**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo check 2>&1 | tail -10
```

Expected: `Finished checking` — no errors.

- [ ] **Step 5.5: Commit**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk git add docs/contracts/qdrant-payload-schema.md && \
  rtk git add -u src/ && \
  rtk git commit -m "docs(contracts): clarify gh_* field status; bump schema version to 4

- Split gh_* fields into two groups: GitHub-specific indexed (not deprecated)
  and backwards-compat duplicates of git_* (deprecated)
- Document git_meta remaining contents
- Bump PAYLOAD_SCHEMA_VERSION to 4"
```

---

## Task 6: Full test suite gate

**Files:** none (verification only)

- [ ] **Step 6.1: Run the full ingest test suite**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo test ingest -- --nocapture 2>&1 | tail -30
```

Expected: All tests pass. No regressions.

- [ ] **Step 6.2: Run `just verify`**

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && just verify 2>&1 | tail -40
```

Expected: `fmt check` + `clippy` + `check` + `test` all green.

- [ ] **Step 6.3: Confirm the test count in `payload_has_gh_and_git_keys`**

The existing test asserts `gh_count == 32`. Verify this assertion still holds (we did not add any new `gh_*` keys, only changed where promoted ones are stored):

```bash
cd /home/jmagar/workspace/axon_rust/.worktrees/vertical-metadata && \
  rtk cargo test payload_has_gh_and_git_keys -- --nocapture 2>&1 | tail -10
```

Expected: PASS with `"expected 32 gh_* keys"` matching the actual count.

---

## Self-Review

### Spec Coverage Check

| Requirement | Task |
|---|---|
| Promote `gh_stars`, `gh_forks` from `git_meta` to top-level | Task 3 |
| Promote `gh_language` from `git_meta` to top-level | Task 3 |
| Promote `gh_topics` from `git_meta` to top-level | Task 3 |
| Promote `gh_is_fork`, `gh_is_archived` from `git_meta` to top-level | Task 3 |
| Promote `gh_file_type` from `git_meta` to top-level | Task 3 |
| Promote `gh_line_start`, `gh_line_end` from `git_meta` to top-level | Task 3 |
| Keep `gh_comment_count`, `gh_is_pr`, `gh_is_test`, `gh_file_size_bytes` in `git_meta` | Task 3 |
| Add keyword indexes: `gh_language`, `gh_file_type`, `gh_topics`, `gh_is_fork`, `gh_is_archived` | Task 4 |
| Add integer indexes: `gh_stars`, `gh_forks`, `gh_line_start`, `gh_line_end` | Task 4 |
| Update schema contract doc: distinguish indexed gh_* from deprecated gh_* | Task 5 |
| Update tests to verify promoted fields are top-level | Task 2 |

All requirements covered. No gaps.

### Placeholder Scan

No TBD, TODO, or "similar to Task N" placeholders. All code blocks contain complete, copy-pasteable content.

### Type Consistency

- `gh_is_fork` and `gh_is_archived` are stored as JSON booleans (`json!(params.is_fork)` where `params.is_fork: Option<bool>`). Qdrant will receive them as booleans. The keyword index type in the plan says "bool" in the table but "keyword" in `KEYWORD_INDEX_FIELDS`. **Important:** Qdrant does not have a native `bool` index type — booleans are best indexed as `keyword` with values `"true"` / `"false"`. However, `serde_json` serializes `bool` as JSON `true`/`false`, not as strings. This is fine for Qdrant filtering — Qdrant accepts JSON boolean values for keyword fields via `match: { value: true }`. The current pattern in the codebase (e.g. `gh_is_private`) is to store them as native booleans via `json!(params.is_private)`. Follow the same pattern — no string conversion needed.

- `gh_file_type` is stored as `json!(params.file_type)` where `params.file_type: Option<String>`. Qdrant will receive it as a string keyword. Correct.

- Integer fields (`gh_stars`, `gh_forks`, `gh_line_start`, `gh_line_end`) are stored as `json!(params.stars)` where `params.stars: Option<u32>`. Qdrant receives them as JSON numbers. Correct for `"integer"` index type.
