# Horizontal Scaling

**Tracking issue:** A-L-04
**Status:** Documentation
**Last updated:** 2026-03-04

---

## Table of Contents

1. [Current Architecture](#current-architecture)
2. [Worker Types and Scaling Characteristics](#worker-types-and-scaling-characteristics)
3. [Running Multiple Worker Containers](#running-multiple-worker-containers)
4. [Lane Count Tuning](#lane-count-tuning)
5. [What Prevents and Enables Horizontal Scaling](#what-prevents-and-enables-horizontal-scaling)
6. [Example Docker Compose Override](#example-docker-compose-override)

---

## Current Architecture

The default deployment runs a single `axon-workers` container that hosts four worker types:

| Worker | s6 service | Queue | Concurrency |
|--------|-----------|-------|------------|
| Crawl | `crawl-worker` | `axon.crawl.jobs` | 1 job at a time (spider futures are `!Send`) |
| Extract | `extract-worker` | `axon.extract.jobs` | 2 concurrent jobs (WORKER_CONCURRENCY) |
| Embed | `embed-worker` | `axon.embed.jobs` | 2 concurrent jobs |
| Ingest | `ingest-worker` | `axon.ingest.jobs` | `AXON_INGEST_LANES` lanes (default: 2) |

All workers use RabbitMQ (`axon.*.jobs` queues) as the job bus and PostgreSQL for persistent job state.

---

## Worker Types and Scaling Characteristics

### Crawl Worker

**Scaling: Vertical only (single instance)**

The crawl worker uses `spider.rs`, which produces `!Send` futures. This means the crawl loop cannot be moved between tokio threads and must run on a pinned single-threaded executor.

What this means in practice:
- One crawl job runs at a time per container
- The `--crawl-concurrency-limit` flag controls how many *pages* are fetched concurrently within a single job
- You can run multiple crawl-worker containers, but each runs one job at a time
- Multiple containers pull from the same RabbitMQ queue — PostgreSQL's `FOR UPDATE SKIP LOCKED` claim prevents double-processing

**Safe to run multiple containers:** Yes. Each container processes one job at a time, and the claim mechanism is race-safe.

### Extract Worker

**Scaling: Horizontal**

Extract jobs run on the generic `worker_lane.rs` module with `WORKER_CONCURRENCY = 2` concurrent jobs per container. The extract worker uses `reqwest` (fully `Send`), so it can run in the multi-threaded tokio runtime.

Multiple extract-worker containers pull from the same AMQP queue. No coordination beyond the claim mechanism is required.

### Embed Worker

**Scaling: Horizontal**

Same as extract. Embed workers call TEI via HTTP (reqwest). Multiple containers can run simultaneously. TEI itself becomes the bottleneck before the embed workers do — tune `TEI_MAX_CLIENT_BATCH_SIZE` and TEI's own concurrency before adding embed workers.

### Ingest Worker

**Scaling: Horizontal, lane-limited**

Ingest workers run via `worker_lane.rs`. `AXON_INGEST_LANES` (default 2) controls how many lanes (AMQP consumers) run within a single container. Each lane holds one AMQP consumer and processes one ingest job at a time.

To scale:
- Increase `AXON_INGEST_LANES` (vertical scaling — more lanes per container)
- Run multiple ingest-worker containers (horizontal scaling)

Note: GitHub/Reddit/YouTube API rate limits are the practical bottleneck for ingest, not compute.

---

## Running Multiple Worker Containers

Workers are designed to run multiple instances safely. The claim mechanism in `common::claim_next_pending` uses PostgreSQL's `SELECT ... FOR UPDATE SKIP LOCKED` — the database guarantees that two workers cannot claim the same job.

### Selective Scaling

Rather than duplicating the full `axon-workers` container (which includes all four worker types), prefer running specialized containers for the worker type you want to scale.

Each worker type has an environment variable to enable it:

| Worker | Enable/disable env var |
|--------|----------------------|
| Crawl | No flag — always starts |
| Extract | No flag — always starts |
| Embed | No flag — always starts |
| Ingest | `AXON_INGEST_LANES` (set to 0 to disable) |

Currently all four workers start together. To run isolated worker types, the s6 overlay startup scripts would need to be modified to check an env var before starting each service. This is a future improvement.

---

## Lane Count Tuning

`AXON_INGEST_LANES` controls the number of parallel ingest lanes in the ingest worker:

```bash
AXON_INGEST_LANES=2    # default — 2 parallel ingest jobs
AXON_INGEST_LANES=4    # 4 parallel ingest jobs
AXON_INGEST_LANES=0    # disable ingest worker entirely
```

Each lane is an independent AMQP consumer with its own channel. Lanes share the same PostgreSQL pool (single `PgPool` per container).

**When to increase:**
- Queue depth for `axon.ingest.jobs` is consistently > 0
- TEI and LLM endpoints have headroom
- The source APIs (GitHub, Reddit) have not rate-limited you

**When NOT to increase:**
- You are hitting GitHub API rate limits (authenticated token allows 5000 req/hour)
- TEI is at capacity (HTTP 429 responses in logs)
- PostgreSQL connection pool is exhausted (`max_connections` in pg logs)

---

## What Prevents and Enables Horizontal Scaling

### Enabling factors

1. **RabbitMQ queue-based dispatch.** Workers do not communicate directly — they pull from a shared queue. Adding workers is zero-configuration.

2. **PostgreSQL `FOR UPDATE SKIP LOCKED`.** Job claims are atomic and safe for concurrent workers without application-level locking.

3. **Stateless workers.** Workers do not hold in-memory state between jobs. A worker can be killed and restarted without losing job progress beyond the current in-flight job (which is reclaimed by the watchdog after `AXON_JOB_STALE_TIMEOUT_SECS`).

4. **AMQP reconnect with backoff.** Workers reconnect to RabbitMQ automatically on connection loss, so container restarts during broker maintenance do not require manual intervention.

### Limiting factors

1. **spider.rs `!Send` futures (crawl only).** One active crawl per container. You can run more containers, but each processes one job at a time. This is a fundamental constraint of the spider crate's architecture.

2. **TEI embedding throughput.** TEI is an external single-instance service. All embed workers share it. TEI's concurrency limit is the system bottleneck for embed-heavy workloads. Options: increase TEI concurrency (`--max-concurrent-requests`), or run multiple TEI instances with load balancing.

3. **Qdrant write throughput.** All workers write to the same Qdrant instance. Qdrant handles concurrent writes well, but very high embed rates can produce write contention. Qdrant supports sharding for horizontal scaling at the vector store layer.

4. **PostgreSQL connection pool.** Each worker container creates a `PgPool` with `max_connections=10` (default). 10 worker containers = 100 connections, reaching the Postgres default `max_connections=100`. Tune with `AXON_PG_POOL_SIZE` env var if needed, or use PgBouncer.

---

## Example Docker Compose Override

To run three crawl-worker containers in parallel, create a `docker-compose.override.yml`:

```yaml
# docker-compose.override.yml
# Run three crawl worker replicas for higher crawl throughput.
# Each replica processes one crawl job at a time.

services:
  axon-workers-2:
    build:
      context: .
      dockerfile: docker/Dockerfile
    container_name: axon-workers-2
    env_file: .env
    environment:
      - AXON_PG_URL=${AXON_PG_URL}
      - AXON_REDIS_URL=${AXON_REDIS_URL}
      - AXON_AMQP_URL=${AXON_AMQP_URL}
      - QDRANT_URL=${QDRANT_URL}
      - TEI_URL=${TEI_URL}
      - OPENAI_BASE_URL=${OPENAI_BASE_URL}
      - OPENAI_API_KEY=${OPENAI_API_KEY}
      - OPENAI_MODEL=${OPENAI_MODEL}
    networks:
      - axon
    depends_on:
      - axon-postgres
      - axon-redis
      - axon-rabbitmq
      - axon-qdrant
    restart: unless-stopped

  axon-workers-3:
    extends:
      service: axon-workers-2
    container_name: axon-workers-3

networks:
  axon:
    external: true
```

Apply with:

```bash
docker compose -f docker-compose.yaml -f docker-compose.override.yml up -d
```

### Monitoring

Watch queue depth in RabbitMQ management UI (`http://localhost:15672`) or via CLI:

```bash
docker exec axon-rabbitmq rabbitmqctl list_queues name messages consumers
```

A queue depth of 0 and consumers equal to your worker count means workers are not the bottleneck. If depth grows, add more workers.

Watch job status:

```bash
./scripts/axon status
```
