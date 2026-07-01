# Performance Tuning Guide
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 2026-02-25T01:26:53-05:00

## Table of Contents

1. Scope
2. Throughput Model
3. Global Performance Profiles
4. Crawl Tuning
5. Worker and Queue Tuning
6. Embedding and Qdrant Tuning
7. Ask/RAG Tuning
8. Server-Mode HTTP Tuning
9. Benchmark Workflow
10. Symptom -> Tuning Matrix
11. Source Map

## Scope

This document describes available performance controls in Axon and how to tune them safely.

## Throughput Model

Overall throughput is constrained by the slowest stage:

1. Crawl fetch/render
2. Content transform/chunking
3. TEI embedding throughput
4. Qdrant upsert/search throughput
5. LLM response time for `ask`

Tune one bottleneck at a time.

## Global Performance Profiles

Use `--performance-profile`:

- `high-stable` (default)
- `balanced`
- `extreme`
- `max`

Profiles control:

- concurrency limits
- request timeouts
- retry count and backoff

Override at runtime:

- `--batch-concurrency`
- `--concurrency-limit`
- `--crawl-concurrency-limit`
- `--backfill-concurrency-limit`
- `--request-timeout-ms`
- `--fetch-retries`
- `--retry-backoff-ms`

## Crawl Tuning

Primary flags:

- `--render-mode` (`http`, `chrome`, `auto-switch`)
- `--max-pages`
- `--max-depth`
- `--include-subdomains`
- `--discover-sitemaps`
- `--min-markdown-chars`
- `--drop-thin-markdown`
- `--delay-ms`

Guidance:

- Start with `http` when sites are static; use `auto-switch` for mixed sites.
- Use `delay-ms` to reduce target pressure and avoid defensive throttling.
- Keep `drop-thin-markdown=true` for higher-quality embedding corpus.
- Sitemap backfill cap defaults to `512` and is configurable via `scrape.max-sitemaps` in `~/.axon/config.toml` (no CLI flag). Restrict backfill by recency with `--sitemap-since-days <n>`.

### Adaptive Crawl Concurrency

Adaptive crawl concurrency is opt-in via TOML:

```toml
[workers.adaptive-concurrency]
enabled = true
min = 1
# max = 64
```

Defaults are unchanged when it is disabled. Adaptive mode applies to the main Spider crawl path; sitemap backfill, standalone screenshots, and other fetch helpers keep their existing fixed limits. HTTP `429`, HTTP `5xx`, and broadcast lag apply negative pressure; successful statuses increase after Spider's fixed success threshold. Spider 2.52.0 halves on failure, so Axon does not expose `decrease-factor`, `sync-interval-ms`, or palette controls for this release.

Shrinking the target limits future admission and does not cancel already in-flight requests. Use adaptive mode with polite crawl settings such as robots, delay, max pages, path budgets, or a URL whitelist.

## Worker and Queue Tuning

Worker controls:

- `workers.ingest-lanes` in `~/.axon/config.toml`

Watchdog controls:

- `AXON_JOB_STALE_TIMEOUT_SECS`
- `AXON_JOB_STALE_CONFIRM_SECS`

Operational guidance:

- Increase lanes only when SQLite, Qdrant, and TEI headroom exists.
- If watchdog reclaim triggers frequently, reduce concurrency or raise stale timeout.

## Embedding and Qdrant Tuning

TEI behavior:

- batch embedding with automatic split on payload-too-large patterns
- retry on transient overload (`429` or any `5xx`) with exponential backoff
- client batch sizing via `tei.max-client-batch-size` in `~/.axon/config.toml`

Measured RTX 4070 + `Qwen/Qwen3-Embedding-0.6B` docs-chunk profile:

- use `TEI_MAX_BATCH_TOKENS=196608` for the current local profile; reduce it
  if TEI fails warmup with CUDA OOM
- use `TEI_MAX_BATCH_REQUESTS=512` to avoid false overloads when multiple real
  docs batches are in flight
- keep Axon's client batch around `TEI_MAX_CLIENT_BATCH_SIZE=128`; on the
  `code.claude.com` docs corpus this reduced TEI calls to 37 and was faster
  than 96, 192, and 256
- keep `AXON_EMBED_POOL_MAX_INPUTS=512` for docs-style corpora so small files
  are pooled before TEI client-side sub-batching
- `AXON_TEI_MAX_CONCURRENT=8` is a reasonable single-process ceiling when the
  server batch-request budget is `512`
- `AXON_TEI_MAX_IN_FLIGHT_INPUTS=320` caps `batch_size * request_concurrency`,
  so small batches can use more request concurrency without large batches
  stampeding into TEI overload

Embed pipeline controls:

- `workers.embed-doc-timeout-secs` in `~/.axon/config.toml`

Qdrant controls:

- `search.collection` in `~/.axon/config.toml`
- `QDRANT_URL`
- `workers.qdrant-point-buffer=1024` batches points before each pipeline flush
- upsert batching via `qdrant.upsert-batch-size` in `~/.axon/config.toml`
  (env override: `AXON_QDRANT_UPSERT_BATCH_SIZE`; default `1024`)
- upsert fanout via `qdrant.upsert-parallelism` in `~/.axon/config.toml`
  (env override: `AXON_QDRANT_UPSERT_PARALLELISM`; default `1`).
  Qdrant's generic bulk-upload guidance suggests `64-256` point batches with
  `2-4` parallel streams; on the local `code.claude.com` docs corpus,
  `1024/1` measured faster, so treat `256/2-4` as a large-import tuning profile
  to validate with `bench-embed`
- fresh-collection bulk indexing profile via `qdrant.bulk-load=true`
  (env override: `AXON_QDRANT_BULK_LOAD=true`): Axon creates the collection
  with `qdrant.bulk-indexing-threshold-kb` and restores
  `qdrant.indexing-threshold-kb` after the embed pipeline finishes
- HNSW build cost for new collections via `qdrant.hnsw-m` and
  `qdrant.hnsw-ef-construct`; lower values can speed indexing but must be
  validated with exact-vs-approx recall before becoming a quality default
- fresh payload-index cost via `qdrant.payload-index-profile=core`, which
  creates only URL/domain/source/schema/time indexes for docs-style collections;
  keep `full` for mixed code/package/social collections unless evaluated

## Ask/RAG Tuning

Core `ask` tuning lives in `~/.axon/config.toml`:

- `ask.min-relevance-score`
- `ask.candidate-limit`
- `ask.chunk-limit`

Additional ask controls now live in TOML as:

- `ask.full-docs`
- `ask.backfill-chunks`
- `ask.doc-fetch-concurrency`
- `ask.doc-chunk-limit`
- `ask.max-context-chars`

Tuning strategy:

1. For poor recall, raise `ask.candidate-limit` and/or lower `ask.min-relevance-score`.
2. To reduce latency, lower candidate/chunk limits and context chars.
3. For low answer quality on long docs, increase `FULL_DOCS` and backfill chunks gradually.

## Server-Mode HTTP Tuning

`axon serve` exposes MCP, `/v1/ask`, direct `/v1` REST routes, and the setup/config panel
on one Axum listener. External HTTP/MCP clients call those routes directly.
The bundled CLI no longer performs generic server-mode forwarding.

For high-latency LLM or embedding paths:

- keep TEI/Qdrant local or on low-latency links
- reduce ask context/candidate limits before increasing worker lanes
- compare HTTP/MCP latency against the same command run locally in-process

## Benchmark Workflow

Baseline:

```bash
./scripts/axon doctor
./scripts/axon stats
```

Crawl benchmark:

```bash
time ./scripts/axon crawl https://example.com --wait true --performance-profile high-stable
```

Embedding benchmark:

```bash
time ./scripts/axon embed docs/architecture/overview.md --wait true
```

RAG benchmark:

```bash
time ./scripts/axon ask "summarize architecture" --limit 10
```

Track:

- total duration
- pages/chunks processed
- error/retry frequency
- worker saturation signals in logs

## Symptom -> Tuning Matrix

| Symptom | Likely bottleneck | First knobs |
|---|---|---|
| crawl is slow but stable | fetch/render | profile -> `extreme`, increase crawl concurrency |
| many thin pages | rendering mismatch | `--render-mode chrome` or `auto-switch` |
| embed backlog grows | TEI throughput | lower batch/lane pressure, increase TEI capacity |
| frequent stale reclaim | worker overload | reduce concurrency, raise stale timeout |
| `ask` too slow | context size/LLM latency | lower candidate/chunk/context limits |
| HTTP/MCP action appears slow | upstream TEI/Qdrant/LLM or network latency | compare with local CLI, lower ask context, verify service endpoints |

## Source Map

- `README.md` (profiles and tuning flags)
- `src/core/config/*`
- `src/crawl/engine.rs`
- `src/vector/ops/tei/tei_client.rs`
- `src/vector/ops/commands/*`
- `src/web/server/handlers/rest/*` (server-mode REST + ask routes)
- `src/web/server.rs`
