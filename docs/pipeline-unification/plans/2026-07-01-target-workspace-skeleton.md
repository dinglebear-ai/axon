# Target Workspace Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the first implementation PR for issue #298 by adding the target crate workspace skeleton, repo-structure guardrails, and documentation corrections without moving runtime behavior.

**Architecture:** This PR establishes the target crate map as inert compileable structure. Existing runtime crates remain wired exactly as they are today. New crates expose marker modules only, so later PRs can move DTOs, traits, stores, providers, adapters, and services into their final homes under a checked workspace shape.

**Tech Stack:** Rust 2024 workspace, Cargo workspace members, xtask repo checks, crate-local `src/CLAUDE.md` files with `AGENTS.md` and `GEMINI.md` symlinks.

## Global Constraints

- Keep `main` behavior unchanged: no CLI, MCP, REST, job runtime, Qdrant, TEI, crawl, embed, ingest, ask, memory, or watch behavior moves in this PR.
- Keep all existing crates in place until replacement crates have real behavior in later PRs.
- Add target crates as compileable skeletons with marker modules and no operational side effects.
- Use crate names with one hyphen only: `axon-name`. Do not introduce double-hyphen crate names.
- Do not add compatibility aliases for removed future surfaces.
- Do not migrate, tombstone, or prune existing local data. The target implementation assumes an empty database.
- Do not add new external dependencies for skeleton crates.
- Write crate-local agent memory to `src/CLAUDE.md`; make `src/AGENTS.md` and `src/GEMINI.md` symlinks to `CLAUDE.md`.
- Run the smallest checks that prove this docs-and-skeleton PR, plus `cargo check --workspace --locked` because workspace membership changes.

---

## Target PR Scope

This PR is PR0 for issue #298.

It includes:

- Target crate directories and Cargo manifests.
- Target marker modules matching the crate README contracts.
- Root workspace membership update.
- `cargo xtask check-repo-structure`.
- Integration of repo-structure check into `cargo xtask check`.
- Delivery docs correction so `first-implementation-pr.md` no longer claims DTO-first is PR1.
- No public runtime wiring.

It excludes:

- Moving DTOs from existing crates.
- Replacing `axon-source-ledger`.
- Renaming `axon-vector`.
- Deleting `axon-code-index`.
- Replacing job runtime types.
- Changing Qdrant payloads.
- Changing CLI commands.
- Changing MCP tool schema.
- Changing REST routes.
- Adding migrations.

---

## Target New Crates

Create these new crates under `crates/`:

```text
axon-error
axon-observe
axon-route
axon-adapters
axon-ledger
axon-parse
axon-graph
axon-memory
axon-document
axon-embedding
axon-vectors
axon-retrieval
axon-llm
axon-prune
```

Keep these transitional crates in the workspace:

```text
axon-crawl
axon-vector
axon-ingest
axon-extract
axon-jobs
axon-source-ledger
axon-code-index
```

Existing stable crates also remain:

```text
axon-api
axon-authz
axon-core
axon-services
axon-mcp
axon-web
axon-cli
```

---

## Module Matrix

Every target crate must include `src/lib.rs`, `src/CLAUDE.md`, `src/AGENTS.md`, `src/GEMINI.md`, and the module files listed here.

| Crate | Module files |
| --- | --- |
| `axon-adapters` | `adapter.rs`, `registry.rs`, `capability.rs`, `acquisition.rs`, `manifest.rs`, `web.rs`, `local.rs`, `git.rs`, `registry_sources.rs`, `feed.rs`, `youtube.rs`, `reddit.rs`, `sessions.rs`, `cli_tool.rs`, `mcp_tool.rs`, `testing.rs` |
| `axon-error` | `api_error.rs`, `code.rs`, `stage.rs`, `severity.rs`, `retry.rs`, `degradation.rs`, `cooling.rs`, `context.rs`, `conversion.rs`, `testing.rs` |
| `axon-observe` | `event.rs`, `phase.rs`, `heartbeat.rs`, `progress.rs`, `metric.rs`, `span.rs`, `log.rs`, `collector.rs`, `testing.rs` |
| `axon-route` | `resolver.rs`, `router.rs`, `canonical.rs`, `source_id.rs`, `scope.rs`, `authority.rs`, `alias.rs`, `capability.rs`, `testing.rs` |
| `axon-ledger` | `store.rs`, `sqlite.rs`, `migration.rs`, `source.rs`, `item.rs`, `manifest.rs`, `diff.rs`, `generation.rs`, `document_status.rs`, `lease.rs`, `cleanup_debt.rs`, `transaction.rs`, `testing.rs` |
| `axon-parse` | `parser.rs`, `registry.rs`, `facts.rs`, `graph_candidate.rs`, `code.rs`, `manifest.rs`, `schema.rs`, `session.rs`, `tool.rs`, `env.rs`, `docker.rs`, `config.rs`, `testing.rs` |
| `axon-graph` | `store.rs`, `sqlite.rs`, `migration.rs`, `node.rs`, `edge.rs`, `evidence.rs`, `candidate.rs`, `authority.rs`, `merge.rs`, `query.rs`, `testing.rs` |
| `axon-memory` | `store.rs`, `sqlite.rs`, `migration.rs`, `record.rs`, `link.rs`, `decay.rs`, `review.rs`, `recall.rs`, `context.rs`, `graph.rs`, `testing.rs` |
| `axon-document` | `preparer.rs`, `chunk_router.rs`, `profile.rs`, `prepared.rs`, `chunk.rs`, `metadata.rs`, `code.rs`, `markdown.rs`, `transcript.rs`, `session.rs`, `schema.rs`, `text.rs`, `testing.rs` |
| `axon-embedding` | `provider.rs`, `batch.rs`, `capability.rs`, `reservation.rs`, `tei.rs`, `openai_compat.rs`, `fake.rs`, `testing.rs` |
| `axon-vectors` | `store.rs`, `qdrant.rs`, `collection.rs`, `point.rs`, `payload.rs`, `filter.rs`, `query.rs`, `health.rs`, `testing.rs` |
| `axon-retrieval` | `engine.rs`, `plan.rs`, `query.rs`, `filter.rs`, `rank.rs`, `context.rs`, `citation.rs`, `memory.rs`, `graph.rs`, `testing.rs` |
| `axon-llm` | `provider.rs`, `capability.rs`, `completion.rs`, `stream.rs`, `prompt.rs`, `openai_compat.rs`, `codex.rs`, `gemini.rs`, `fake.rs`, `testing.rs` |
| `axon-prune` | `plan.rs`, `executor.rs`, `debt.rs`, `generation.rs`, `orphan.rs`, `dedupe.rs`, `receipt.rs`, `safety.rs`, `testing.rs` |

---

## Task 1: Add Failing Repo-Structure Check Tests

- [ ] Create `xtask/src/checks/repo_structure_tests.rs` with tests that prove the checker fails when a target crate is missing and passes for a complete fixture.

Use this exact test file:

```rust
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::repo_structure::{
    check_root, EXISTING_STABLE_CRATES, TARGET_NEW_CRATES, TRANSITIONAL_CRATES,
};

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn symlink(target: &str, link: &Path) {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    unix_fs::symlink(target, link).unwrap();
}

fn complete_fixture() -> PathBuf {
    let dir = tempdir().unwrap().into_path();
    let all_crates = TARGET_NEW_CRATES
        .iter()
        .chain(TRANSITIONAL_CRATES.iter())
        .chain(EXISTING_STABLE_CRATES.iter())
        .copied()
        .collect::<Vec<_>>();
    let members = all_crates
        .iter()
        .map(|krate| format!("    \"crates/{krate}\","))
        .collect::<Vec<_>>()
        .join("\n");
    write(
        &dir.join("Cargo.toml"),
        &format!("[workspace]\nmembers = [\n{members}\n]\n"),
    );

    for krate in all_crates {
        let root = dir.join("crates").join(krate);
        write(&root.join("Cargo.toml"), "[package]\nname = \"fixture\"\n");
        write(&root.join("src/lib.rs"), "pub const CRATE_NAME: &str = \"fixture\";\n");
        write(&root.join("src/CLAUDE.md"), "# Fixture\n");
        symlink("CLAUDE.md", &root.join("src/AGENTS.md"));
        symlink("CLAUDE.md", &root.join("src/GEMINI.md"));
    }

    dir
}

#[test]
fn complete_fixture_passes() {
    let root = complete_fixture();
    check_root(&root).unwrap();
}

#[test]
fn missing_target_crate_fails() {
    let root = complete_fixture();
    fs::remove_dir_all(root.join("crates/axon-prune")).unwrap();

    let err = check_root(&root).unwrap_err();
    assert!(
        err.contains("missing target crate directory: crates/axon-prune"),
        "{err}"
    );
}

#[test]
fn broken_agent_memory_symlink_fails() {
    let root = complete_fixture();
    fs::remove_file(root.join("crates/axon-route/src/AGENTS.md")).unwrap();
    symlink("../CLAUDE.md", &root.join("crates/axon-route/src/AGENTS.md"));

    let err = check_root(&root).unwrap_err();
    assert!(
        err.contains("crates/axon-route/src/AGENTS.md must symlink to CLAUDE.md"),
        "{err}"
    );
}
```

- [ ] Add the test module to `xtask/src/checks.rs`:

```rust
pub mod repo_structure;

#[cfg(test)]
mod repo_structure_tests;
```

- [ ] Run the test before implementing the checker.

```bash
cargo test -p xtask repo_structure
```

Expected failure:

```text
error[E0583]: file not found for module `repo_structure`
```

---

## Task 2: Implement `cargo xtask check-repo-structure`

- [ ] Create `xtask/src/checks/repo_structure.rs`.

Use this checker shape:

```rust
use std::fs;
use std::path::{Path, PathBuf};

pub const TARGET_NEW_CRATES: &[&str] = &[
    "axon-error",
    "axon-observe",
    "axon-route",
    "axon-adapters",
    "axon-ledger",
    "axon-parse",
    "axon-graph",
    "axon-memory",
    "axon-document",
    "axon-embedding",
    "axon-vectors",
    "axon-retrieval",
    "axon-llm",
    "axon-prune",
];

pub const TRANSITIONAL_CRATES: &[&str] = &[
    "axon-crawl",
    "axon-vector",
    "axon-ingest",
    "axon-extract",
    "axon-jobs",
    "axon-source-ledger",
    "axon-code-index",
];

pub const EXISTING_STABLE_CRATES: &[&str] = &[
    "axon-api",
    "axon-authz",
    "axon-core",
    "axon-services",
    "axon-mcp",
    "axon-web",
    "axon-cli",
];

pub fn check() -> anyhow::Result<()> {
    check_root(Path::new(".")).map_err(anyhow::Error::msg)
}

pub fn check_root(root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    let cargo_toml = read(root.join("Cargo.toml"), &mut errors);

    for krate in TARGET_NEW_CRATES {
        check_target_crate(root, krate, &cargo_toml, &mut errors);
    }

    for krate in TRANSITIONAL_CRATES.iter().chain(EXISTING_STABLE_CRATES.iter()) {
        check_workspace_member(root, krate, &cargo_toml, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn check_target_crate(root: &Path, krate: &str, cargo_toml: &str, errors: &mut Vec<String>) {
    if krate.matches('-').count() != 1 {
        errors.push(format!("target crate has invalid double-hyphen-style name: {krate}"));
    }

    check_workspace_member(root, krate, cargo_toml, errors);

    let crate_root = root.join("crates").join(krate);
    require_file(&crate_root.join("Cargo.toml"), errors);
    require_file(&crate_root.join("src/lib.rs"), errors);
    require_file(&crate_root.join("src/CLAUDE.md"), errors);
    require_claude_symlink(&crate_root.join("src/AGENTS.md"), errors);
    require_claude_symlink(&crate_root.join("src/GEMINI.md"), errors);
}

fn check_workspace_member(root: &Path, krate: &str, cargo_toml: &str, errors: &mut Vec<String>) {
    let crate_dir = format!("crates/{krate}");
    if !root.join(&crate_dir).is_dir() {
        errors.push(format!("missing target crate directory: {crate_dir}"));
    }
    if !cargo_toml.contains(&format!("\"{crate_dir}\"")) {
        errors.push(format!("root Cargo.toml is missing workspace member: {crate_dir}"));
    }
}

fn require_file(path: &Path, errors: &mut Vec<String>) {
    if !path.is_file() {
        errors.push(format!("missing required file: {}", display(path)));
    }
}

fn require_claude_symlink(path: &Path, errors: &mut Vec<String>) {
    match fs::read_link(path) {
        Ok(target) if target == PathBuf::from("CLAUDE.md") => {}
        Ok(_) => errors.push(format!("{} must symlink to CLAUDE.md", display(path))),
        Err(_) => errors.push(format!("missing required symlink: {}", display(path))),
    }
}

fn read(path: impl AsRef<Path>, errors: &mut Vec<String>) -> String {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(body) => body,
        Err(err) => {
            errors.push(format!("failed to read {}: {err}", display(path)));
            String::new()
        }
    }
}

fn display(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}
```

- [ ] Wire the command in `xtask/src/main.rs` by adding a `Command::CheckRepoStructure` variant:

```rust
CheckRepoStructure,
```

- [ ] Wire the match arm in `xtask/src/main.rs`:

```rust
Command::CheckRepoStructure => checks::repo_structure::check(),
```

- [ ] Add `repo_structure::check()?;` to `xtask/src/checks.rs::check()` after `claude_symlinks::check()?;` so the broad `cargo xtask check` path validates target crate shape.

- [ ] Run:

```bash
cargo test -p xtask repo_structure
```

Expected output:

```text
test result: ok.
```

At this point `cargo xtask check-repo-structure` is expected to fail in the real checkout because target crates do not exist yet.

---

## Task 3: Add Target Skeleton Crates

- [ ] For every crate in the target new crate list, create `crates/<crate>/Cargo.toml`.

Use this exact manifest shape, changing only the `name` field to the crate name:

```toml
[package]
name = "axon-error"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
```

- [ ] For every crate in the module matrix, create `src/lib.rs` that declares all listed modules and exposes the crate marker.

Example for `axon-error/src/lib.rs`:

```rust
//! Target pipeline crate skeleton for `axon-error`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod api_error;
pub mod code;
pub mod stage;
pub mod severity;
pub mod retry;
pub mod degradation;
pub mod cooling;
pub mod context;
pub mod conversion;
pub mod testing;

pub const CRATE_NAME: &str = "axon-error";
```

Example for `axon-adapters/src/lib.rs`:

```rust
//! Target pipeline crate skeleton for `axon-adapters`.
//!
//! This crate is intentionally marker-only in PR0. Runtime behavior moves here
//! in issue #298 implementation PRs after contract tests exist.

pub mod adapter;
pub mod registry;
pub mod capability;
pub mod acquisition;
pub mod manifest;
pub mod web;
pub mod local;
pub mod git;
pub mod registry_sources;
pub mod feed;
pub mod youtube;
pub mod reddit;
pub mod sessions;
pub mod cli_tool;
pub mod mcp_tool;
pub mod testing;

pub const CRATE_NAME: &str = "axon-adapters";
```

- [ ] For every module file in the module matrix, add a marker constant.

Example for `axon-error/src/api_error.rs`:

```rust
//! Marker module for the target `axon-error::api_error` boundary.

pub const MODULE_NAME: &str = "api_error";
```

Example for `axon-adapters/src/registry_sources.rs`:

```rust
//! Marker module for the target `axon-adapters::registry_sources` boundary.

pub const MODULE_NAME: &str = "registry_sources";
```

- [ ] For every target crate, add `src/CLAUDE.md`.

Use this exact structure, changing the title and module list to the crate:

```md
# axon-error

This crate is part of the issue #298 pipeline-unification target structure.

## Ownership

- Owns the target boundaries documented in `docs/pipeline-unification/crates/axon-error/README.md`.
- Contains marker modules only in PR0.
- Must not own runtime behavior until the implementation PR that moves that boundary also moves its contract tests.

## PR0 Rules

- Do not import from runtime crates.
- Do not change public CLI, MCP, REST, job, vector, crawl, embed, ingest, ask, memory, or watch behavior from this crate.
- Keep this crate compileable with workspace defaults and no external dependencies unless a later PR moves real behavior here.

## Modules

- `api_error`
- `code`
- `stage`
- `severity`
- `retry`
- `degradation`
- `cooling`
- `context`
- `conversion`
- `testing`
```

- [ ] For every target crate, create symlinks:

```bash
ln -sf CLAUDE.md crates/<crate>/src/AGENTS.md
ln -sf CLAUDE.md crates/<crate>/src/GEMINI.md
```

- [ ] Run:

```bash
cargo fmt --all
```

Expected output: command exits 0 and only formats newly added Rust files.

---

## Task 4: Update Workspace Membership

- [ ] Replace the root `Cargo.toml` workspace `members` list with this order:

```toml
members = [
    "xtask",
    "crates/axon-error",
    "crates/axon-api",
    "crates/axon-authz",
    "crates/axon-core",
    "crates/axon-observe",
    "crates/axon-route",
    "crates/axon-adapters",
    "crates/axon-ledger",
    "crates/axon-parse",
    "crates/axon-graph",
    "crates/axon-memory",
    "crates/axon-document",
    "crates/axon-embedding",
    "crates/axon-vectors",
    "crates/axon-retrieval",
    "crates/axon-llm",
    "crates/axon-prune",
    "crates/axon-crawl",
    "crates/axon-vector",
    "crates/axon-ingest",
    "crates/axon-extract",
    "crates/axon-jobs",
    "crates/axon-source-ledger",
    "crates/axon-code-index",
    "crates/axon-services",
    "crates/axon-mcp",
    "crates/axon-web",
    "crates/axon-cli",
]
```

- [ ] Do not remove dependency declarations for existing crates in this PR.

- [ ] Run:

```bash
cargo metadata --no-deps --format-version 1
```

Expected output includes package entries for all target crates and exits 0.

---

## Task 5: Correct Delivery Docs For PR0

- [ ] Replace `docs/pipeline-unification/delivery/first-implementation-pr.md` with PR0 scope language.

The doc must say:

```md
# First Implementation PR Scope
Last Modified: 2026-07-01

## Contract

The first implementation PR is the target workspace skeleton. It makes the
crate map concrete and checked, while preserving current runtime behavior.

This PR creates the new target crates, crate-local agent memory files, marker
modules, workspace membership, and repo-structure checks. It does not move DTOs,
adapters, providers, stores, services, commands, routes, jobs, vector payloads,
or migrations.

## Scope

Include:

- target skeleton crates listed in `docs/pipeline-unification/plans/2026-07-01-target-workspace-skeleton.md`
- marker modules matching each crate README
- `cargo xtask check-repo-structure`
- `cargo xtask check` integration for repo structure
- root workspace membership update
- crate-local `src/CLAUDE.md` files and sibling symlinks

Exclude:

- public CLI command changes
- MCP action changes
- REST route changes
- DTO movement
- source adapter movement
- provider implementation movement
- ledger/runtime replacement
- Qdrant payload shape changes
- migrations
- data migration, tombstoning, or pruning

## Acceptance Criteria

- `cargo fmt --check --all`
- `cargo check --workspace --locked`
- `cargo xtask check-repo-structure`
- `cargo xtask check-layering`
- `cargo xtask check-claude-symlinks`
- no public runtime behavior changes

## Next PR

The next implementation PR moves shared error, observation, and source request
DTO primitives only after contract tests exist for those shapes.
```

- [ ] Add a link to this plan in `docs/pipeline-unification/delivery/implementation-checklist.md` under the phase that tracks issue #298 implementation order.

Use this bullet:

```md
- PR0 plan: `docs/pipeline-unification/plans/2026-07-01-target-workspace-skeleton.md`
```

---

## Task 6: Verification

- [ ] Run formatting:

```bash
cargo fmt --check --all
```

Expected output:

```text
```

The command exits 0.

- [ ] Run workspace compile:

```bash
cargo check --workspace --locked
```

Expected output ends with:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in
```

- [ ] Run repo-structure check:

```bash
cargo xtask check-repo-structure
```

Expected output: command exits 0.

- [ ] Run layering check:

```bash
cargo xtask check-layering
```

Expected output:

```text
Layering check passed
```

- [ ] Run agent memory symlink check:

```bash
cargo xtask check-claude-symlinks
```

Expected output: command exits 0.

- [ ] Run focused xtask tests:

```bash
cargo test -p xtask repo_structure
```

Expected output:

```text
test result: ok.
```

---

## Task 7: PR Closeout

- [ ] Inspect the diff:

```bash
git diff --stat
git diff -- Cargo.toml xtask/src/main.rs xtask/src/checks.rs xtask/src/checks/repo_structure.rs docs/pipeline-unification/delivery/first-implementation-pr.md
```

- [ ] Confirm no public runtime code paths changed:

```bash
git diff --name-only | grep -E 'crates/axon-(cli|mcp|web|services|crawl|vector|ingest|extract|jobs|code-index)/src' || true
```

Expected output: empty, except `crates/axon-cli` remains untouched for runtime behavior. The root `src/` binary stays untouched.

- [ ] Commit:

```bash
git add Cargo.toml xtask/src docs/pipeline-unification crates
git commit -m "Add target pipeline workspace skeleton"
```

- [ ] Push and create the PR against `main`.

PR title:

```text
Add target pipeline workspace skeleton
```

PR body:

```md
## Summary

- adds the issue #298 target crate skeletons as compileable marker crates
- adds `cargo xtask check-repo-structure`
- updates first-implementation docs so PR0 is the workspace skeleton

## Runtime Behavior

No runtime behavior changes. Existing crawl, embed, ingest, ask, MCP, REST, jobs,
watch, Qdrant, and TEI paths remain on the current crates.

## Verification

- [ ] `cargo fmt --check --all`
- [ ] `cargo check --workspace --locked`
- [ ] `cargo xtask check-repo-structure`
- [ ] `cargo xtask check-layering`
- [ ] `cargo xtask check-claude-symlinks`
- [ ] `cargo test -p xtask repo_structure`
```

---

## Completion Criteria

The PR is complete when:

- The workspace compiles with all target skeleton crates.
- The repo-structure check proves target crate presence, workspace membership, and agent memory symlinks.
- The stale DTO-first delivery doc is corrected to PR0 skeleton scope.
- No public runtime surface changes.
- Issue #298 can point to this PR as the foundation for the remaining implementation sequence.
