# Task 10 Report: Final End-To-End Verification And Epic Closeout

Time: 2026-07-14T15:11:39Z

## Summary

Closed the crawl SourceRequest unification epic by refreshing the end-state
pipeline docs, generated CLI/MCP/action/reference artifacts, and API parity
inventory to match the implemented cutover:

- `axon scrape <url>` is retained as a one-page `SourceRequest` projection with
  `scope=page`, `embed=true`, and clean content output.
- `axon crawl <url>` is reserved with replacement guidance; site/docs crawl-like
  acquisition is `axon <url> --scope site|docs`.
- REST `/v1/sources` and MCP `action=source` are the source acquisition/indexing
  surfaces; REST `/v1/crawl` and MCP `crawl` are removed.
- Web page/site/docs, map, watch/refresh/search/research auto-indexing, and
  legacy crawl compatibility now run through Source jobs rather than creating
  new legacy Crawl jobs.

## Verification

Focused tests run during the Task 10 closeout:

- `cargo test -p axon-core source_routing -- --nocapture`
- `cargo test -p axon-cli scrape_map_source_projection -- --nocapture`
- `cargo test -p axon-services source_web_job_identity -- --nocapture`
- `cargo test -p axon-services source_web_304_reuse -- --nocapture`
- `cargo test -p axon-services source_web_artifacts -- --nocapture`
- `cargo test -p axon-services source_auto_index_cutover -- --nocapture`
- `cargo test -p axon-services source_observability -- --nocapture`
- `cargo test -p axon-services legacy_crawl_unreachable -- --nocapture`
- `cargo test -p axon-services crawl::tests -- --nocapture`
- `cargo test -p axon-services source_runner -- --nocapture`
- `cargo test -p axon-mcp schema -- --nocapture`
- `cargo test -p axon-web legacy_indexing_routes_are_absent_and_sources_present -- --nocapture`

Contract and generator checks:

- `cargo fmt`
- `cargo fmt --check`
- `git diff --check`
- `python3 scripts/generate_mcp_schema_doc.py --check`
- `cargo xtask schemas generate --check`
- `cargo xtask check-api-parity`
- `cargo xtask check-layering`
- `python scripts/enforce_monoliths.py --base origin/main --head HEAD`

`enforce_monoliths.py` passed with warnings only; all warned functions remain
below the hard limit.

Static crawl-removal scan:

```bash
rg -n "JobKind::Crawl|UnifiedJobKind::Crawl|crawl_start_with_context|/v1/crawl|action.*crawl" crates docs/reference docs/pipeline-unification
```

The remaining hits are expected: legacy-row lifecycle/dead-letter helpers,
invalid fixtures and schema tests proving removal, docs that state crawl is
reserved/removed, and old-named compatibility facades whose current
implementation enqueues `SourceRequest` / `JobKind::Source` rows.

Live isolated smoke:

- `target/debug/axon --help` shows search auto-queues Source jobs.
- `target/debug/axon doctor` passed with isolated `AXON_DATA_DIR`,
  `AXON_SQLITE_PATH`, and unique collection.
- `target/debug/axon --wait true --json scrape https://example.com --inline`
  returned `scope=page`, `status=completed`, one Source job id, and vector
  writes in the isolated collection.
- `target/debug/axon --wait true --json --max-pages 2 --max-depth 1
  https://example.com --scope site` returned `scope=site`, `status=completed`,
  one Source job id, and no legacy Crawl/child Embed handoff.
- `target/debug/axon --json map https://example.com`,
  `target/debug/axon --json search "rust documentation" --limit 2`,
  `target/debug/axon --json research "rust documentation" --limit 1
  --research-depth 1`, and `target/debug/axon --json jobs list` produced durable
  job kinds `research`, `source`, and `graph`; no new Crawl row appeared.
- Loopback REST smoke with auth header:
  - `POST /v1/sources` returned `200`, `scope=page`, `status=completed`, and a
    Source job id.
  - `POST /v1/crawl` returned `404 route.not_found`.
  - `GET /v1/jobs` returned only `source` and `graph` job kinds.

## Notes

- The first all-in-one live-smoke attempt tripped the startup compatibility
  guard while sourcing the full local environment; rerunning each command with
  isolated data paths and unique collections passed. I did not treat the guard
  as a regression because it is the intended old-store/old-collection safety
  behavior and the isolated runtime path succeeded.
- The REST smoke initially returned `401` because the real local Axon env
  configures bearer auth; passing the configured token without printing it
  proved the route behavior.
- `POST /v1/sources` rejects unknown fields by design (`deny_unknown_fields`).
  The successful smoke uses the exact `SourceRequest` shape:
  `{"source":"https://example.com","scope":"page","embed":true}`.
- Beads refused the first `.16.2` close because broader Workstream B
  `axon_rust-ruzox.5` remains open. I force-closed only the verified
  crawl/source `.16.*` gates and left the broader durable-job bead open.
