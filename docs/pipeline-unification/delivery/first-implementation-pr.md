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
