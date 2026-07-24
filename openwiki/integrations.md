---
type: "Reference"
title: "Integrations"
description: "External and internal integration changes touched by the `e7d34a6b` update."
---

# Integrations

## External integrations affected indirectly

- CI tooling integration shifts:
  - `taiki-e/install-action` usage in specific flows is being replaced by `jdx/mise-action` with pinned package IDs in selected jobs.
  - `mcporter` is installed via mise shim and pinned.
- OpenWiki workflow now runs against a Tailscale-accessible OpenAI-compatible API and performs preflight checks.

## Repository control integrations

- OpenWiki automation now includes control files (`AGENTS.md`, `CLAUDE.md`, workflow file) in generated update PRs, improving documentation-control alignment.

## Related project references

- README has added RMCP-related “Related Servers” links that are useful for ecosystem context.

## For maintainers

If integration behavior drifts:

- verify `.github/workflows/openwiki-update.yml` endpoint and secret wiring,
- confirm `.github/workflows/ci.yml` still aligns installer IDs and expected tool behavior,
- inspect `scripts/cargo-rustc-wrapper` for wrapper helper precedence changes before changing wrapper-specific build infra.
