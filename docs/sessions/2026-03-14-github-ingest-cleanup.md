# Session: GitHub Ingest Code Review Cleanup
Date: 2026-03-14

## Session Overview

Executed all 7 tasks from the GitHub ingest code review plan (`docs/superpowers/plans/2026-03-14-github-ingest-cleanup.md`). All changes were applied directly (no subagents) to `crates/ingest/github/` — addressing monolith violations, silent error swallows, redundant allocations, and minor cleanups. Two commits pushed to `origin/main`.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Skill invoked: `/superpowers:subagent-driven-development` with plan file |
| Early | Subagent dispatch rejected by user — switched to direct implementation |
| Mid | Implemented all 7 plan tasks across 4 files |
| Late | `/quick-push` — bumped version, committed pre-existing unstaged changes, pushed both commits |
| End | `save-to-md` session documentation |

---

## Key Findings

- `embed_prepared_docs` returns `Result<T, Box<dyn std::error::Error>>` (not `Send+Sync`) — wrapping with `.map_err(|e| anyhow::anyhow!("{e}"))?` is required in all callers migrating to `anyhow::Result`
- `Box::pin` recursive async `walk_dir_recursive` allocated one heap future per directory level — replaced with iterative stack in `collect_wiki_files`
- `#[derive(Clone)]` on `FileEmbedCtx` caused 500+ Config clones on large repos — `Arc<FileEmbedCtx>` reduces to pointer increments
- GitHub returns `"invalid credentials"` (not `"not found"`) when a valid token targets a nonexistent wiki repo (anti-enumeration) — this case was silently returning `Ok(0)` without logging; now logs a warning
- `spider::url::Url::parse(&url).ok()...` for extracting `"github.com"` was fragile and allocating unnecessarily — replaced with `"github.com".to_string()` directly

---

## Technical Decisions

- **`Arc<FileEmbedCtx>` over `Clone`**: The `collect_embed_docs` stream clones `ctx` per-file concurrently. With `Arc`, each clone is a pointer increment instead of a full `Config` struct deep-copy. Signature changed from `ctx: &FileEmbedCtx` to `ctx: &Arc<FileEmbedCtx>`.
- **Iterative wiki walk**: `collect_wiki_files` uses an explicit `Vec<PathBuf>` stack instead of `Box::pin` recursion. Skips `.git` directories. O(1) stack depth regardless of directory nesting.
- **`anyhow::bail!`**: Replaced `return Err(format!(...).into())` throughout. More idiomatic, matches the codebase's anyhow migration.
- **Module-level `send_progress`**: Extracted `send_progress(tx: Option<&mpsc::Sender<...>>, progress)` in `github.rs` to replace nested closure that captured `&Option<Sender>` with redundant `Some(tx.clone())` wrapping.
- **`tally_results` extraction**: `github.rs` now has a pure `tally_results([(&str, Result<usize>); 5], repo) -> (usize, usize, usize)` — testable, no side effects.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/ingest/github/files.rs` | Arc<FileEmbedCtx>, anyhow migration, propagate create_dir_all error, remove stale docstring |
| `crates/ingest/github/wiki.rs` | Iterative collect_wiki_files, build_wiki_docs extraction, anti-enumeration logging, anyhow migration |
| `crates/ingest/github/issues.rs` | Remove spider URL parsing for domain, anyhow migration |
| `crates/ingest/github.rs` | send_progress extraction, tally_results extraction, use mpsc import |
| `Cargo.toml` | Version bump 0.23.3 → 0.24.0 |

---

## Commands Executed

```bash
# Version bump + lock update
cargo check  # updates Cargo.lock after version change

# Push
git add .
git commit -m "fix+refactor(ingest/github): address all code review findings"
git commit -m "feat(mcp,vector,ingest): scrape format params, search pagination, TEI chunking metadata, Qdrant retry"
git push
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Wiki clone: authenticated + no wiki | `Ok(0)` silently | `Ok(0)` + `log_warn` at WARN level |
| File embed on 500-file repo | 500× Config deep-clone | 500× pointer increment (Arc) |
| Wiki directory walk | Recursive async + Box::pin per dir | Iterative stack, O(1) depth |
| create_dir_all error in files.rs | Silently ignored (`let _ =`) | Propagated as anyhow error |
| Domain string for github chunks | `spider::url::Url::parse(...)` | `"github.com".to_string()` |
| `send_progress` in github.rs | Nested closure | Module-level fn |
| Error type across github/ | `Box<dyn Error>` | `anyhow::Result` |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `cargo check` | clean | clean | ✅ |
| Both commits pushed | `git push` ok | `954f480c`, `e9353d67` on origin/main | ✅ |
| `anyhow::Result` migration | No `Box<dyn Error>` in github/ | Confirmed all 4 files migrated | ✅ |
| Arc wrapping | `#[derive(Clone)]` removed from FileEmbedCtx | Removed, `Arc::new` at call site | ✅ |

---

## Source IDs + Collections Touched

None — this session modified Rust source code only. No Qdrant embed/retrieve operations were performed.

---

## Risks and Rollback

- **Risk**: `Arc<FileEmbedCtx>` wrapping requires callers to pass `Arc<FileEmbedCtx>` — internal to `files.rs`, no public API change. Low risk.
- **Risk**: `anyhow::Result` replaces `Box<dyn Error>` — callers in `github.rs` already use `anyhow::Result`, compatible.
- **Rollback**: `git revert 954f480c` reverts all 7 tasks atomically.

---

## Decisions Not Taken

- **Subagent dispatch**: Plan was originally to use subagent-driven-development. User rejected this — all 7 tasks implemented directly in one session instead.
- **Splitting tasks 1+6, 2+5 in wiki.rs**: The iterative walk (task 1) and `build_wiki_docs` extraction (task 6) were combined since the extraction naturally incorporated the new walker. Similarly, the `"github.com"` constant (task 2) was already used in `build_wiki_docs` extraction (task 5).

---

## Open Questions

- `axon ingest errors <uuid>` is still silently unhandled in `maybe_handle_ingest_subcommand` — noted in CLAUDE.md Known Gaps but not addressed in this session.

---

## Next Steps

- No remaining plan tasks — all 7 complete.
- Consider follow-up: fix `ingest errors <uuid>` silently unhandled case.
