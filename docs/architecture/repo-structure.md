# Repository Structure

Last Modified: 2026-07-19

The Axon repo is a Cargo workspace plus client apps, deployment configs,
generated docs, and tooling. This page maps the top-level tree and points at
the authoritative reference for each subtree.

> The contract target for this layout lives at
> [`docs/pipeline-unification/foundation/repo-structure.md`](../pipeline-unification/foundation/repo-structure.md).
> This document describes the **current** tree; divergences from the contract
> target are flagged inline.

## Top-level layout

```text
axon/
├── src/                  root binary shim (main.rs, lib.rs) + bin/
├── crates/               Cargo workspace (23 crates) — see crate-structure.md
├── apps/                 client apps (one per release component)
│   ├── web/              bundled web panel (shipped with the CLI)
│   ├── android/          Android APK
│   ├── chrome-extension/ Chrome extension
│   └── palette-tauri/    Palette Tauri desktop app (own workspace)
├── docs/                 architecture/, reference/, guides/, development/,
│                         operations/, pipeline-unification/ (design record)
├── xtask/                repo checks + generators (schemas, layering, release)
├── config/               chrome/, qdrant/, Dockerfile, mcporter.json
├── deploy/               incus/ (preferred), systemd/ (bare-metal)
├── migrations/           root-level SQLite migrations (001-003)
├── tests/                root-level integration tests + fixtures/
├── scripts/              Python/bash tooling (~60 scripts: lint, schema,
│                         install, qdrant-quality, ci/, lib/, searxng-research/)
├── plugins/              axon/ Claude plugin packaging + scripts/
├── vendor/               vendored deps (e.g. lab-auth) via [patch]
├── Cargo.toml            workspace root (version.workspace = true)
├── build.rs              root build script
├── README.md             project README
├── CLAUDE.md             agent instructions (AGENTS.md, GEMINI.md symlink here)
├── Justfile              task runner (verify, fix, precommit, services-up/down)
├── config.example.toml   non-secret tuning template
├── .env.example          URLs/secrets/auth template
├── docker-compose.yaml           dev stack (bind-mounted debug binary)
├── docker-compose.prod.yaml      canonical infra reference
├── docker-compose.external-qdrant.yaml  remote-Qdrant override
├── install.sh / install.ps1      verified one-line installers
├── rust-toolchain.toml   pins 1.96.0
├── deny.toml             cargo-deny policy
├── lefthook.yml          git hooks (monolith + test gates)
└── release-please-config.json / .release-please-manifest.json
```

## What lives where

| Path | Purpose | Authoritative reference |
|---|---|---|
| `crates/` | The 23-crate Rust workspace | [crate-structure.md](crate-structure.md) |
| `apps/` | Client apps, each a separate release component | `apps/*/README.md` |
| `deploy/incus/` | Preferred deployment (system container) | [deploy/incus/README.md](../../deploy/incus/README.md) |
| `deploy/systemd/` | Bare-metal systemd unit + walkthrough | [deploy/systemd/README.md](../../deploy/systemd/README.md) |
| `docs/reference/` | Generated + hand-written reference (source of truth) | [docs/README.md](../README.md) |
| `docs/pipeline-unification/` | Historical design-contract packet (issue #298) | [its README](../pipeline-unification/README.md) |
| `xtask/` | `check-layering`, `schemas`, `bump-version`, `check-release-versions` | `xtask/src/` |
| `config/` | Infra build contexts (chrome Dockerfile, qdrant config) | `config/Dockerfile` |
| `migrations/` | Root SQLite migrations (jobs, indexes, export tracking) | `migrations/` |
| `tests/` | Root integration tests + `fixtures/` | — |
| `vendor/` | `[patch]`-ed dependencies | root `Cargo.toml` `[patch]` |

## Divergences from the contract target

The pipeline-unification contract specified a tighter target tree. Current
divergences:

- **`config/` has no `tei/`** — TEI runs from its container image; no build
  context is needed.
- **No root-level `fixtures/` or `examples/`** — fixtures live under `tests/fixtures/`
  and inside individual crates.
- **`apps/desktop/`** (contract) does not exist; the desktop app is `palette-tauri/`.
- **`src/vector/`** exists at the root alongside `src/bin/`. The contract's
  "root crate keeps only binary/bootstrap glue" rule would prefer these move
  into a workspace crate; this is grandfathered transitional debt.
- **Root `Cargo.toml` has no `[workspace.dependencies]`** — each crate pins its
  own external deps; the shared inheritance is only `version.workspace = true`.

## Validation

- `cargo xtask check-layering` — enforces transport→domain-internal reach bans
  and the PR9 provider-crate surface ban (see
  [dependency-layering.md](dependency-layering.md)).
- `cargo xtask check-release-versions --mode pr` — the PR gate for component
  version parity across shipping paths.
- `cargo xtask schemas generate --check` — generated-docs drift check.

If this layout changes, update this file in the same PR.
