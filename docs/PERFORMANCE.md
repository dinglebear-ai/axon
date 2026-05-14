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
- Sitemap backfill cap is currently fixed at `512` (not a user-exposed CLI flag).

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

Embed pipeline controls:

- `workers.embed-doc-timeout-secs` in `~/.axon/config.toml`

Qdrant controls:

- `search.collection` in `~/.axon/config.toml`
- `QDRANT_URL`
- upsert batching via `AXON_QDRANT_UPSERT_BATCH_SIZE` (default: `256` when unset)

## Ask/RAG Tuning

Core `ask` tuning lives in `~/.axon/config.toml`:

- `ask.min-relevance-score`
- `ask.candidate-limit`
- `ask.chunk-limit`

Remaining runtime controls are env-only until typed TOML fields exist:
- `AXON_ASK_FULL_DOCS`
- `AXON_ASK_BACKFILL_CHUNKS`
- `AXON_ASK_DOC_FETCH_CONCURRENCY`
- `AXON_ASK_DOC_CHUNK_LIMIT`
- `AXON_ASK_MAX_CONTEXT_CHARS`

Tuning strategy:

1. For poor recall, raise `ask.candidate-limit` and/or lower `ask.min-relevance-score`.
2. To reduce latency, lower candidate/chunk limits and context chars.
3. For low answer quality on long docs, increase `FULL_DOCS` and backfill chunks gradually.

## Server-Mode HTTP Tuning

`axon serve` exposes MCP, `/v1/ask`, `/v1/actions`, and the setup/config panel
on one Axum listener. Stateful CLI server-mode commands run through
`/v1/actions` and share the same service layer as local CLI/MCP execution.

For high-latency LLM or embedding paths:

- keep TEI/Qdrant local or on low-latency links
- reduce ask context/candidate limits before increasing worker lanes
- run CLI commands with `AXON_LOCAL_MODE=1` when you need to bypass a remote server

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
time ./scripts/axon embed docs/ARCHITECTURE.md --wait true
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
| server-mode action appears slow | upstream TEI/Qdrant/LLM or remote latency | compare with `AXON_LOCAL_MODE=1`, lower ask context, verify service endpoints |

## Source Map

- `README.md` (profiles and tuning flags)
- `src/core/config/*`
- `src/crawl/engine.rs`
- `src/vector/ops/tei/tei_client.rs`
- `src/vector/ops/commands/*`
- `src/web/actions.rs`
- `src/web/server.rs`
