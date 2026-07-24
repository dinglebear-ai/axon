---
type: "Reference"
title: "Quickstart for Repository Documentation"
description: "Entry point for the Axon repository wiki after the `e7d34a6b` update, with the most important links and how-to context."
---

# Quickstart for Repository Documentation

This OpenWiki snapshot covers repository-level documentation for the current runtime and tooling in this workspace snapshot (`HEAD=e7d34a6b`).

## Start here

1. [Architecture overview](architecture/overview.md) — runtime and control-plane boundaries.
2. [Source map](source-map.md) — changed files and where to inspect first.
3. [Domain concepts](domain-concepts.md) — action-surface changes and compatibility behavior.
4. [Workflows](workflows.md) — CI and OpenWiki automation behavior.
5. [Operations notes](operations.md) — local checks and maintenance habits.
6. [Testing guidance](testing.md) — what to validate for this update.
7. [Integrations](integrations.md) — external build/tool integration changes.

## Why this update was needed

This update captures source changes from `e461da278357bb594c62408b6cfb34cf47a91e14` to `e7d34a6b`.

- Re-enabled CI jobs by removing temporary `false &&` conditions while keeping path-based conditionals.
- Tightened gate enforcement in `ci.yml` with explicit skip-vs-required checks.
- Refined OpenWiki automation to run through a fixed OpenAI-compatible endpoint on Tailscale, including preflight checks.
- Updated doc control surface handling so OpenWiki updates can include `AGENTS.md`, `CLAUDE.md`, and workflow updates in its PR payload.
- Updated source-unification docs to align with the current action model and compatibility redirects.

## Core mental model

- **Runtime path:** startup still begins in `src/main.rs`, then delegates to `axon::run()` via `src/lib.rs`.
- **Build/tooling control plane:** `scripts/cargo-rustc-wrapper`, `.github/workflows/ci.yml`, and `scripts/generate_action_docs.py` shape how this repository is built, validated, and documented.
- **Data fidelity:** generated action/parity documentation (`docs/reference/api-parity.md`, `docs/reference/actions/README.md`) is the source of truth for runtime surface claims.

## Backlog

No documentation backlog is currently pending for this run.
