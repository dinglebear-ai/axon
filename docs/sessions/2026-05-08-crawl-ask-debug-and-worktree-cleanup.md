# 2026-05-08 Crawl/Ask Debug and Worktree Cleanup

## Context

Working directory: `/home/jmagar/workspace/axon_rust`

Primary branch at the end of the session: `main`

Final pushed commit:

```text
f060d49e docs: fix stale job status import path
```

## Crawl / Ask Debug

Initial symptoms:

- `axon status` showed pending crawl jobs for `https://ui.shadcn.com/docs/registry`.
- `axon ask "how do i create a shadcn registry"` failed because Qdrant collection `axon` did not exist at `http://127.0.0.1:53333/`.
- `ask` first warned that dual-search fell back to parallel-single, then failed during vector search dispatch because the collection probe returned 404.

Findings:

- The pending crawl queue was in `/home/jmagar/appdata/jobs.db`.
- The only long-running `axon serve mcp` process was a cached plugin binary running from `/home/jmagar`, with `AXON_COLLECTION=cortex` and no `AXON_DATA_DIR`; it was not processing the CLI queue that `axon status` was showing.
- Running a crawl worker from the repo with the effective repo environment drained the relevant crawl queue.

Actions:

- Ran the crawl worker in the correct repo/env.
- Completed crawl job `8aeb4520-d140-4aee-a6ea-55ab357b5bb9`.
- Embedded `183` docs and `2193` chunks into the `axon` collection.
- Canceled duplicate queued crawl jobs:
  - `fcbec6c2-d736-4f0e-83b8-9be3176f2d24`
  - `18103ce3-e3cc-4a54-b55c-f34cb72dcc2d`

Result:

- `axon ask "how do i create a shadcn registry"` succeeded with shadcn registry citations.
- Retrieval completed in about `39ms`; total ask runtime was about `7027ms`.

## Crawl Scoping Bead

A follow-up Bead was filed for the crawl auto-scope behavior, then reclassified after review:

- Bead: `axon_rust-b4y`
- Final classification: feature
- Final title: `Decide crawl auto-scope behavior for two-segment documentation roots`

Reasoning captured during the session:

- `/docs/registry` currently behaves like a leaf path for auto-scope derivation, reducing to `/docs/`.
- Because `/docs/` is only one segment, auto-scope is disabled.
- That allows broad crawl results such as `/blocks`, `/charts`, and `/docs/installation`.
- This may be desirable as a feature decision rather than a clear bug, so it was reclassified from bug to feature.

`bd dolt push` completed after the Bead update.

## Worktree Cleanup

Initial worktrees:

```text
/home/jmagar/workspace/axon_rust                         main
/home/jmagar/workspace/axon_rust/.claude/worktrees/src-layout  worktree-src-layout
/home/jmagar/workspace/axon_rust/.worktrees/axon-6dl-ask-headless  axon-6dl-ask-headless
```

Findings:

- Primary checkout was already on `main`.
- `axon-6dl-ask-headless` was merged into `main`.
- `worktree-src-layout` was not a strict ancestor of `main`, but the important layout change was already present on `main` via:

```text
3f421355 refactor: rename crates/ to src/, adopt standard single-crate layout
```

- `src/` existed on `main`; the earlier concern that the layout branch was unmerged was too literal.
- Active CI/script surfaces no longer contained stale `crates/` path references.
- The only live Rust stale import found was a documentation example in `src/jobs/status.rs`.

Actions:

- Removed worktree `.worktrees/axon-6dl-ask-headless`.
- Deleted local branch `axon-6dl-ask-headless`.
- Deleted remote branch `origin/axon-6dl-ask-headless`.
- Removed worktree `.claude/worktrees/src-layout`.
- Deleted local branch `worktree-src-layout`.
- Fixed stale doc import:

```diff
-/// # use axon::crates::jobs::status::JobStatus;
+/// # use axon::jobs::status::JobStatus;
```

## Verification

Commands run successfully:

```bash
cargo check --bin axon
git pull --rebase
bd dolt push
git push
```

Pre-commit hook passed:

- monolith
- rustfmt
- unwrap-warn
- claude-symlinks
- env-guard
- mcp-http-only
- no-mod-rs
- clippy
- test

Final state:

```text
git status: main...origin/main, clean
git worktree list: only /home/jmagar/workspace/axon_rust remains
```

## Open Questions

- The permanent service/env mismatch remains a follow-up concern: cached plugin `axon serve mcp` had different collection/data-dir settings than the CLI queue.
- `axon_rust-b4y` remains open as a feature decision for two-segment documentation-root crawl scoping.
