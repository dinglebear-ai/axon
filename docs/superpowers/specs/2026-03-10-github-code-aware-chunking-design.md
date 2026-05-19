# GitHub Ingestion: Code-Aware Chunking + Source by Default

**Date:** 2026-03-10
**Status:** Draft
**Scope:** `crates/vector/ops/input/`, `crates/vector/ops/tei.rs`, `crates/ingest/github/`, `crates/services/ingest.rs`, `Cargo.toml`

## Problem

GitHub ingestion currently:
1. Indexes source code only when `--include-source` is explicitly passed (default: docs only)
2. Chunks all content — prose and code — with a generic 2000-char / 200-char-overlap splitter that cuts at arbitrary character boundaries
3. Source file chunks carry no language or file-type metadata

This means most ingestions skip the most authoritative content in a repo (the source code), and when source IS included, function bodies get split mid-implementation, producing low-quality embeddings that hurt semantic search recall.

## Solution

1. **Flip the default**: source code is included by default; `--no-source` opts out
2. **Code-aware chunking**: use `text-splitter` crate with tree-sitter grammars to chunk source files at AST boundaries (functions, structs, classes, impl blocks)
3. **Richer metadata**: tag each source file chunk with language, file type, and test status
4. **Graceful fallback**: files without a matching grammar use the existing `chunk_text()` prose splitter

## Dependencies

### New Crates

```toml
text-splitter = { version = "0.29", features = ["code"] }
tree-sitter-rust = "0.24"
tree-sitter-python = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-go = "0.23"
tree-sitter-bash = "0.23"
```

Version notes:
- `text-splitter 0.29` depends on `tree-sitter 0.26`
- All grammar crates must use `tree-sitter-language 0.1.x` (which targets `tree-sitter 0.26`)
- Grammar bundle crates (`rs-tree-sitter-languages`, `tree-sitter-language-pack`) are pinned to `tree-sitter 0.22` and are **incompatible**

### Supported Languages (v1)

| Extension(s) | Grammar Crate | Notes |
|---|---|---|
| `.rs` | `tree-sitter-rust` | |
| `.py` | `tree-sitter-python` | |
| `.js`, `.jsx` | `tree-sitter-javascript` | |
| `.ts`, `.tsx` | `tree-sitter-typescript` | Uses TypeScript sub-grammar for `.ts`, TSX for `.tsx` |
| `.go` | `tree-sitter-go` | |
| `.sh`, `.bash` | `tree-sitter-bash` | Covers Bash + most POSIX shell |

All other extensions (`.c`, `.cpp`, `.java`, `.toml`, `.yaml`, `.json`, etc.) fall back to `chunk_text()`. New languages are added with one `Cargo.toml` line + one match arm.

## Architecture

### Layer Boundaries

```
CLI / MCP / Web
      │
      ▼
crates/services/ingest.rs          ← orchestration entry point (called by all surfaces)
      │
      ▼
crates/ingest/github/              ← GitHub-specific fetch + embed orchestration
  ├── files.rs                     ← file tree fetch, calls embed pipeline
  ├── meta.rs                      ← Qdrant payload builders (repo, issue, PR, file)
  ├── issues.rs                    ← issue/PR fetch + embed (unchanged)
  └── wiki.rs                      ← wiki clone + embed (unchanged)
      │
      ▼
crates/vector/ops/tei.rs           ← embed pipeline (chunk → TEI → Qdrant)
      │
      ▼
crates/vector/ops/input/           ← chunking primitives (pure, no I/O)
  ├── mod.rs                       ← re-exports chunk_text() (existing)
  └── code.rs                      ← chunk_code() + language dispatch (NEW)
```

The chunking primitive (`chunk_code`) is a pure function with no I/O — it lives in `vector/ops/input/` alongside the existing `chunk_text()`. The GitHub-specific orchestration in `ingest/github/files.rs` calls the embedding pipeline in `tei.rs`, which delegates to the appropriate chunker. The service layer in `services/ingest.rs` remains the entry point for CLI, MCP, and web consumers.

### Component 1: Code Chunker Module — `crates/vector/ops/input/code.rs`

New submodule alongside existing `input.rs`. Responsible for:
- Mapping file extensions to tree-sitter `LanguageFn`
- Constructing a `CodeSplitter` per language
- Returning `Vec<String>` chunks identical in shape to `chunk_text()`

```rust
/// Returns AST-aware chunks if a grammar exists for the extension,
/// otherwise returns None (caller falls back to chunk_text).
pub fn chunk_code(content: &str, file_extension: &str) -> Option<Vec<String>>
```

Internal implementation:

```rust
fn language_for_extension(ext: &str) -> Option<tree_sitter_language::LanguageFn> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE),
        "py" => Some(tree_sitter_python::LANGUAGE),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX),
        "go" => Some(tree_sitter_go::LANGUAGE),
        "sh" | "bash" => Some(tree_sitter_bash::LANGUAGE),
        _ => None,
    }
}
```

Chunk sizing: `500..2000` characters (range mode). `text-splitter` will produce chunks between 500 and 2000 chars, preferring to split at the largest AST node boundary that fits. This means small functions stay intact as single chunks, and large functions get split at inner statement boundaries rather than mid-line.

### Component 2: Embedding Pipeline Integration — `crates/vector/ops/tei.rs`

New function alongside `embed_text_with_metadata` and `embed_text_with_extra_payload`:

```rust
pub async fn embed_code_with_metadata(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    file_extension: &str,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>>
```

Logic:
1. Try `chunk_code(content, file_extension)`
2. If `Some(chunks)` → use those
3. If `None` → fall back to `chunk_text(content)`
4. Rest of pipeline (TEI embed, Qdrant upsert, stale tail cleanup) is unchanged

### Component 3: Unified GitHub Payload — `crates/ingest/github/meta.rs`

**All GitHub chunks share the same flat payload schema.** Non-applicable fields are null. This replaces the current per-type builders (`build_github_repo_extra_payload`, `build_github_issue_extra_payload`, `build_github_pr_extra_payload`) with a single unified builder.

```rust
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value
```

`GitHubPayloadParams` is a struct with all fields as `Option`, populated by each caller with what it knows:

```rust
pub struct GitHubPayloadParams {
    // ── Common (all chunk types) ──
    pub repo: String,                    // "owner/repo"
    pub owner: String,                   // "owner"
    pub branch: Option<String>,          // "main" — files/wiki; null for issues/PRs
    pub default_branch: Option<String>,  // always set — the repo's default branch
    pub content_kind: String,            // "file" | "issue" | "pr" | "wiki" | "repo_metadata"
    pub repo_description: Option<String>,// repo description — on ALL chunks for LLM grounding
    pub pushed_at: Option<String>,       // RFC3339 — repo's last push timestamp (staleness signal)
    pub is_private: Option<bool>,        // repo visibility — for access-scoped queries

    // ── Repo metadata ──
    pub stars: Option<u32>,
    pub forks: Option<u32>,
    pub open_issues: Option<u32>,
    pub language: Option<String>,        // repo primary language
    pub topics: Option<Vec<String>>,
    pub is_fork: Option<bool>,
    pub is_archived: Option<bool>,

    // ── Issue / PR ──
    pub issue_number: Option<u64>,
    pub state: Option<String>,           // "open" | "closed"
    pub author: Option<String>,
    pub created_at: Option<String>,      // RFC3339
    pub updated_at: Option<String>,      // RFC3339
    pub comment_count: Option<u32>,
    pub labels: Option<Vec<String>>,
    pub is_pr: Option<bool>,
    pub merged_at: Option<String>,       // RFC3339, PRs only
    pub is_draft: Option<bool>,          // PRs only

    // ── File ──
    pub file_path: Option<String>,       // repo-relative path
    pub file_language: Option<String>,   // "rust", "python", etc.
    pub file_type: Option<String>,       // "source" | "test" | "config" | "doc"
    pub is_test: Option<bool>,
    pub file_size_bytes: Option<u64>,    // raw file size — filter trivial/generated files
    pub chunking_method: Option<String>, // "tree-sitter" | "prose"
}
```

Output JSON uses `gh_` prefixed keys. Null `Option` fields are included as `null` values so every chunk has the same key set:

```json
{
    "gh_repo": "rust-lang/rust",
    "gh_owner": "rust-lang",
    "gh_branch": "main",
    "gh_default_branch": "main",
    "gh_content_kind": "file",
    "gh_repo_description": "Empowering everyone to build reliable and efficient software.",
    "gh_pushed_at": "2026-03-10T12:34:56Z",
    "gh_is_private": false,
    "gh_stars": null,
    "gh_forks": null,
    "gh_open_issues": null,
    "gh_language": null,
    "gh_topics": null,
    "gh_is_fork": null,
    "gh_is_archived": null,
    "gh_issue_number": null,
    "gh_state": null,
    "gh_author": null,
    "gh_created_at": null,
    "gh_updated_at": null,
    "gh_comment_count": null,
    "gh_labels": null,
    "gh_is_pr": null,
    "gh_merged_at": null,
    "gh_is_draft": null,
    "gh_file_path": "src/lib.rs",
    "gh_file_language": "rust",
    "gh_file_type": "source",
    "gh_is_test": false,
    "gh_file_size_bytes": 4280,
    "gh_chunking_method": "tree-sitter"
}
```

#### File type classification heuristics

| `gh_file_type` | Heuristic |
|---|---|
| `"test"` | Path contains `test/`, `tests/`, `__tests__/`, `_test.rs`, `_test.go`, `test_*.py`, `*.test.ts`, `*.spec.ts` |
| `"config"` | Extensions `.toml`, `.yaml`, `.yml`, `.json` or filenames like `Cargo.toml`, `package.json`, `tsconfig.json` |
| `"doc"` | Extensions `.md`, `.mdx`, `.rst`, `.txt` |
| `"source"` | Everything else |

#### Language name mapping

| Extension | `gh_file_language` |
|---|---|
| `.rs` | `"rust"` |
| `.py` | `"python"` |
| `.js`, `.jsx` | `"javascript"` |
| `.ts`, `.tsx` | `"typescript"` |
| `.go` | `"go"` |
| `.sh`, `.bash` | `"shell"` |
| `.toml` | `"toml"` |
| `.yaml`, `.yml` | `"yaml"` |
| `.json` | `"json"` |
| `.md`, `.mdx` | `"markdown"` |
| other | extension as-is |

### Component 4: Default Flip — `crates/cli/commands/ingest.rs`

- `include_source` defaults to `true`
- New `--no-source` flag to opt out: `--no-source` sets `include_source = false`
- Existing `--include-source` flag becomes a no-op (already the default) but kept for backward compat

### Component 5: `files.rs` Integration

`embed_files()` in `crates/ingest/github/files.rs` changes:
- Extract file extension from path
- Call `embed_code_with_metadata()` instead of `embed_text_with_metadata()`
- Build and pass `build_github_file_extra_payload()` as extra payload

## Data Flow

```
GitHub repo
  │
  ├─ repos().get() → repo_info                         ← EXISTING (already done)
  │    extracts: owner, name, default_branch,
  │              description, pushed_at, private        ← NEW (read from repo_info)
  │    builds: GitHubCommonFields struct                ← NEW (passed to ALL sub-tasks)
  │
  ├─ tokio::join! (5 concurrent pipelines):
  │
  │  ├─ files: fetch tree → for each file:
  │  │    ├─ fetch raw content (existing)
  │  │    ├─ detect extension
  │  │    ├─ chunk_code(content, ext) → AST chunks      ← NEW
  │  │    │   └─ fallback: chunk_text(content)           ← EXISTING
  │  │    ├─ build_github_payload(common + file fields)  ← NEW
  │  │    └─ embed chunks + unified payload → Qdrant     ← MODIFIED
  │  │
  │  ├─ repo metadata:
  │  │    └─ build_github_payload(common + repo fields)  ← MODIFIED (was per-type builder)
  │  │
  │  ├─ issues:
  │  │    └─ build_github_payload(common + issue fields) ← MODIFIED (was per-type builder)
  │  │
  │  ├─ PRs:
  │  │    └─ build_github_payload(common + PR fields)    ← MODIFIED (was per-type builder)
  │  │
  │  └─ wiki:
  │       └─ build_github_payload(common + wiki fields)  ← MODIFIED (was no extra payload)
```

## Qdrant Payload (All GitHub Chunks)

### Base fields (set by `embed_text_impl`, unchanged):

| Field | Value |
|---|---|
| `url` | source URL |
| `domain` | `"github.com"` |
| `source_type` | `"github"` |
| `source_command` | `"github"` |
| `content_type` | `"text"` |
| `chunk_index` | 0, 1, 2... |
| `chunk_text` | chunk content |
| `scraped_at` | RFC3339 timestamp |
| `title` | optional string |

### GitHub fields (set by `build_github_payload`, same keys on ALL chunk types):

#### Common (always set on every chunk)

| Field | Type | Source | Notes |
|---|---|---|---|
| `gh_repo` | string | `repos().get()` | `"owner/repo"` — primary filter key |
| `gh_owner` | string | `repos().get()` | `"owner"` |
| `gh_branch` | string | files, wiki callers | null for issues/PRs/repo_metadata |
| `gh_default_branch` | string | `repos().get()` | always set — the repo's default branch |
| `gh_content_kind` | string | each caller | `"file"` / `"issue"` / `"pr"` / `"wiki"` / `"repo_metadata"` |
| `gh_repo_description` | string | `repos().get()` | on ALL chunks — helps LLM ground answers |
| `gh_pushed_at` | string | `repos().get()` | RFC3339 — staleness signal for re-ingestion |
| `gh_is_private` | bool | `repos().get()` | repo visibility — for access-scoped queries |

#### Repo metadata

| Field | Type | Applies to | Null when |
|---|---|---|---|
| `gh_stars` | u32 | repo_metadata | everything else |
| `gh_forks` | u32 | repo_metadata | everything else |
| `gh_open_issues` | u32 | repo_metadata | everything else |
| `gh_language` | string | repo_metadata | everything else |
| `gh_topics` | string[] | repo_metadata | everything else |
| `gh_is_fork` | bool | repo_metadata | everything else |
| `gh_is_archived` | bool | repo_metadata | everything else |

#### Issue / PR

| Field | Type | Applies to | Null when |
|---|---|---|---|
| `gh_issue_number` | u64 | issues, PRs | files, wiki, repo_metadata |
| `gh_state` | string | issues, PRs | files, wiki, repo_metadata |
| `gh_author` | string | issues, PRs | files, wiki, repo_metadata |
| `gh_created_at` | string | issues, PRs, repo_metadata | files, wiki |
| `gh_updated_at` | string | issues, PRs | files, wiki, repo_metadata |
| `gh_comment_count` | u32 | issues, PRs | files, wiki, repo_metadata |
| `gh_labels` | string[] | issues, PRs | files, wiki, repo_metadata |
| `gh_is_pr` | bool | issues, PRs | files, wiki, repo_metadata |
| `gh_merged_at` | string | PRs | everything else |
| `gh_is_draft` | bool | PRs | everything else |

#### File

| Field | Type | Applies to | Null when |
|---|---|---|---|
| `gh_file_path` | string | files | issues, PRs, wiki, repo_metadata |
| `gh_file_language` | string | files | issues, PRs, wiki, repo_metadata |
| `gh_file_type` | string | files | issues, PRs, wiki, repo_metadata |
| `gh_is_test` | bool | files | issues, PRs, wiki, repo_metadata |
| `gh_file_size_bytes` | u64 | files | issues, PRs, wiki, repo_metadata |
| `gh_chunking_method` | string | files | issues, PRs, wiki, repo_metadata |

**Total: 29 `gh_*` fields.** Every chunk carries all 29 keys. Non-applicable fields are null.

## What Does NOT Change

- `chunk_text()` in `input.rs` — untouched, still the prose chunker
- `embed_text_impl()` internals (point ID generation, upsert, stale tail cleanup)
- TEI batch size handling, retry logic
- Issue/PR/wiki **content** formatting (what gets embedded as text)
- All existing CLI flags except `--include-source` default

## What Changes Beyond New Code

- **`meta.rs`**: three per-type builders replaced by one `build_github_payload()` with `GitHubPayloadParams`
- **`github.rs`**: `ingest_github()` passes `owner`, `name`, `default_branch` to all sub-tasks so they can populate the common fields
- **`issues.rs`**: switches from `build_github_issue_extra_payload` / `build_github_pr_extra_payload` to `build_github_payload` with common + issue/PR-specific fields
- **`wiki.rs`**: switches from `embed_text_with_metadata` to `embed_text_with_extra_payload` using `build_github_payload` with `content_kind: "wiki"`
- **`files.rs`**: switches from `embed_text_with_metadata` to `embed_code_with_metadata` using `build_github_payload` with file-specific fields

## Testing Strategy

### Unit Tests

1. **`language_for_extension()`** — all supported extensions return `Some`, unknown returns `None`
2. **`chunk_code()`** — Rust source with multiple functions produces one chunk per function (when each fits in 2000 chars)
3. **`chunk_code()` fallback** — unknown extension returns `None`
4. **`classify_file_type()`** — test paths, config paths, doc paths, source paths
5. **`is_test_path()`** — `tests/foo.rs`, `src/foo_test.go`, `__tests__/bar.ts`, `test_baz.py`
6. **`build_github_file_extra_payload()`** — correct JSON shape
7. **`chunk_code()` large function** — function body > 2000 chars splits at inner statement boundaries, not mid-line

### Integration Tests

8. **Round-trip**: chunk a known Rust file → verify chunks reassemble to cover all content (no dropped lines)
9. **Fallback path**: `.yaml` file goes through `chunk_text()`, produces expected chunk count

## Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Grammar crate version drift vs `tree-sitter 0.26` | Pin exact versions in `Cargo.toml`, verify in CI |
| Compile time increase from 6 grammar crates | Grammars are pre-compiled C; actual compile overhead is ~30-60s, not minutes |
| `text-splitter` produces empty chunks for malformed code | Filter empty chunks before embedding (same guard as existing pipeline) |
| `tree-sitter-typescript` exposes two sub-grammars (TS + TSX) | Extension-based dispatch: `.ts` → `LANGUAGE_TYPESCRIPT`, `.tsx` → `LANGUAGE_TSX` |
| Binary size increase | Grammar crates add ~2-5MB total — acceptable for a server binary |

### Component 6: Scheduled GitHub Re-Ingestion — `axon refresh schedule`

Extends the existing `RefreshSchedule` system to support GitHub repo targets alongside URLs.

#### How It Works

The existing refresh system has `RefreshSchedule` rows with `seed_url` and `urls_json` fields, a `claim_due_refresh_schedules()` poller, and a worker that re-fetches and re-embeds URLs. We extend this to recognize `github:owner/repo` as a schedule target.

#### User Interface

```bash
# Schedule a repo for automatic re-indexing every 6 hours
axon refresh schedule github:rust-lang/rust --every 6h

# Schedule with a custom name
axon refresh schedule github:anthropics/claude-code --every 24h --name claude-code-sync

# List schedules (shows both URL and GitHub schedules)
axon refresh schedule list

# Disable/enable
axon refresh schedule disable claude-code-sync
axon refresh schedule enable claude-code-sync

# Delete
axon refresh schedule delete claude-code-sync
```

#### Implementation

**New field on `RefreshSchedule`:**

```sql
ALTER TABLE axon_refresh_schedules ADD COLUMN source_type TEXT;
-- NULL = URL refresh (existing behavior), 'github' = GitHub repo refresh
```

**New field on `RefreshScheduleCreate`:**

```rust
pub struct RefreshScheduleCreate {
    // ... existing fields ...
    pub source_type: Option<String>,    // None = URL, Some("github") = GitHub repo
    pub target: Option<String>,         // "owner/repo" for GitHub schedules
}
```

**Schedule tick logic** (in `handle_refresh_schedule_run_due`):

When a claimed schedule has `source_type = Some("github")`:

1. Call GitHub API: `GET /repos/{owner}/{repo}` → check `pushed_at`
2. Compare `pushed_at` against `last_run_at` on the schedule
3. If `pushed_at > last_run_at` → enqueue an ingest job (not a refresh job)
4. If `pushed_at <= last_run_at` → skip, update `last_run_at`, log "no changes"

This is a lightweight check — one API call per scheduled repo per tick. The actual re-ingestion only happens when the repo has been pushed to since last ingest.

**Why ingest job, not refresh job:**

A GitHub re-index isn't "re-fetch these URLs" (what refresh does). It's "re-run the full ingest pipeline" — re-fetch the file tree, re-check issues/PRs, re-clone wiki. So the schedule tick enqueues via `enqueue_ingest_job()` into the existing ingest queue, reusing all the ingest worker infrastructure.

#### Data Flow

```
axon_refresh_schedules (source_type='github', target='owner/repo')
  │
  ├─ claim_due_refresh_schedules() picks up due rows (existing poller)
  │
  ├─ source_type == 'github'?
  │    ├─ YES → GET /repos/owner/repo → compare pushed_at vs last_run_at
  │    │         ├─ changed → enqueue_ingest_job(cfg, IngestSource::GitHub { repo })
  │    │         └─ unchanged → skip, update last_run_at
  │    └─ NO → existing URL refresh logic (unchanged)
```

#### What This Does NOT Do

- No webhook endpoint (could add later for real-time)
- No per-file diff detection (full re-ingest on any push)
- No partial re-ingest (re-indexes everything — files, issues, PRs, wiki)
- No issue/PR-only refresh (pushed_at only reflects code pushes, not issue activity)

#### Known Limitation: `pushed_at` vs Issue Activity

GitHub's `pushed_at` only updates on git pushes — not on issue/PR creation, comments, or label changes. This means a repo with active issues but no code changes won't trigger a re-ingest. This is acceptable for v1 — code freshness is the primary signal. A future enhancement could also check `updated_at` on the repo (which reflects all activity) or poll the Events API.

## Future Extensions (Not In Scope)

- Additional grammar crates (C, C++, Java, Kotlin, Ruby, etc.) — one match arm + one dep each
- Token-based chunk sizing (tiktoken/HF tokenizers) instead of character-based
- Issue/PR comment ingestion
- Release/changelog ingestion
- `comrak` for markdown-specific AST chunking (heading-aware splits)
- Webhook endpoint (`/webhook/github`) for real-time re-ingestion on push
- `updated_at` polling for issue/PR activity detection
- Per-file diff detection (only re-embed changed files)
