# GitLab Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class GitLab ingest support for CLI, MCP, async jobs, docs, and real endpoint verification.

**Architecture:** Add GitLab as a peer provider beside GitHub rather than introducing a broad git-provider abstraction. GitLab uses its own target parser and REST client because nested namespace paths, URL-encoded project IDs, and merge-request terminology differ from GitHub; file and wiki indexing reuse the existing local `git clone` plus source-file filtering approach.

**Tech Stack:** Rust 2024, reqwest, serde/serde_json, existing SQLite job runtime, existing Axon services layer, GitLab REST API v4, existing `git` subprocess helper, Qdrant/TEI live stack for verification.

---

## File Structure

**New files:**
- `src/ingest/gitlab.rs` — provider entrypoint, target parsing, REST client, metadata/issues/MR/wiki/file orchestration.
- `src/ingest/gitlab_tests.rs` — parser and URL-building tests.

**Modified files:**
- `src/ingest.rs` — export the GitLab provider module.
- `src/ingest/classify.rs` and `src/ingest/classify_tests.rs` — auto-detect `gitlab.com/...` URLs and explicit `gitlab:...` targets without breaking GitHub `owner/repo`.
- `src/jobs/ingest/types.rs` — add `IngestSource::Gitlab`, labels, and serialized job config support.
- `src/jobs/workers/runners/ingest.rs` — run GitLab ingest jobs with cancellation behavior matching GitHub.
- `src/services/ingest.rs` — add validation, MCP request mapping, and `ingest_gitlab_with_progress`.
- `src/cli/commands/ingest.rs` and `src/cli/commands/ingest_common.rs` — usage text and sync dispatch.
- `src/mcp/schema/requests.rs`, `src/mcp/server/handlers_embed_ingest.rs`, schema tests — expose `source_type: "gitlab"`.
- `src/core/config/types/*`, `src/core/config/parse/*`, `.env.example`, `config.example.toml` as needed — add `GITLAB_TOKEN`, reuse `--max-issues`, `--max-prs`, and `--no-source`.
- `docs/MCP-TOOL-SCHEMA.md`, `src/ingest/README.md`, top-level docs — document GitLab ingest and verification command.

## Task 1: Source Type and Classification

- [ ] Add a failing unit test proving `https://gitlab.com/gitlab-org/gitlab-runner` classifies as GitLab with target `gitlab.com/gitlab-org/gitlab-runner`.
- [ ] Add a failing unit test proving nested namespaces such as `https://gitlab.com/group/subgroup/project` preserve the full path.
- [ ] Add a failing unit test proving bare `owner/repo` still classifies as GitHub.
- [ ] Add `IngestSource::Gitlab { target: String, include_source: bool }`.
- [ ] Add `source_type_label` and `target_label` branches returning `gitlab` and the normalized target.
- [ ] Implement GitLab classification after YouTube and before GitHub slug fallback so GitHub short slugs remain unchanged.
- [ ] Run `cargo test ingest::classify --lib`.

## Task 2: GitLab Provider Core

- [ ] Create `src/ingest/gitlab.rs` with `GitLabTarget { host, namespace_path, project, web_url, clone_url, api_base }`.
- [ ] Parse only URL or `gitlab:`-prefixed inputs for now; reject ambiguous bare multi-segment paths with an actionable error.
- [ ] URL-encode the full namespace path for `/api/v4/projects/:id`.
- [ ] Build a reqwest client with `PRIVATE-TOKEN` when `GITLAB_TOKEN` is set.
- [ ] Fetch project metadata from `/projects/:id`.
- [ ] Emit one metadata `PreparedDoc` with `source_type: "gitlab"` and payload fields for host, namespace, project, default branch, visibility, stars, forks, issues enabled, MRs enabled, and wiki enabled.
- [ ] Run `cargo test ingest::gitlab --lib`.

## Task 3: Files and Wiki

- [ ] Reuse GitHub source-file filtering by making the filter helpers provider-neutral or importing the public helpers.
- [ ] Clone the project with `git clone --depth=1 --branch <default_branch> --single-branch`.
- [ ] Pass auth through `git -c http.extraHeader=PRIVATE-TOKEN: <token>` for GitLab clone without token-in-URL.
- [ ] Embed docs always and source files unless `--no-source` is set.
- [ ] Add wiki ingest through the GitLab Project Wikis API (`/projects/:id/wikis`) rather than guessing clone URL shape.
- [ ] Run provider tests and one local no-network test for auth stderr redaction.

## Task 4: Issues and Merge Requests

- [ ] Fetch project issues from `/projects/:id/issues?state=all&order_by=updated_at&sort=desc&per_page=100`.
- [ ] Respect existing `--max-issues` / `GITHUB_MAX_ISSUES` value until a separate GitLab-specific config is justified.
- [ ] Fetch merge requests from `/projects/:id/merge_requests?state=all&order_by=updated_at&sort=desc&per_page=100`.
- [ ] Respect existing `--max-prs` / `GITHUB_MAX_PRS` value for merge requests.
- [ ] Embed issue and MR docs with stable GitLab payload fields and web URLs.
- [ ] Add pagination tests using parsed `Link` headers or page counters.

## Task 5: CLI, MCP, Docs

- [ ] Add sync and async service dispatch for `IngestSource::Gitlab`.
- [ ] Add MCP `IngestSourceType::Gitlab`, handler parsing, service mapping, and serde tests.
- [ ] Update `axon ingest --help` text to mention GitLab URLs.
- [ ] Update `docs/MCP-TOOL-SCHEMA.md`, `.env.example`, and `src/ingest/README.md`.
- [ ] Run `cargo test mcp::schema mcp::server::handlers_embed_ingest --lib`.

## Task 6: Live Verification

- [ ] Build the debug binary with the real Rust toolchain if the default cargo wrapper fails.
- [ ] Start or confirm live Qdrant/TEI compose services.
- [ ] Run `./target/debug/axon ingest https://gitlab.com/gitlab-org/gitlab-runner --wait true --max-issues 1 --max-prs 1 --no-source --collection <test_collection>`.
- [ ] Run a source-enabled small public GitLab repo ingest against a throwaway collection.
- [ ] Query Qdrant through `axon query` for a known GitLab metadata term.
- [ ] Run focused Rust tests, `cargo fmt --check`, and `cargo check --bin axon`.

## Self-Review

- Spec coverage: the plan covers classification, provider implementation, source/wiki/issues/MRs, CLI, MCP, docs, and live endpoint verification.
- Placeholder scan: no deferred implementation placeholders remain; optional future self-hosted GitLab polish is intentionally out of scope unless URL-derived hosts work without extra config.
- Type consistency: the plan uses one new `IngestSource::Gitlab` variant and one provider entrypoint across CLI, MCP, worker, and services.
