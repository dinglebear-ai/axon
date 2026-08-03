---
title: "Repository Structure"
created: 2026-07-15
updated: 2026-08-02
---

# Repository Structure

Axon is a Cargo workspace with a thin root binary, 23 focused Rust crates,
client applications, deployment assets, generated references, integration
tests, and repository tooling.

## Top-level layout

```text
axon/
├── src/                  thin binary shim, integration binaries, root tests
├── crates/               23 Rust workspace crates
├── apps/                 web, Android, Chrome extension, Palette desktop app
├── docs/                 living docs plus dated history directories
├── xtask/                architecture checks, generators, release tooling
├── deploy/               Incus and systemd deployment assets
├── config/               supporting-service configuration and build contexts
├── migrations/           root compatibility and bootstrap migrations
├── tests/                cross-surface and repository integration tests
├── scripts/              CI, validation, install, release, and ops scripts
├── plugins/              Axon Claude plugin and reusable skills
├── vendor/               vendored or patched dependencies
├── Cargo.toml            workspace manifest and shared product version
├── build.rs              root build integration
├── Justfile              common development and verification recipes
├── lefthook.yml          local commit and push gates
├── .env.example          endpoint, credential, and deployment template
├── config.example.toml   non-secret runtime tuning template
├── docker-compose*.yaml  development and infrastructure compositions
├── install.sh            Linux installer
└── install.ps1           Windows installer
```

## Root binary

The root crate intentionally contains only bootstrap glue:

```text
src/
├── main.rs
├── lib.rs
├── main_tests.rs
├── README.md
└── bin/
    ├── axon-openapi.rs
    └── axon-route-contracts.rs
```

`src/lib.rs` re-exports the CLI entry point. Domain and transport behavior
belongs in workspace crates, not in new root modules.

## Workspace crates

The current crate inventory is maintained in
[Crate Structure](crate-structure.md) and enforced by
`cargo xtask check-crate-contracts` and `cargo xtask check-layering`.

Broad ownership groups:

- shared contracts and policy: `axon-api`, `axon-authz`, `axon-core`,
  `axon-error`
- acquisition and source processing: `axon-route`, `axon-adapters`,
  `axon-ledger`, `axon-document`, `axon-parse`, `axon-extract`
- providers and stores: `axon-embedding`, `axon-vectors`, `axon-llm`,
  `axon-graph`, `axon-memory`, `axon-prune`
- runtime and composition: `axon-observe`, `axon-jobs`, `axon-services`
- transports: `axon-cli`, `axon-mcp`, `axon-web`

Every non-trivial crate keeps its live maintenance contract in
`crates/<name>/src/CLAUDE.md`, with `AGENTS.md` and `GEMINI.md` symlinked
to the same file.

## Client applications

| Path | Component | Authoritative documentation |
|---|---|---|
| `apps/web/` | bundled web control panel | `docs/reference/surfaces/web.md` |
| `apps/android/` | Android client | `docs/reference/surfaces/android.md` |
| `apps/chrome-extension/` | browser capture extension | `docs/reference/surfaces/chrome-extension.md` |
| `apps/palette-tauri/` | Palette desktop app | `docs/reference/surfaces/palette.md` |

Each app is versioned and released as its own component where applicable.

## Documentation

Living documentation is grouped by purpose:

- `docs/guides/`: setup and task-oriented workflows
- `docs/reference/`: factual and generated runtime contracts
- `docs/architecture/`: current system design and ownership
- `docs/operations/`: deployment, security, performance, and runbooks
- `docs/development/`: contribution and extension workflows

Dated records remain in `docs/sessions/`, `docs/plans/`,
`docs/reports/`, and `docs/superpowers/`. They are historical context, not
live runtime documentation. See [docs/README.md](../README.md).

## Generated artifacts

`cargo xtask schemas generate` and `cargo xtask docs generate` own the
machine-readable and rendered references under `docs/reference/`. Generated
files must not be hand-edited.

Primary outputs include:

- CLI command registry and help
- MCP tool schema
- REST/OpenAPI routes and schemas
- API DTO and enum references
- configuration and environment schemas
- database, event, graph, vector-payload, and provider-capability schemas
- public API and crate dependency snapshots

## Deployment and infrastructure

| Path | Purpose |
|---|---|
| `deploy/incus/` | preferred native Axon deployment in an Incus system container |
| `deploy/systemd/` | bare-metal native Axon deployment |
| `docker-compose.yaml` | development stack |
| `docker-compose.prod.yaml` | canonical supporting-service images and ports |
| `docker-compose.external-qdrant.yaml` | external Qdrant override |
| `docker-compose.external-providers.yaml` | external embedding/LLM provider override |
| `config/` | Chrome, Qdrant, and supporting-service configuration |

## Tests and repository tooling

- crate-local unit and integration tests live beside their owners
- `tests/` contains cross-crate, cross-surface, and workflow tests
- `xtask/` contains repository policy and generation code
- `scripts/` contains shell/Python support tools used by CI and operations
- `Justfile` exposes the supported local workflows
- `lefthook.yml` runs fast staged checks before commit and push

## Structure rules

1. New domain logic belongs in the owning crate.
2. Transport crates remain projections over `axon-services` and `axon-api`.
3. New generated contracts must be registered with `xtask` and checked into
   `docs/reference/`.
4. Do not add `mod.rs` files.
5. Keep root `src/` limited to binary and repository integration glue.
6. Update this page, [Crate Structure](crate-structure.md), and the owning
   crate contract whenever the repository shape changes.
