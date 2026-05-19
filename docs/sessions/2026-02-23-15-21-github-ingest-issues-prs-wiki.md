# Session: GitHub Ingest — Issues, PRs, Wiki, Repo Metadata

**Date**: 02/23/2026 15:21 EST
**Branch**: `fix-crawl`
**Session type**: Feature implementation

---

## Session Overview

Implemented the full GitHub ingest pipeline as specified in the plan. `ingest_github()` previously only fetched file tree + file contents. This session closes the gap by adding: repo metadata, all issues (open+closed), all pull requests (open+closed), and wiki pages (via `git clone`).

The existing `crates/ingest/github.rs` (386 lines) was split into a proper module to stay under the 500-line monolith limit.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read plan, reviewed existing `github.rs` and `youtube.rs` for patterns |
| +5 min | Verified octocrab 0.44 API via Context7 (pagination, builder, repos.get()) |
| +8 min | Checked `docker/Dockerfile` — confirmed `git` was missing from runtime stage |
| +10 min | Created `crates/ingest/github/files.rs` (moved existing logic) |
| +12 min | Created `crates/ingest/github/issues.rs` (new octocrab pagination) |
| +14 min | Created `crates/ingest/github/wiki.rs` (git clone subprocess) |
| +16 min | Created `crates/ingest/github/mod.rs` (orchestration + pure logic + all 22 tests) |
| +17 min | Deleted `crates/ingest/github.rs` |
| +18 min | `cargo check` → clean, `cargo test ingest::github` → 22/22 pass |
| +20 min | Fixed `clippy::collapsible_str_replace` in `wiki.rs` |
| +22 min | Fixed `cargo fmt` whitespace in `wiki.rs` |
| +24 min | `cargo test --lib` → 337/337 pass, `just verify` → clean (pre-existing doctest failure in `status.rs` unrelated) |
| +25 min | Added `git` to `docker/Dockerfile` runtime `apt-get install` |
| +26 min | Updated `docs/ingest/github.md` |

---

## Key Findings

- **octocrab 0.44 API**: `Octocrab::builder().personal_token(token).build()?` compiles clean; `build()` returns `Result<Octocrab>` which coerces to `Box<dyn Error>` via `?`
- **Issues API returns PRs**: GitHub's Issues API returns both issues and PRs in one endpoint. Filter via `issue.pull_request.is_some()` to avoid double-embedding.
- **`git` missing from Dockerfile**: The runtime image only had `curl`, `bash`, `ca-certificates`, `libssl3`, `xz-utils`. `git` was not present — would have caused wiki clone to fail silently (mapped to `Ok(0)` since non-zero exit is "no wiki" signal, but would fail for actual wikis too with "git not found" error).
- **`octocrab::Page<T>` iteration**: `for item in &page` iterates by reference. `octo.get_page::<T>(&page.next)` gives `Option<Page<T>>` for pagination.
- **pre-existing doctest failure**: `crates/jobs/status.rs:12` has a broken doctest with async code in a non-async context. This was failing before this session. All 337 lib tests pass.

---

## Technical Decisions

### Hybrid client strategy (raw reqwest + octocrab)
- **Raw reqwest** for file tree + raw content: already working, reliable, avoids octocrab overhead for simple GET requests.
- **octocrab** for issues, PRs, repo metadata: typed responses, built-in pagination, `all_pages()` helper available.
- **`git clone` subprocess** for wiki: No GitHub REST API exists for wiki pages; octocrab doesn't support it either. Same subprocess pattern as `youtube.rs` (yt-dlp).

### Single `repos().get()` fetch before `tokio::join!`
The `GET /repos/{owner}/{name}` response provides both `default_branch` (needed by `files.rs`) and the full `Repository` struct (needed by `embed_repo_metadata()`). Fetching once and passing the result avoids a duplicate API call.

### Wiki failure = `Ok(0)`, not an error
`git clone` returning non-zero exit most commonly means the repo has no wiki. Treating this as an error would pollute logs for every repo without a wiki. The plan explicitly calls for silent `Ok(0)`.

### Individual failures don't abort the run
Each of the five sub-tasks (files, metadata, issues, prs, wiki) returns its result independently. Failures are logged via `log_warn` and counted as 0. A single transient API error shouldn't abort a large ingest that's 90% complete.

### `ingest_wiki` tempdir lifetime
`let _tmp = tempfile::tempdir()?;` — the `_tmp` binding keeps the tempdir alive for the function's duration. When the function returns, `_tmp` is dropped and the directory is cleaned up. Same pattern as `youtube.rs`.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/ingest/github.rs` | Deleted | Replaced by module directory |
| `crates/ingest/github/mod.rs` | Created (336 lines) | Orchestration, pure logic, 22 tests |
| `crates/ingest/github/files.rs` | Created (153 lines) | Moved file tree + raw content fetching |
| `crates/ingest/github/issues.rs` | Created (111 lines) | octocrab issues + PRs pagination |
| `crates/ingest/github/wiki.rs` | Created (84 lines) | git clone subprocess + file walk |
| `docker/Dockerfile` | Modified | Added `git` to runtime apt-get install |
| `docs/ingest/github.md` | Modified | Updated description and How It Works step 5 |

---

## Commands Executed

```bash
# Verify compilation
cargo check
# → Finished `dev` profile in 8.38s (clean)

# Run github-specific tests
cargo test ingest::github
# → 22 passed; 0 failed (all pure logic tests)

# Run full lib test suite
cargo test --lib
# → 337 passed; 0 failed; finished in 0.23s

# Clippy (after collapsible_str_replace fix)
cargo clippy
# → 0 errors, 0 warnings

# Format check (after wiki.rs fix)
cargo fmt --check
# → clean

# Full verify gate
just verify
# → 337 lib tests pass; 1 pre-existing doctest failure in status.rs (unrelated)
```

---

## Behavior Changes (Before → After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon github owner/repo` | Fetches file tree + content only | Also fetches repo metadata, all issues, all PRs, wiki pages |
| Repo metadata | Not embedded | Embedded as single doc: description, language, topics, license, stars |
| Issues | Not embedded | All open + closed issues with title, body, labels embedded |
| PRs | Not embedded | All open + closed PRs with title, body embedded |
| Wiki | Not embedded | Cloned via `git clone --depth=1`, all .md/.rst/.txt pages embedded |
| Container wiki support | Would fail silently (git not found → non-zero exit → Ok(0)) | `git` installed, actual wikis clone successfully |
| Module structure | Single 386-line `github.rs` | 4-file module: mod.rs, files.rs, issues.rs, wiki.rs |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | Finished in 8.38s | ✅ Pass |
| `cargo test ingest::github` | 22/22 pass | 22 passed; 0 failed | ✅ Pass |
| `cargo test --lib` | All lib tests pass | 337 passed; 0 failed | ✅ Pass |
| `cargo clippy` | 0 errors | 0 errors | ✅ Pass |
| `cargo fmt --check` | Clean | Clean | ✅ Pass |
| `./scripts/monolith` | Each file ≤ 500 lines | mod.rs 336, files.rs 153, issues.rs 111, wiki.rs 84 | ✅ Pass |

---

## Source IDs + Collections Touched

No Qdrant embed operations were performed during this session (code change only, no live services running).

---

## Risks and Rollback

**Risks:**
- `tokio::join!` runs all 5 sub-tasks concurrently — if the GitHub API rate limits, multiple tasks will fail simultaneously rather than sequentially. Mitigated: failures log + continue (`Ok(0)`), not abort.
- Wiki clone embeds the `.git` directory path as part of the temp path — this is excluded by the `.md`/`.rst`/`.txt` extension filter, so no `.git` files are embedded.
- octocrab `build()` with no token creates an unauthenticated instance (60 req/hr). Large repos may hit rate limits before finishing issues/PRs.

**Rollback:**
```bash
git checkout crates/ingest/github.rs   # restore old single-file version
git rm -r crates/ingest/github/        # remove new module
git checkout docker/Dockerfile         # remove git from apt-get
git checkout docs/ingest/github.md     # restore old docs
```

---

## Decisions Not Taken

| Alternative | Why rejected |
|-------------|-------------|
| `octocrab::all_pages()` for automatic pagination | Manual loop chosen to allow per-item error handling (`log_warn` + continue) without aborting on first failure |
| `tokio::spawn` for true parallelism | `tokio::join!` is sufficient since all tasks are I/O-bound; avoids lifetime complexity of moving data across task boundaries |
| Using octocrab for file content too | Raw reqwest approach already works and is simpler; octocrab's `repos().get_content()` returns base64-encoded data requiring decode step |
| Separate `--include-wiki`, `--include-issues`, `--include-prs` flags | Plan specifies these are unconditionally enabled (matching existing docs); no new flags needed |

---

## Open Questions

- **octocrab unauthenticated rate limit for issues/PRs**: For repos with hundreds of issues, the 60 req/hr unauthenticated limit may exhaust mid-pagination. No retry/backoff was added to octocrab calls. Future: add `Retry-After` header handling.
- **Wiki for private repos**: The token is injected into the clone URL as `https://{token}@github.com/...`. This works but exposes the token in process listing. Alternative: git credential helper via env vars. Not blocking for current use.
- **`axon ingest errors <uuid>` still unhandled**: Known gap documented in `crates/ingest/CLAUDE.md`. Not addressed in this session.

---

## Next Steps

- [ ] Test end-to-end against a real repo with live services: `./scripts/axon github jmagar/axon_rust --wait true`
- [ ] Consider adding retry/backoff for octocrab pagination calls (rate limit resilience)
- [ ] Fix pre-existing doctest in `crates/jobs/status.rs:12` (async code in non-async doctest)
- [ ] Address `axon ingest errors <uuid>` gap in `ingest_jobs.rs`
