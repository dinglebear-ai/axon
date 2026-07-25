---
type: "Reference"
title: "Domain Concepts"
description: "Action-surface and control-plane concepts introduced/adjusted in the `e7d34a6b` update."
---

# Domain Concepts

## What changed in this update

### Runtime and error-handling concept

`src/main.rs` now enriches user-facing failure text by traversing `Error::source()` chains with a bounded depth (16), then applying transport redaction. This keeps CLI failures actionable while preserving privacy boundaries.

### Wrapper delegation concept

`scripts/cargo-rustc-wrapper` now follows a layered execution model:

1. explicit helper override (`CARGO_BIN_ARTIFACT_WRAPPER_HELPER`),
2. auto-discovered `cargo-bin-artifact-wrapper`,
3. `sccache-wrapper`,
4. `sccache`,
5. bare `rustc`.

This supports configurable wrapper composition without changing callers.

### Action-surface compatibility concept

The action docs model moved to a unified `source` interpretation for removed source-family commands:

- `github`, `reddit`, `youtube` are now documented as CLI compatibility entrypoints (`axon <source>`) that dispatch through `source` semantics.
- `crawl`, `ingest`, and `code-search` docs now explicitly indicate deprecation-style removal with migration guidance.

`scripts/generate_action_docs.py` now emits compatibility and “Not inventoried” fallbacks when parity data is absent, so generated pages are explicit about surface coverage.

### OpenWiki workflow control concept

`openwiki-update.yml` now:

- runs a preflight API check before generation,
- includes additional control files in its PR payload,
- and runs with explicit helper environment and fixed endpoint settings.

## Related files

- `src/main.rs`
- `scripts/cargo-rustc-wrapper`
- `scripts/generate_action_docs.py`
- `docs/reference/api-parity.md`
- `docs/reference/actions/README.md`
- `docs/reference/actions/*.md`
- `.github/workflows/openwiki-update.yml`

## Why this matters

- Error handling gives operators better root-cause visibility without widening secret exposure.
- Wrapper delegation supports team-specific tool orchestration.
- Action docs now reflect the unified source contract and removed-compatibility behavior consistently.
- Automated OpenWiki PRs now refresh more authoritative control files, reducing drift.

## Next-linking for related concepts

- [src/main.rs](architecture/overview.md#runtime-behavior-impact)
- [Workflow gates](workflows.md)
- [Action parity matrix](../docs/reference/api-parity.md)
- [Action index](../docs/reference/actions/README.md)
