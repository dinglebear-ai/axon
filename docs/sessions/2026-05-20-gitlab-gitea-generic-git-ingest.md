---
date: 2026-05-20 23:36:36 EST
repo: git@github.com:jmagar/axon.git
branch: feature/gitlab-ingest
head: c26df9be
plan: docs/superpowers/plans/2026-05-21-gitlab-ingest.md
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust c26df9be [feature/gitlab-ingest]
---

# GitLab, Gitea, and Generic Git Ingest

## User Request

Add GitLab ingest, then add Gitea/Forgejo and generic Git HTTPS ingest, verify against live services, and quick-push the branch.

## Session Overview

Implemented and pushed first-class GitLab ingest plus Gitea/Forgejo and explicit generic HTTPS Git ingest. The work is on `feature/gitlab-ingest` and was pushed through commit `a8db6165`, follow-up docs commit `5e90648e`, and requested empty checkpoint commit `c26df9be`.

## Sequence of Events

1. Wrote the implementation plan at `docs/superpowers/plans/2026-05-21-gitlab-ingest.md`.
2. Added GitLab target parsing, API client, metadata/file/issues/merge-request/wiki ingest, classification, CLI dispatch, async job dispatch, MCP mapping, config, tests, and docs.
3. Added Gitea/Forgejo support and generic HTTPS Git support, including explicit `git:` targets and provider-specific source metadata.
4. Ran live CLI ingests against the running local Qdrant/TEI services for GitLab, Codeberg/Forgejo, and generic Git.
5. Bumped Axon from `4.2.0` to `4.3.0`, updated changelog and version-bearing files, and fixed the compose env contract for new ingest tokens.
6. Split `src/jobs/workers/runners/ingest.rs` after the pre-commit monolith hook caught `run_ingest_job()` growing past the function limit.
7. Pushed the feature branch and then committed generated CLAUDE note updates.
8. Created and pushed an empty checkpoint commit after an explicit `git add . commit and push` request when the tree was clean.

## Key Findings

- GitLab needs explicit URL or `gitlab:` target support because nested namespaces make bare path shorthand ambiguous with GitHub `owner/repo`.
- Gitea/Forgejo can use the Gitea-compatible API for metadata, issues, and pulls, while file ingest can share the clone/file traversal path with generic Git.
- Generic Git should stay explicit-only (`git:https://...`) and HTTPS-only to avoid accidental provider misclassification and unsafe local/SSH target handling.
- After refactoring Gitea to share generic clone logic, a live Qdrant payload check showed the debug binary had not been rebuilt; rebuilding verified file chunks now use `source_type: "gitea"` and `provider: "gitea"`.

## Technical Decisions

- GitLab was implemented as a peer provider rather than a generic provider abstraction because its namespace parsing, URL-encoded project IDs, merge request terminology, and wiki API differ from GitHub.
- `GITLAB_TOKEN` and `GITEA_TOKEN` remain env-only secrets and are redacted/configured alongside existing ingest tokens.
- Existing `--max-issues`, `--max-prs`, and `--no-source` behavior is reused for GitLab and Gitea/Forgejo.
- Gitea file chunks delegate to generic clone traversal but override provider metadata so downstream filtering can distinguish `gitea` from generic `git`.

## Files Modified

- Provider implementation: `src/ingest/gitlab.rs`, `src/ingest/gitlab/*`, `src/ingest/gitea.rs`, `src/ingest/generic_git.rs`.
- Provider tests: `src/ingest/gitlab_tests.rs`, `src/ingest/gitea_tests.rs`, `src/ingest/generic_git_tests.rs`, `src/ingest/classify_tests.rs`.
- CLI/services/jobs/MCP wiring: `src/cli/commands/ingest*.rs`, `src/services/ingest.rs`, `src/services/ingest/*`, `src/jobs/ingest/types.rs`, `src/jobs/workers/runners/ingest.rs`, `src/mcp/*`.
- Config/docs: `.env.example`, `README.md`, `CLAUDE.md`, `docs/CONFIG.md`, `docs/MCP-TOOL-SCHEMA.md`, `docs/commands/ingest.md`, `docs/ingest/*`, `docs/auth/API-TOKEN.md`, env migration docs, and local CLAUDE notes.
- Version/release: `Cargo.toml`, `Cargo.lock`, `apps/web/package.json`, `CHANGELOG.md`.
- Contract test: `tests/compose_env_contract.rs`.

## Commands Executed

- `cargo fmt --check` passed.
- `cargo check --bin axon` passed.
- `cargo clippy --bin axon -- -D warnings` passed.
- `cargo test gitea --lib` passed.
- `cargo test generic_git --lib` passed.
- `cargo test --lib` passed with `2052 passed; 0 failed; 6 ignored`.
- `cargo test --test compose_env_contract` passed with `13 passed`.
- `python3 scripts/enforce_monoliths.py --staged` passed after refactoring the ingest runner.
- `git push -u origin feature/gitlab-ingest` pushed the feature branch.

## Errors Encountered

- The pre-commit monolith hook rejected `src/jobs/workers/runners/ingest.rs` because `run_ingest_job()` reached 174 lines. Fixed by factoring provider execution into helper functions and a shared cancel wrapper.
- `cargo test --test compose_env_contract` initially failed because `.env.example` gained `GITLAB_TOKEN` and `GITEA_TOKEN` but the allowed production env key list had not been updated. Fixed in `tests/compose_env_contract.rs`.
- A post-tightening live Gitea ingest initially appeared to emit generic `source_type: "git"` because `target/debug/axon` had not been rebuilt. Rebuilt the binary and reran the live ingest, confirming `source_type: "gitea"`.

## Behavior Changes

Before:
- `axon ingest` supported GitHub, Reddit, YouTube, and sessions.
- MCP ingest source types did not include GitLab, Gitea/Forgejo, or generic Git.

After:
- `axon ingest` accepts GitLab URLs / `gitlab:` targets, Gitea/Forgejo URLs / prefixes, and explicit `git:https://...` clone URLs.
- Async jobs, status output, services, MCP request mapping, config, docs, and tests understand `gitlab`, `gitea`, and `git` source types.
- Axon version is `4.3.0`.

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `axon --local ingest https://gitlab.com/gitlab-org/cli --wait true --no-source --max-issues 1 --max-prs 1 --collection gitlab_ingest_live --json` | GitLab API ingest succeeds | `{"chunks_embedded":424}` | Pass |
| `axon --local ingest https://gitlab.com/gitlab-examples/nodejs --wait true --max-issues 1 --max-prs 1 --collection gitlab_ingest_source_live_2 --json` | GitLab source ingest succeeds | `{"chunks_embedded":15}` | Pass |
| `axon --local ingest https://codeberg.org/forgejo/forgejo --wait true --no-source --max-issues 1 --max-prs 1 --collection gitea_ingest_live_verify2 --json` | Gitea/Forgejo ingest succeeds | `{"chunks_embedded":1033}` | Pass |
| `curl .../collections/gitea_ingest_live_verify2/points/scroll` | Gitea file payload uses Gitea source metadata | `source_type: "gitea"`, `provider: "gitea"` | Pass |
| `axon --local ingest git:https://codeberg.org/forgejo/forgejo.git --wait true --no-source --collection generic_git_ingest_live --json` | Generic Git ingest succeeds | `{"chunks_embedded":1029}` | Pass |
| `cargo test --lib` | Unit suite passes | `2052 passed; 0 failed; 6 ignored` | Pass |
| Pre-commit hook on `a8db6165` | Repo gates pass | monolith, rustfmt, clippy, secrets, and tests passed | Pass |

## Risks and Rollback

- GitLab and Gitea API behavior can vary across self-hosted instances; explicit prefix targets are documented for non-default hosts.
- Generic Git intentionally indexes only repository files and does not try to infer issues, PRs, metadata, or wiki pages.
- Rollback path is reverting `a8db6165` and `5e90648e`; `c26df9be` is an empty checkpoint.

## References

- GitLab REST API resources: https://docs.gitlab.com/api/api_resources/
- GitLab merge requests API: https://docs.gitlab.com/api/merge_requests/
- Gitea API docs: https://docs.gitea.com/api/1.25/
- Forgejo API usage docs: https://forgejo.org/docs/latest/user/api-usage/

## Current Git State

At save time, branch `feature/gitlab-ingest` was even with `origin/feature/gitlab-ingest` at `c26df9be`, but the worktree had separate uncommitted extract/vertical changes:

- `src/extract/registry.rs`
- `src/extract/registry_tests.rs`
- `src/extract/verticals.rs`
- `src/extract/verticals/reddit.rs`
- deleted `src/extract/verticals/youtube_video.rs`
- deleted `src/extract/verticals/youtube_video_tests.rs`

Those changes were not part of the ingest branch commits documented above.

## Next Steps

- Open or update the PR for `feature/gitlab-ingest`.
- Decide whether the current uncommitted extract/vertical changes should be committed separately, reverted, or moved to another branch.
