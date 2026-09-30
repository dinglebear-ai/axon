---
title: "Repository Structure"
created: 2026-07-15
updated: 2026-09-29
---

# Repository Structure

Last reviewed: 2026-09-29

The repository contains the Rust workspace, client apps, deployment examples,
contracts, documentation, and maintenance tools. This map describes tracked
source, not build products or a particular installation.

## Top-level layout

```text
axon/
├── src/                  thin binary/library bootstrap and utility binaries
├── crates/               focused Rust product crates
├── apps/
│   ├── web/              web panel assets bundled with the server
│   ├── android/          native Android client
│   ├── chrome-extension/ browser client
│   └── palette-tauri/    Palette client and standalone Tauri workspace
├── contracts/            canonical integration and protocol inputs
├── docs/                 current guides/references plus dated design/evidence
├── xtask/                repository checks and generators
├── xtask-release/        release tooling
├── deploy/               Incus and systemd deployment material
├── config/               provider/container build contexts and examples
├── plugins/
│   ├── axon/             usage plugin, skills, and Labby snippet source
│   └── install-axon/     separate installer/setup plugin
├── scripts/              maintenance, validation, release, and test tools
├── tests/                integration tests and fixtures
├── vendor/               explicitly patched dependencies
├── Cargo.toml            workspace membership and product metadata
├── Cargo.lock            resolved Rust dependency versions
├── AGENTS.md             canonical root agent instructions
├── CLAUDE.md             direct symlink to AGENTS.md
├── GEMINI.md             direct symlink to AGENTS.md
├── Justfile              developer task entry points
├── lefthook.yml          repository hook configuration
├── rust-toolchain.toml   pinned Rust toolchain
├── config.example.toml   non-secret configuration example
├── .env.example          environment/auth example
├── docker-compose*.yaml development/reference deployment surfaces
└── install.sh / install.ps1
```

Exact workspace membership and dependencies are defined by the manifests.
See [crate structure](crate-structure.md) and the
[generated dependency graph](../reference/crate-dependency-graph.md).

## What lives where

| Area | Authority and maintenance |
|---|---|
| Rust domain code | `crates/`; follow [crate ownership](crate-ownership.md) |
| Client behavior | App-local manifests, source, tests, and instructions; shared API contracts remain in Rust/OpenAPI |
| Durable SQLite state | Owning crate migrations/schema code, including jobs, ledger, graph, memory, observability, and Codex state |
| Generated contracts | Typed sources and canonical inputs, followed by ordered schema and documentation generation |
| Product guidance | Current architecture, development, guides, operations, and reference directories |
| Historical evidence | Dated plans, sessions, reports, investigations, and archived material; do not silently reinterpret as current behavior |
| Production deployment | [Incus](../../deploy/incus/README.md) or [bare-metal systemd](../../deploy/systemd/README.md), with verified local installation notes |
| Plugin packaging | Separate [usage](../../plugins/axon/README.md) and [installer](../../plugins/install-axon/README.md) manifests and instructions |

There is no root `migrations/` directory or remaining root `src/vector/`
implementation. The [database schema reference](../reference/runtime/database-schema.md)
links the current durable-state contracts; applied migrations must not be
rewritten to match documentation.

## Design targets versus the current tree

The [pipeline-unification packet](../pipeline-unification/README.md) preserves
architecture contracts and historical delivery plans. Not every directory
name in an early target tree became a real path. For example, the desktop app
is `apps/palette-tauri/`, not `apps/desktop/`; test fixtures live under
`tests/fixtures/` and individual crates, not a required root `fixtures/`.

Compose files are development/reference surfaces. Their presence does not
mean a deployed Axon service runs in Docker. Use
[deployment](../operations/deployment.md) to choose supported lifecycle
commands, and preserve an installation's unrelated configuration.

## Validation

```bash
cargo xtask check-layering
cargo xtask check-claude-symlinks
cargo xtask check-broken-symlinks
cargo xtask generated-contracts check
python3 scripts/test_operational_docs.py
```

Source/schema changes require regeneration before the final drift check.
[Documentation maintenance](../development/documentation.md) explains
generator ownership, current versus historical material, and link checks.
[Release guidance](../development/release-checklist.md) covers component version gates.

Update this map when ownership or tracked layout changes. Do not add build
artifacts, local paths, or temporary worktree state to the canonical tree.
