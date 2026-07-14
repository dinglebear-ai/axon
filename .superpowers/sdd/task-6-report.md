# Task 6 Report: Retained Scrape And Source-Backed Map CLI Projections

## Summary

Implemented the retained `scrape` and `map` CLI projections through the unified SourceRequest contract.

`scrape` now builds `SourceRequest { intent: Acquire, scope: Page, embed: cfg.embed }` with single-page limits (`max_items=1`, `max_pages=1`, `max_depth=0`) before calling the shared Source service renderer. This keeps `scrape` as the user-facing one-page convenience command while preserving embed-by-default behavior.

`map` now runs the existing web map discovery to produce the URL list, then hands that list to the Source pipeline as `SourceRequest { intent: Map, scope: Map, embed: false }` with `options.map_urls`. This preserves the current sitemap/crawl discovery behavior while moving the command's public output through `SourceResult` and the web adapter's map-scope manifest path.

## Changes

- `crates/axon-cli/src/commands/source.rs`
  - Exposed `run_source_request` for retained command shims.
  - Exposed `build_source_request` for in-crate projection tests.
  - Added scrape-specific SourceRequest projection: `intent=Acquire`, `scope=Page`, single-page limits, and `embed=cfg.embed`.
- `crates/axon-cli/src/commands/map.rs`
  - Added `build_map_source_request`.
  - Changed `run_map` to accept `ServiceContext`, discover map URLs, and render through `run_source_request`.
  - Removed legacy direct map-result printing from the command path to avoid double JSON output.
- `crates/axon-cli/src/lib.rs`
  - Dispatches `map` with the service context.
  - Treats `map` as requiring workers so the Source service data plane is available.
- `crates/axon-cli/src/scrape_map_source_projection_tests.rs`
  - Added coverage for retained scrape projection, `--no-embed`, map projection, and worker routing.

## Notes

- The Task 6 brief referenced a newer CLI module layout (`commands/mod.rs`, `scrape_source.rs`, `GlobalArgs`) that does not exist in the merged tree. The behavior landed in the current flattened CLI layout instead.
- The web adapter's `SourceScope::Map` currently requires caller-supplied `map_urls`; therefore `map` still uses the existing map discovery step before Source dispatch. This keeps discovery behavior stable while moving acquisition/result projection onto the unified source contract.

## Verification

- `cargo test -p axon-cli scrape_map_source_projection -- --nocapture`
- `cargo test -p axon-core source_routing -- --nocapture`
- `cargo test -p axon-cli map_payload -- --nocapture`
- `cargo test -p axon-services map -- --nocapture`
- `cargo test -p axon-adapters web_map_scope -- --nocapture`
- `cargo fmt --check`
- `git diff --check`
- `python scripts/enforce_monoliths.py --file crates/axon-cli/src/commands/map.rs --file crates/axon-cli/src/commands/source.rs --file crates/axon-cli/src/lib.rs --file crates/axon-cli/src/scrape_map_source_projection_tests.rs`
