---
type: "Reference"
title: "Operations Notes"
description: "Operational checks and local maintenance notes after the `e7d34a6b` update."
---

# Operations Notes

## Local build/tooling adjustments

- `Justfile` dropped `nextest-install` and `llvm-cov-install` recipes in favor of `mise`-managed commands.
- `scripts/cargo-rustc-wrapper` now supports explicit/auto helper delegation and layered cache wrappers.

## Runtime/tooling validation priorities

1. **Wrapper behavior:** run a normal local build path that exercises `RUSTC_WRAPPER` (or a comparable CI-equivalent path) after wrapper-selection changes.
2. **Workflow debugging:** inspect `.github/workflows/ci.yml` change-conditioned jobs and the final `verify required jobs` phase for skipped-vs-required mismatches.
3. **OpenWiki updates:** when docs automation is suspect, inspect `openwiki-update` run logs around preflight endpoint checks and generated PR payload composition.

## OpenWiki/Docs control surfaces

- `CLAUDE.md` contains the OpenWiki navigation pointer.
- `README.md` now includes related-server links in its Related Servers section.
- `src/main.rs` and parity docs should be kept aligned with generated surface pages.

## Recurrent commands

- `python3 scripts/generate_action_docs.py` (refresh generated action surfaces)
- `python3 scripts/generate_action_docs.py --check` (if available in your local workflow)
- `cargo run --manifest-path xtask/Cargo.toml --no-default-features -- docs generate --check` (in current CI path)

## Risk watchlist

- CI endpoint availability for OpenWiki preflight (`/models`) can fail independently of source correctness.
- Wrapper fallback precedence changes should be validated before major branch merges that depend on custom artifact wrappers.
