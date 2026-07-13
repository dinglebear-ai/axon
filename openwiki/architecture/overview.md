# Architecture Overview

## Scope

This snapshot documents the repository-facing architecture around the changes in this update, with a focus on:

- Compile-time tooling (`scripts/cargo-rustc-wrapper`)
- Workflow orchestration (`.github/workflows/*`)
- CLI/developer controls (`Justfile`, `CLAUDE.md`, `README.md`)
- Action surface docs generation (`docs/reference/actions/*`, `scripts/generate_action_docs.py`)

## Source map (this update)

- `src/main.rs`: runtime startup, env loading, and top-level error redaction path.
- `scripts/cargo-rustc-wrapper`: now supports optional helper-driven wrapper delegation and `sccache-wrapper` fallback.
- `.github/workflows/ci.yml`: migrated several pinned installer actions to `jdx/mise-action`.
- `.github/workflows/openwiki-update.yml`: revised trigger/env flow and PR creation scope.
- `Justfile`: simplified install/test helper entrypoints and updated dependency-install guidance.
- `scripts/generate_action_docs.py`: generated action surface tables now treat missing parity rows as intentionally surfaced as “Not inventoried / not dedicated action” when not in matrix.

## Runtime behavior impact

The architectural control flow for Axon runtime commands and services is unchanged in this update. The relevant architecture-level changes are tooling-adjacent:

- Build tooling can now delegate rustc wrapping to a helper executable (`CARGO_BIN_ARTIFACT_WRAPPER_HELPER`) and preserves existing artifact-install behavior through `cargo-bin-artifact-wrapper` when present.
- Workflow jobs now favor `jdx/mise-action` installation steps, which affects local and CI reproducibility but not runtime command semantics.
- OpenWiki update job now writes PRs for docs-control files (`openwiki`, `AGENTS.md`, `CLAUDE.md`, workflow file), so documentation maintenance can be triggered and propagated more predictably.

## Recommended reading order

1. [source-map.md](source-map.md) for direct changed-file drill-down.
2. [workflows.md](workflows.md) for CI and automation behavior.
3. [testing.md](testing.md) for validation commands.
