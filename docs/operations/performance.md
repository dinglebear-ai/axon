---
title: "Performance"
created: 2026-02-25
updated: 2026-08-18
---

# Performance

Axon throughput is bounded by source acquisition, document preparation,
embedding capacity, Qdrant writes, provider reservations, and the durable job
scheduler. Tune one constrained boundary at a time and keep correctness gates
enabled.

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
11. Safety Limits
12. Source Map

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

### Docs Markdown-alternate dedupe proof

For docs discovery, Axon prefers an extensionless HTML route's Markdown
representation **only when that Markdown URL was explicitly advertised by
`llms.txt`**. Independently discovered `/guide` and `/guide.md` routes are
not assumed to be equivalent. The same canonical URL key is used for both the
advertised-alternate lookup and the map dedupe, including trailing-slash and
query handling.

The preserved 2026-08-12 `code.claude.com` baseline manifest provides an
exact coverage proof for the 370 -> 187 document reduction:

- 370 original manifest items = 185 Markdown + 185 non-Markdown routes
- 183 exact extensionless/Markdown route pairs
- 2 Markdown-only routes: `/docs/en/settings.md` and
  `/docs/en/whats-new/index.md`
- 2 HTML-only routes: the site root and `/docs/en/whats-new`
- removing only the 183 paired HTML representations therefore leaves exactly
  185 Markdown + 2 HTML-only = **187 semantic document routes**

A fresh 2026-08-18 live audit independently fetched all 187 Markdown URLs
advertised by `code.claude.com/llms.txt` and all 187 extensionless
counterparts: 187/187 pairs returned HTTP 200 with zero fetch errors. The
current sitemap explicitly lists 186 of those HTML counterparts; the remaining
counterpart is live but omitted from the sitemap. This proves route coverage,
not byte-for-byte identity between the HTML and Markdown representations.

Unit tests additionally assert that advertised alternate replacement preserves
the complete semantic route set, while unadvertised HTML/Markdown siblings are
kept independently.

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
- client batch sizing via `providers.embedding.batch-size` in `~/.axon/config.toml`

Measured RTX 4070 + `Qwen/Qwen3-Embedding-0.6B` docs-chunk profile:

- use `TEI_MAX_BATCH_TOKENS=196608` for the current local profile; reduce it
  if TEI fails warmup with CUDA OOM
- use `TEI_MAX_BATCH_REQUESTS=512` to avoid false overloads when multiple real
  docs batches are in flight
- keep Axon's client batch at `TEI_MAX_CLIENT_BATCH_SIZE=96` for this
  profile. The preserved 370-document provider-concurrency candidate ran in
  92.1 s at 96/8/320, while a later 128-input cold smoke on the same corpus
  ran in 110.2 s. The deduplicated 187-document run also used 96 and completed
  in 68.3 s with 47 TEI requests. Treat those as hardware-profile evidence,
  not a universal batch-size rule; re-benchmark before changing the default
- keep `AXON_EMBED_POOL_MAX_INPUTS=512` for docs-style corpora so small files
  are pooled before TEI client-side sub-batching
- `AXON_TEI_MAX_CONCURRENT=8` is a reasonable single-process ceiling when the
  server batch-request budget is `512`
- `AXON_TEI_MAX_IN_FLIGHT_INPUTS=320` caps `batch_size * request_concurrency`,
  so small batches can use more request concurrency without large batches
  stampeding into TEI overload
- the 2026-08-18 live RTX 4070 validation at `TEI_MAX_BATCH_TOKENS=196608`
  ran six repeated realistic 96+96+96+32 input waves, exactly 320 simultaneous
  inputs, with 24/24 HTTP 200 responses, zero TEI restarts, and a 3,086 MiB
  observed VRAM peak. A separate deliberately long-chunk 96+96+96+32 wave also
  returned 4/4 HTTP 200 and peaked at 11,086 MiB of the 12,282 MiB GPU with no
  OOM or admission rejection. A deliberately **out-of-envelope** 8x96-input
  long-chunk stress wave reached TEI admission backpressure (HTTP 429) rather
  than CUDA OOM. Keep the 320-input client gate: it is part of this 196608
  safety profile, especially because the long-chunk case uses most available
  VRAM

Embed pipeline controls:

- `workers.embed-doc-timeout-secs` in `~/.axon/config.toml`

Qdrant controls:

- `search.collection` in `~/.axon/config.toml`
- `QDRANT_URL`
- `workers.qdrant-point-buffer=1024` batches points before each pipeline flush
- upsert batching via `qdrant.upsert-batch-size` in `~/.axon/config.toml`
  (env override: `AXON_QDRANT_UPSERT_BATCH_SIZE`; default `1024`)
- upsert fanout via `qdrant.upsert-parallelism` in `~/.axon/config.toml`
  (env override: `AXON_QDRANT_UPSERT_PARALLELISM`; default `1`). This is a
  process-shared point/generation-write request ceiling for stores using the
  same Qdrant endpoint/admission profile; durable vector scheduler slots govern
  logical operations separately. Payload-index creation has its own bounded
  `qdrant.payload-index-parallelism` gate.
  Qdrant's generic bulk-upload guidance suggests `64-256` point batches with
  `2-4` parallel streams; on the local `code.claude.com` docs corpus,
  `1024/1` measured faster, so treat `256/2-4` as a large-import tuning profile
  to validate with `bench-embed`
- payload-index creation fanout via `qdrant.payload-index-parallelism`; requests
  are bounded client-side even though Qdrant may serialize index work internally
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

Crawl benchmark (quick manual timing):

```bash
time ./scripts/axon source https://example.com --scope site --wait true --performance-profile high-stable
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

### Reproducible live source benchmark

Use the `xtask` harness when comparing pipeline changes. It runs against the
real site and live TEI/Qdrant services, while isolating each cold run's SQLite,
crawl cache, and Qdrant collection. Live network access must be acknowledged
explicitly, so CI and ordinary developer commands cannot accidentally crawl an
external site.

Capture a three-run cold and warm baseline:

```bash
cargo xtask bench-source https://code.claude.com/ \
  --axon-bin target/release/axon \
  --scenario both \
  --runs 3 \
  --allow-live-network \
  --output target/bench-source/code-claude-baseline.json
```

The warm scenario performs one unmeasured cache-primer crawl, then measures
conditional recrawls using `--cache true --etag-conditional`. The cold scenario
uses a new state directory and collection for every measured run. The harness
removes those generated resources unless `--keep-state` is supplied.

After changing the pipeline, compare the candidate directly with the baseline:

```bash
cargo xtask bench-source https://code.claude.com/ \
  --axon-bin target/release/axon \
  --scenario both \
  --runs 3 \
  --allow-live-network \
  --baseline target/bench-source/code-claude-baseline.json \
  --output target/bench-source/code-claude-candidate.json
```

The JSON artifact records:

- Git revision, branch, Axon version, scenario, and page cap
- Total wall time plus durable-job phase timings
- Discovered items, prepared documents, chunks, and stored vector points
- Completion/degradation status and warning counts by stable warning code
- TEI inputs, requests, input tokens, embedding time, and queue time when the
  service exposes Prometheus metrics
- Qdrant upsert request/time deltas when the service exposes matching metrics
- Min, median, and max distributions, plus baseline percentage changes for
  time, pages, documents, chunks, and vector points

Treat page/document/chunk/vector deltas as correctness signals, not automatic
performance wins. Live sites can change between runs; make baseline and
candidate runs close together, use the same binary profile and services, and
repeat noisy comparisons. Use `--max-pages` for cheap harness smoke tests, not
for final throughput claims.

### Isolated crawler stress run

Use the dedicated harness for the final live load phase. It defaults to a
non-mutating JSON plan:

```bash
scripts/stress-crawler.sh --mode plan \
  --url https://docs.example.com/
```

Run a cheap end-to-end check before the heavy pass:

```bash
scripts/stress-crawler.sh --mode smoke
```

Heavy mode requires an explicit target and confirmation. It maps the site
first, targets at least 500 pages when available, queues concurrent page jobs
beside the main site crawl, and uses the existing external Qdrant endpoint.
It never starts a container runtime or Qdrant:

```bash
AXON_STRESS_CONFIRM=CRAWL_AND_DELETE_ISOLATED_STATE \
  scripts/stress-crawler.sh --mode heavy \
  --url https://docs.example.com/ \
  --max-pages 500 \
  --concurrent-jobs 8
```

Each run owns an `axon_stress_*` collection and an isolated
`AXON_DATA_DIR`/SQLite database. The exit trap deletes both even after failure.
The retained report contains discovery evidence, per-job latency, p50/p95/max
latency, document/chunk/vector throughput, terminal counts, graph counts,
error counts, Qdrant point verification, and durable provider-reservation rows
for the tracked jobs. Verification fails if any reservation remains requested,
queued, granted, or active after those jobs are terminal. Prepared chunks are
pre-redaction while Qdrant points are post-redaction, so the
report records secret-policy skips and the resulting point delta explicitly;
it requires nonzero publication rather than falsely requiring those counts to
match.
Failed runs still retain a structured report with terminal/error evidence and
verified SQLite/Qdrant cleanup. Heavy mode rejects loopback and the
`axon-qdrant` Compose-service hostname.

## Symptom -> Tuning Matrix

| Symptom | Likely bottleneck | First knobs |
|---|---|---|
| crawl is slow but stable | fetch/render | profile -> `extreme`, increase crawl concurrency |
| many thin pages | rendering mismatch | `--render-mode chrome` or `auto-switch` |
| embed backlog grows | TEI throughput | lower batch/lane pressure, increase TEI capacity |
| frequent stale reclaim | worker overload | reduce concurrency, raise stale timeout |
| `ask` too slow | context size/LLM latency | lower candidate/chunk/context limits |
| HTTP/MCP action appears slow | upstream TEI/Qdrant/LLM or network latency | compare with local CLI, lower ask context, verify service endpoints |

## Safety Limits

Do not remove SSRF checks, authorization, redaction, cancellation polling,
generation publication rules, provider reservation cleanup, or cleanup-debt
handling to gain throughput. Those boundaries are part of correctness and the
benchmark/stress harnesses are expected to keep them enabled.

## Source Map

- `README.md` (profiles and tuning flags)
- `crates/axon-core/src/config/` (runtime configuration and tuning)
- `crates/axon-adapters/src/web_engine/` (web acquisition and discovery)
- `crates/axon-services/src/source/` (unified source execution)
- `crates/axon-embedding/src/tei.rs` (embedding provider admission)
- `crates/axon-vectors/src/qdrant/` (Qdrant writes and indexing)
- `crates/axon-jobs/src/scheduler.rs` (durable provider scheduling)
- `crates/axon-web/src/server/` (REST/MCP-adjacent HTTP surfaces)
