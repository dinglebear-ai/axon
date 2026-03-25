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
8. Pulse API Tuning
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
5. LLM response time for `ask`/pulse

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

Queue and worker controls:

- `AXON_CRAWL_QUEUE`
- `AXON_EXTRACT_QUEUE`
- `AXON_EMBED_QUEUE`
- `AXON_INGEST_QUEUE`
- `AXON_INGEST_LANES`

Watchdog controls:

- `AXON_JOB_STALE_TIMEOUT_SECS`
- `AXON_JOB_STALE_CONFIRM_SECS`

Operational guidance:

- Increase lanes only when DB/AMQP/TEI headroom exists.
- If watchdog reclaim triggers frequently, reduce concurrency or raise stale timeout.

## Embedding and Qdrant Tuning

TEI behavior:

- batch embedding with automatic split on payload-too-large patterns
- retry on transient overload (`429` or any `5xx`) with exponential backoff
- client batch sizing via `TEI_MAX_CLIENT_BATCH_SIZE`

Embed pipeline controls:

- `AXON_EMBED_DOC_TIMEOUT_SECS`
- `AXON_EMBED_STRICT_PREDELETE`

Qdrant controls:

- `AXON_COLLECTION`
- `QDRANT_URL`
- upsert batching via `AXON_QDRANT_UPSERT_BATCH_SIZE` (default: `256` when unset)

## Ask/RAG Tuning

`ask` tuning env vars:

- `AXON_ASK_MIN_RELEVANCE_SCORE`
- `AXON_ASK_CANDIDATE_LIMIT`
- `AXON_ASK_CHUNK_LIMIT`
- `AXON_ASK_FULL_DOCS`
- `AXON_ASK_BACKFILL_CHUNKS`
- `AXON_ASK_DOC_FETCH_CONCURRENCY`
- `AXON_ASK_DOC_CHUNK_LIMIT`
- `AXON_ASK_MAX_CONTEXT_CHARS`

Tuning strategy:

1. For poor recall, raise `CANDIDATE_LIMIT` and/or lower `MIN_RELEVANCE_SCORE`.
2. To reduce latency, lower candidate/chunk limits and context chars.
3. For low answer quality on long docs, increase `FULL_DOCS` and backfill chunks gradually.

## Pulse API Tuning

Pulse endpoints (`/api/pulse/chat`, `/api/ai/copilot`) enforce upstream timeouts in their route handlers; treat route source as the current source of truth.

For high-latency models:

- use lower-latency request model (Pulse request `model` field: `haiku`/`sonnet`/`opus`)
- reduce context or citation count at caller level
- ensure TEI/Qdrant are local and low-latency

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
| pulse appears slow | upstream LLM | faster model, lower context, verify env endpoints |

## Source Map

- `README.md` (profiles and tuning flags)
- `crates/core/config/*`
- `crates/crawl/engine.rs`
- `crates/jobs/worker_lane.rs`
- `crates/jobs/common/watchdog.rs`
- `crates/vector/ops/tei/tei_client.rs`
- `crates/vector/ops/commands/*`
- `apps/web/app/api/pulse/chat/route.ts`
- `apps/web/app/api/ai/copilot/route.ts`
