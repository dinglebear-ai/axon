# RAG Pipeline Consolidation Summary

## 1. Feasibility
- You can make a much simpler stack.
- This direction is valid: reduce dependency sprawl and keep performance acceptable.

## 2. Current Hard Blocker in This Codebase
- The CLI currently requires `AXON_PG_URL`, `AXON_REDIS_URL`, and `AXON_AMQP_URL` during config parsing, even for flows that could run synchronously.
- So today, "no Postgres/Redis/Rabbit" is blocked by startup validation, not by core crawling/embedding capability.

## 3. What Already Works Without Queues (Partially)
- `embed --wait true` already runs direct/native embedding (`embed_path_native`) and does not need queue workers in that branch.
- `crawl --wait true` still queues embed work (`start_embed_job`) instead of calling native embedding directly, so this part still depends on job infrastructure.

## 4. "Drop Workers?"
- If you remove Postgres/Redis/Rabbit and run only sync/in-process execution, worker code is no longer required for that mode.
- Keep workers only for an optional "full async mode" if you still want queue-based high-throughput operation.

## 5. TEI vs FastEmbed/Transformers Tradeoff
- TEI: better throughput/concurrency under load; ideal when ingest is parallel/heavy.
- FastEmbed/Transformers in-process: much simpler deployment, good for single-user or low-concurrency pipelines.
- Expected hit moving away from TEI: small-to-moderate for low concurrency, larger under heavier concurrent ingestion.

## 6. Qwen3 0.6B Idea
- Reasonable candidate for this simplification path.
- You need to verify exact runtime compatibility/model support in your chosen embedding backend during implementation.

## 7. "Can Qdrant + TEI Be One Container?"
- Technically possible.
- Operationally not recommended: separate containers are cleaner for lifecycle, resources, restarts, and upgrades.
- If your goal is minimal infra, keep them as two services on one host/network.

## 8. Practical Minimal Deployment Options
- Option A (most minimal): `CLI local + Qdrant + local embedder (FastEmbed/Transformers)`
- Option B (balanced): `CLI local + Qdrant + TEI`
- Option B is still only two services and usually preserves better throughput.

## 9. Proposed Implementation Direction
Implement a standalone mode (for example, `--standalone`) that:
1. bypasses required PG/Redis/AMQP env checks,
2. forces sync execution paths,
3. routes crawl/scrape/embed directly to native embedding + Qdrant,
4. disables/hides job-control subcommands in standalone mode.
