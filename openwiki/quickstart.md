# Quickstart for Repository Documentation

This OpenWiki snapshot covers repository-level documentation for the current runtime and tooling in this workspace snapshot (`HEAD=e461da278357bb594c62408b6cfb34cf47a91e14`).

## Start here

1. [Architecture overview](architecture/overview.md) — runtime shape and where key behavior lives.
2. [Domain concepts](domain-concepts.md) — how Axon surfaces and action contracts are organized.
3. [Source map](source-map.md) — files most relevant to this update.
4. [Workflows and CI](workflows.md) — what changed in workflow automation.
5. [Operations](operations.md) — runbook notes for local maintenance and updates.
6. [Testing guidance](testing.md) — updated command paths and installation expectations.
7. [Integrations](integrations.md) — related tools and interfaces mentioned in this branch.

## Why this update was needed

This OpenWiki run documents the following recent source-level changes:

- CI and OpenWiki workflows now consistently install helper CLIs via `jdx/mise-action` instead of the old `taiki-e/install-action`/manual npm install path.
- Wrapper behavior around Rust compile artifact production changed in `scripts/cargo-rustc-wrapper` to support an optional helper executable and `sccache-wrapper`.
- `Justfile` guidance was tightened: removed helper install recipes and changed several install hints to `mise`-based commands.
- OpenWiki orchestration workflow (`.github/workflows/openwiki-update.yml`) was simplified and permissioned to create PRs including documentation-control files.
- Action reference pages under `docs/reference/actions/` were rewritten at the generated surfaces block level based on parity tooling.

## Core mental model

- **Runtime path:** CLI execution still funnels through `src/main.rs` + `axon::run` with in-process services and unified web/MCP runtime entry points.
- **Doc source of truth:** this wiki points to canonical runtime and workflow evidence in the repository, not in connector evidence or external notes.
- **Update discipline:** prefer changing source (code, workflows, scripts, and docs that drive runtime), then regenerating or aligning documentation artifacts (especially generated blocks).
