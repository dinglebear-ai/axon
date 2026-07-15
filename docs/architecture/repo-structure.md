# Repo Structure
Last Modified: 2026-07-15

The Axon repository is organized around product code, generated references,
apps, and delivery contracts.

## Top-Level Areas

| Path | Purpose |
|---|---|
| `src/` | root binary shim |
| `crates/` | Rust workspace crates |
| `apps/` | web, desktop, Android, and extension clients |
| `docs/` | guides, references, architecture, development docs |
| `docs/pipeline-unification/` | #298 contracts and delivery plans |
| `xtask/` | repo checks and generators |
| `config/` | container/runtime support files |
| `plugins/` | plugin and skill packaging |

## Source of Truth

Generated reference files are checked in but their source of truth is the
corresponding generator under `xtask` and the implementation crate it reads.
Hand-authored docs describe behavior and ownership, not generated tables.
