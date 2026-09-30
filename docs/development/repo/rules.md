---
title: "Coding Rules -- Axon"
created: 2026-04-04
updated: 2026-09-29
---

# Coding Rules -- Axon

Last reviewed: 2026-09-29

This is a contributor checklist, not a second policy source. Read the
[canonical agent instructions](../../../AGENTS.md),
[contributing guide](../contributing.md), and the instructions in each
affected source scope.

## Git workflow

Work on a focused branch, preserve unrelated uncommitted changes, and submit
a PR. Use the repository's conventional commit and release rules. Do not
commit credentials, `.env` secrets, dependency/build caches, or accidental
binary artifacts. Inspect the staged diff and generated outputs before push.

## Version bumping

[release/components.toml](../../../release/components.toml) owns shipping
components, tag prefixes, workflows, and version-bearing files. Follow
[release checklist](../release-checklist.md), rather than a duplicated list of
version fields or an assumption that every file changes with every release.
Product crates use workspace metadata; client/tooling components can have
separate version contracts.

The usage plugin at `plugins/axon/` ships no Axon binary, and its manifest
must not gain a `version` key. Installer/setup behavior lives in the separate
installer plugin. Hook installation does not imply plugin provisioning.

## Monolith policy (enforced)

Rust source files are capped at 500 lines. Functions warn at 80 lines and fail
at 120. The [checker](../../../scripts/enforce_monoliths.py), its helpers, and
[.monolith-allowlist](../../../.monolith-allowlist) define actual exemptions.
Do not invent an exemption or widen the allowlist merely to pass a new change.

## Module layout (enforced)

No `mod.rs`. Use a module root such as `foo.rs` and submodules under `foo/`.
Preserve existing explicit paths, test sidecar names, cfg gates, and selectors.
`cargo check` alone does not compile or run the affected tests.

## Rust code standards

Keep transport-neutral DTOs in `axon-api`, domain logic in its owning crate,
and cross-domain orchestration in `axon-services`. Transports must not import
domain-internal `::ops::*`. Run `cargo xtask check-layering` after boundary
changes and retain its exact reviewed exception contracts.

Use the [error taxonomy](../../pipeline-unification/runtime/error-handling.md)
for stable codes, typed causes, stage/entity context, retryability, safe
side-effect reporting, and concrete recovery actions. Do not flatten provider
errors into unexplained strings or hide failure behind an empty success.
Structured results go to stdout; diagnostics/logs go to stderr and correlate
through `axon-observe`.

### Services layer contract

CLI, MCP, and HTTP call `axon-services` and public domain contracts. Shared
result DTOs do not belong in a new private transport/services type hierarchy.
Adapters emit normalized documents; they do not create family-specific
embedding pipelines or queues. One source job spans the shared pipeline.

## TypeScript code standards

Use each app's TypeScript, lint, package-manager, and test configuration.
Shared DTO/client contracts must be regenerated from their owning schemas,
not patched independently in a UI. Browser rendering, native-shell behavior,
and live API integration need separate evidence.

## Pre-commit hooks (lefthook)

[lefthook.yml](../../../lefthook.yml) and the [Justfile](../../../Justfile)
are authoritative. Repository checks include module/monolith limits,
layering, secrets, symlink correctness, and generated contracts as applicable.
The compatibility command name `check-claude-symlinks` validates canonical
`AGENTS.md` files with direct `CLAUDE.md` and `GEMINI.md` aliases.

Use [testing](../testing.md) and [documentation checks](../documentation.md)
for the affected change. A hook returning success is not evidence that
unselected tests, external providers, or a production deployment were checked.

## Performance profiles

Effective concurrency and timeouts come from typed configuration, provider
reservations, and runtime policy. Do not tune from an old CPU-multiplier table.
Consult [configuration](../../guides/configuration.md),
[pipeline performance boundaries](../../guides/pipeline-performance-boundaries.md),
and [performance operations](../../operations/performance.md). Preserve
authorization, cancellation, memory/output bounds, and publication correctness
when optimizing.
