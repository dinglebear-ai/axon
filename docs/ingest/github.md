# GitHub Ingest
Last Modified: 2026-05-03

> CLI reference (flags, subcommands, examples): [`docs/commands/ingest.md`](../commands/ingest.md)

Ingests a GitHub repository — source code, documentation, issues, pull requests, and wiki pages — into Qdrant via a hybrid approach: shallow `git clone` for repository files, octocrab for metadata/issues/PRs, and a separate shallow `git clone` for wiki pages.

## What Gets Indexed

| Content | Condition |
|---------|-----------|
| Source code files | **By default**: `.rs`, `.py`, `.go`, `.ts`, `.js`, `.tsx`, `.jsx`, `.toml`, `.c`, `.cpp`, `.h`, `.hpp`, `.java`, `.kt`, `.rb`, `.php`, `.sh`, `.yaml`, `.yml`, `.json`, `.swift`, `.cs`. Disable with `--no-source`. |
| Documentation files | Always: `.md`, `.mdx`, `.rst`, `.txt` |
| Issues | Open and closed, title + body |
| Pull requests | Open and closed, title + body |
| Wiki pages | When the repo has a public wiki |

**Excluded** regardless of flags: `target/`, `node_modules/`, `dist/`, `__pycache__/`, `.lock` files, `-lock.json` files. See `is_indexable_source_path()` in `src/ingest/github.rs` for the full list.

### Code Chunking (tree-sitter AST)

Source code files are chunked via **tree-sitter AST-aware splitting** when a grammar is available. This produces chunks aligned to function, struct, class, and method boundaries (500–2000 chars) instead of arbitrary character splits.

| Language | Grammar crate |
|----------|--------------|
| Rust | `tree-sitter-rust` |
| Python | `tree-sitter-python` |
| JavaScript | `tree-sitter-javascript` |
| TypeScript / TSX | `tree-sitter-typescript` |
| Go | `tree-sitter-go` |
| Bash / shell | `tree-sitter-bash` |

Files in unsupported languages fall back to standard 2000-char prose chunking with 200-char overlap.

Implementation: `chunk_code()` in `src/vector/ops/input/code.rs`, used to pre-chunk code files before passing to `embed_prepared_docs()` via `PreparedDoc`.

### File Classification

Each file is classified by `classify_file_type()` in `src/vector/ops/input/classify.rs`:

| Type | Detection |
|------|-----------|
| `test` | Path contains `test/`, `tests/`, `__tests__/`, or filename matches `*_test.*`, `*_spec.*`, `test_*.*` |
| `config` | Known config filenames: `Cargo.toml`, `package.json`, `tsconfig.json`, `.eslintrc.*`, etc. |
| `doc` | Extensions: `.md`, `.mdx`, `.rst`, `.txt` |
| `source` | Everything else |

Classification is stored in the `gh_file_type` metadata field on each chunk.

## Prerequisites

A running Qdrant + TEI stack. `GITHUB_TOKEN` is optional for public repositories and required for private repositories.

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GITHUB_TOKEN` | Optional | Personal access token (classic) with `repo` scope, or fine-grained token with repository contents access. Required for private repos. For public repos it is used for metadata/issues/PR API calls and authenticated clone first; if that clone fails and the repo is public, axon retries clone without the token. |
| `AXON_COLLECTION` | Optional | Qdrant collection name (default: `cortex`) |
| `TEI_URL` | Required | TEI embedding service URL |

```bash
# ~/.axon/.env
GITHUB_TOKEN=ghp_your_token_here
```

## URL / Name Parsing

The argument accepts:
- `owner/repo` — canonical form
- `https://github.com/owner/repo` — full URL (prefix stripped)
- `https://github.com/owner/repo.git` — `.git` suffix stripped

## How It Works

1. Validates and normalizes `owner/repo` from the input
2. Fetches repo metadata via `GET /repos/{owner}/{repo}` — builds `GitHubCommonFields` (owner, name, description, default branch, pushed_at, is_private)
3. Clones the repository with `git clone --depth=1 --branch <default_branch> --single-branch`
4. Walks the local clone and filters files through `is_indexable_doc_path()` (always) and `is_indexable_source_path()` (unless `--no-source`)
5. Reads file contents from disk in bounded parallel batches
6. Code files are chunked via `chunk_code()` (tree-sitter AST when available, prose fallback); doc files use `chunk_text()`. Chunks are embedded through `embed_prepared_docs()` in batches
7. Fetches issues (all states) and PRs (all states) via octocrab with automatic pagination
8. Clones the wiki separately via `git clone --depth=1` and walks `.md`/`.rst`/`.txt` files when GitHub reports a wiki exists
9. All chunk types carry unified `gh_*` metadata payload via `build_github_payload()` in `src/ingest/github/meta.rs`

### Clone Authentication and Fallback

When `GITHUB_TOKEN` is set, repository file ingest tries an authenticated clone first.

- Private repositories never retry unauthenticated after an authenticated clone failure.
- Repositories with unknown visibility skip unauthenticated retry when the clone error is an auth or permission failure.
- Public repositories may retry unauthenticated because stale or over-scoped local credentials should not block public source ingestion.

Clone error messages redact the configured token before returning/logging.

### File Embed Failure Accounting

File embedding flushes `PreparedDoc` batches to the vector pipeline. If a batch fails, axon records:

- `embed_batches_failed`
- `embed_files_failed`
- `embed_docs_failed`
- `embed_chunks_failed`
- `file_read_failures`

The current threshold is fixed and conservative: any failed embed batch makes the GitHub files sub-task fail after reporting the counters. File read/stat failures are counted and skipped because they usually represent unreadable local paths or files filtered after clone; they do not by themselves fail the sub-task.

## Qdrant Metadata Fields

All GitHub chunks carry a **unified** set of 31 `gh_*` payload fields built by `build_github_payload()` in `src/ingest/github/meta.rs` via the `GitHubPayloadParams` struct. Every chunk type gets the same field schema — unused fields are set to `null`/`""` /`[]`/`0`/`false`.

### Repository-level fields (all chunk types)

| Field | Type | Description |
|-------|------|-------------|
| `gh_owner` | `string` | Repository owner |
| `gh_repo` | `string` | Repository name |
| `gh_repo_slug` | `string` | `owner/repo` canonical form |
| `gh_default_branch` | `string` | Default branch name |
| `gh_stars` | `integer` | Stargazer count at index time |
| `gh_forks` | `integer` | Fork count at index time |
| `gh_open_issues` | `integer` | Open issue count at index time |
| `gh_language` | `string \| null` | Primary language as reported by GitHub |
| `gh_topics` | `string[]` | Repository topics array |
| `gh_created_at` | `string \| null` | Repository creation timestamp (RFC 3339) |
| `gh_pushed_at` | `string \| null` | Last push timestamp (RFC 3339) |
| `gh_is_fork` | `boolean` | Whether the repository is a fork |
| `gh_is_archived` | `boolean` | Whether the repository is archived |
| `gh_is_private` | `boolean` | Whether the repository is private |
| `gh_description` | `string` | Repository description |

### File-specific fields (code, doc, wiki chunks)

| Field | Type | Description |
|-------|------|-------------|
| `gh_file_path` | `string` | Relative file path within the repo |
| `gh_file_language` | `string` | Human-readable language name (from extension) |
| `gh_file_type` | `string` | `"test"`, `"config"`, `"doc"`, or `"source"` (from `classify_file_type()`) |
| `gh_is_test` | `boolean` | Whether the file is a test file |
| `gh_file_size_bytes` | `integer` | File size in bytes |
| `gh_chunking_method` | `string` | `"tree-sitter"` or `"prose"` — how the file was chunked |

### Issue/PR fields

| Field | Type | Description |
|-------|------|-------------|
| `gh_issue_number` | `integer` | GitHub issue/PR number |
| `gh_state` | `string` | `"open"`, `"closed"`, or `"unknown"` |
| `gh_author` | `string` | Login of the author |
| `gh_updated_at` | `string \| null` | Last-updated timestamp (RFC 3339) |
| `gh_comment_count` | `integer` | Number of comments |
| `gh_labels` | `string[]` | Label names |
| `gh_is_pr` | `boolean` | `true` for pull requests, `false` for issues |
| `gh_merged_at` | `string \| null` | Merge timestamp; `null` if not merged or if issue |
| `gh_is_draft` | `boolean` | Whether the PR was a draft at index time |

## Known Limitations

| Limitation | Detail |
|-----------|--------|
| **Rate limits without token** | File content ingestion uses git clone and does not make one API request per file. Metadata, issues, and PRs still use GitHub API calls and are rate-limited without `GITHUB_TOKEN`. |
| **Private repos** | Require a token with `repo` (classic) or `contents:read` (fine-grained) scope |
| **Very large repos** | Clone + local walk avoids per-file API calls, but large repositories still take time to clone, walk, chunk, and embed. |
| **Binary files** | Excluded by extension list. The list is hardcoded; PRs welcome for additions. |
| **Forked repos** | Ingests the fork only, not upstream. |
| **AST chunking coverage** | Only Rust, Python, JavaScript, TypeScript, Go, and Bash have tree-sitter grammars. Other languages fall back to prose chunking. |

## Troubleshooting

**`403 Forbidden` / rate limit errors**

Set `GITHUB_TOKEN` in `~/.axon/.env`. Verify the token has `contents:read` access (fine-grained) or `repo` scope (classic).

**`repository not found`**

Repo is private or doesn't exist. Check the owner/repo spelling and token permissions.

**Slow ingestion on large repos**

Expected — clone, local walk, chunking, and TEI embedding can take minutes for large repositories. Consider skipping source code (`--no-source`) when you only need docs/issues/PR metadata.
