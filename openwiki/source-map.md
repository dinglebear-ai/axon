---
type: "Reference"
title: "Source Map"
description: "Changed files and high-value source paths for the `e7d34a6b` update."
---

# Source Map

## Files directly changed since previous documentation snapshot

- `src/main.rs`
- `scripts/cargo-rustc-wrapper`
- `scripts/generate_action_docs.py`
- `.github/workflows/ci.yml`
- `.github/workflows/openwiki-update.yml`
- `CLAUDE.md`
- `Justfile`
- `README.md`
- `docs/reference/actions/README.md`
- `docs/reference/api-parity.md`
- generated `docs/reference/actions/*.md` surface blocks

## Primary inspection paths

### Runtime and startup behavior

- `src/main.rs`: startup flow, bounded error-chain traversal, redaction boundary.
- `src/lib.rs`: command dispatch re-export boundary.

### Build/tooling behavior

- `scripts/cargo-rustc-wrapper`: helper and cache wrapper fallback logic.
- `Justfile`: `mise`-based install guidance and removed wrapper install recipes.

### CI/workflow behavior

- `.github/workflows/ci.yml`: re-enabled conditionals and stricter final gate check.
- `.github/workflows/openwiki-update.yml`: Tailscale/API preflight and expanded PR scope.

### Documentation-generation paths

- `scripts/generate_action_docs.py`: compatibility and surface fallback behavior.
- `docs/reference/api-parity.md`: source parity snapshot.
- `docs/reference/actions/README.md` and action files with generated `Surfaces` blocks.

## Follow-up for deeper archaeology

For implementation-level questions, prefer:

- `src/main.rs`
- `.github/workflows/ci.yml`
- `scripts/generate_action_docs.py`
- `docs/reference/api-parity.md`
- `docs/reference/actions/README.md`
