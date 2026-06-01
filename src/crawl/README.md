# src/crawl
Last Modified: 2026-03-03

Crawl engine and crawl artifact manifest logic for Axon.

## Purpose
- Execute crawl runs for one or more URLs with selected render mode (`http`, `chrome`, `auto-switch`).
- Normalize and persist crawl outputs and metadata.
- Expand coverage using sitemap backfill.

## Responsibilities
- Core crawl orchestration and page collection.
- Render-mode fallback policy (including auto-switch behavior).
- Thin-page filtering and crawl output assembly.
- Crawl manifest generation and bookkeeping.

## Key Files
- `engine.rs` + `engine/`: crawl orchestration (`runtime.rs`, `collector.rs`, `map.rs`, `sitemap.rs`, `cdp_render.rs`, `thin_refetch.rs`, `url_utils.rs`, `waf.rs`, `dir_ops.rs`, `tests.rs`).
- `scrape.rs`: single-URL scrape entrypoint (HTTP + Chrome paths).
- `screenshot.rs`: screenshot capture.
- `manifest.rs`: crawl manifest model and persistence helpers.
- `chrome_bootstrap.rs`: Chrome runtime bootstrap utilities.

## Integration Points
- Invoked by `src/cli/commands/crawl*`.
- Downstream async processing and status tracking live in `src/jobs/crawl/*`.
- Embedding handoff flows into `src/vector/ops` when enabled.

## Notes
- Keep crawl behavior and job lifecycle concerns separated: traversal belongs here; queue and persistence state belong in `src/jobs`.
- Manifest format changes should be validated against downstream consumers that read crawl artifacts.

## Related Docs
- [Repository README](../../README.md)
- [Architecture](../../docs/architecture/overview.md)
- [Docs Index](../../docs/README.md)
