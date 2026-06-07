# Axon Ask Results - gemini-3.5-flash-low via cli-api.tootie.tv

- Date: 2026-06-07
- Binary: `axon 5.1.2`
- Env file: `/home/jmagar/workspace/axon/reports/llm-ask-comparison-2026-06-07/env/gemini-3.5-flash-low.env`

## Effective Model Config

```text
  AXON_ASK_BACKFILL_CHUNKS = 1
  AXON_ASK_DOC_CHUNK_LIMIT = 24
  AXON_ASK_DOC_FETCH_CONCURRENCY = 1
  AXON_ASK_FULL_DOCS = 1
  AXON_LLM_BACKEND = openai-compat
  AXON_OPENAI_BASE_URL = https://cli-api.tootie.tv/v1
  AXON_OPENAI_MODEL = gemini-3.5-flash-low
  ask.adaptive.fulldoc-skip-enabled = false
  ask.adaptive.fulldoc-skip-min-chars = 4000
  ask.adaptive.fulldoc-skip-min-urls = 3
  ask.adaptive.fulldoc-skip-score-delta = 0.15
  ask.authoritative-boost = 0.12
  ask.authoritative-domains = ["code.claude.com"]
  ask.backfill-chunks = 5
  ask.cache.enabled = false
  ask.cache.max-capacity-bytes = 268435456
  ask.cache.ttl-secs = 300
  ask.doc-chunk-limit = 96
  ask.doc-fetch-concurrency = 4
  ask.full-docs = 6
  ask.min-citations-nontrivial = 2
  ask.min-relevance-score = 0.45
  search.hybrid-candidates = 100
  search.hybrid-enabled = true
```

## Questions

1. What is Axon's high-level architecture and which subsystems handle crawl, jobs, vector search, MCP, and web serving?
2. How does Axon's ask pipeline retrieve context and synthesize an answer from indexed documents?
3. What is the services-first contract in Axon, and why should CLI, MCP, and web routes share service functions?
4. How do Axon's SQLite-backed async jobs work, and what does --wait true change for crawl, extract, embed, and ingest commands?
5. What does Axon's watch scheduler do today, and which watch subcommands are implemented versus only parsed?
6. How does Axon choose between SearXNG and Tavily for search or research, and what happens to discovered results?
7. What are Axon's LLM backend options, and how should AXON_OPENAI_BASE_URL be formatted for openai-compat?
8. What are the important Spider feature-flag gotchas in Axon, especially firewall, balance, and glob?
9. How does Axon's hybrid vector search work with Qdrant dense and BM42 sparse vectors, and when does it fall back to dense-only?
10. What configuration and runtime safeguards prevent Axon ask from overloading smaller-context local models?

## Results

### Q1

**Question:** What is Axon's high-level architecture and which subsystems handle crawl, jobs, vector search, MCP, and web serving?

**Answer:**

Conversation
  You: What is Axon's high-level architecture and which subsystems handle crawl, jobs, vector search, MCP, and web serving?
  Assistant:
Axon is designed around a dual-mode, single-binary architecture that operates in both CLI mode (`axon <cmd>`) and MCP mode (`axon mcp`) [S7]. All user execution entry points—including CLI command handlers, MCP tool handlers, and HTTP REST web routes—interface directly with a unified services layer (`src/services/` or the `axon-services` crate), which encapsulates the system's business logic and routes requests to downstream infrastructure [S6][S7][S8]. The codebase is structured as a Cargo workspace with subsystem dependency boundaries enforced at the compiler level via a directed acyclic graph [S1].

The specific subsystems responsible for the requested operations are organized as independent crates:
* **Crawl**: Handled by the `axon-crawl` crate (or `src/crawl/` module), which contains the spider-based crawl engine, page scraping, sitemap backfill, and screenshot capabilities [S1][S2][S3][S5].
* **Jobs**: Handled by the `axon-jobs` crate (or `src/jobs/` module), which houses the SQLite-backed job framework and worker runtime for processing asynchronous tasks [S1][S2][S3][S5].
* **Vector search**: Handled by the `axon-vector` crate (or `src/vector/` module), which manages Qdrant vector database operations, Hugging Face Text Embeddings Inference (TEI) integrations, and hybrid search RAG pipelines [S1][S3][S5][S7].
* **MCP**: Handled by the `axon-mcp` crate (or `src/mcp/` module), which implements the Model Context Protocol (MCP) server schema, stdio/HTTP transport, and OAuth functionality [S1][S3][S4][S5].
* **Web serving**: Handled by the `axon-web` crate (or `src/web/` module), which manages the Axum server, REST API endpoints (such as `/v1/*`), and WebSocket connections [S1][S5][S9].

## Sources
- [S1] https://github.com/jmagar/axon/issues/18
- [S2] https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L1-L68
- [S3] https://github.com/jmagar/axon/blob/main/docs/INVENTORY.md#L89-L137
- [S4] https://github.com/jmagar/axon/blob/main/docs/mcp/DEV.md#L1-L45
- [S5] https://github.com/jmagar/axon/blob/main/docs/reference/inventory.md#L151-L187
- [S6] https://github.com/jmagar/axon/blob/main/docs/mcp/PATTERNS.md#L1-L66
- [S7] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L1-L70
- [S8] https://github.com/jmagar/axon/blob/main/docs/plans/2026-03-11-modular-workspace-and-capability-gating.md#L45-L116
- [S9] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-05-06-config-decomposition-plan.md#L123-L180
  Timing: retrieval=2816ms | context=1ms | llm=5394ms | total=8220ms | ttft=7725ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 60 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/overview.md#L1-L65 ≈ https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L1-L68)


_Status: `0`, elapsed: `8s`_

### Q2

**Question:** How does Axon's ask pipeline retrieve context and synthesize an answer from indexed documents?

**Answer:**

Conversation
  You: How does Axon's ask pipeline retrieve context and synthesize an answer from indexed documents?
  Assistant:
Axon's `ask` pipeline retrieves context and synthesizes answers through a five-stage process: embedding, retrieval, filtering, reranking, and context building, before finally calling the LLM [S2]. First, the user's query is converted into dense vectors using Text Embeddings Inference (TEI) [S2][S3]. A query instruction is prepended to query-side embeddings to optimize retrieval, whereas document chunks are embedded as raw text [S2]. Axon then queries Qdrant to retrieve up to a configured candidate limit (default: 250 candidate chunks) [S3][S5]. This retrieval can utilize dense search or hybrid search (combining BM42 sparse vectors and dense vectors with Reciprocal Rank Fusion) [S1][S3][S5].

Once candidates are retrieved, Axon filters and reranks the pool [S2][S3]. A minimum relevance score threshold (default: 0.45) is applied on cosine/dense retrieval paths to filter out weak candidates [S3][S5][S11]. This cosine-based threshold is skipped on hybrid search paths because RRF scores are unitless rank-fusion outputs [S3][S5]. Candidates are then reranked by the mode-appropriate score, and the pipeline selects the top chunks (default: top 20) [S3][S5].

The context is assembled into a single string in three distinct tiers, separated by delimiters, up to a hard character cap (default: 300,000 characters) [S6][S10]. Tier 1 (Top Chunks) selects up to the chunk limit (default: 20) with a diversity constraint of at most one chunk per unique URL per selection pass [S6]. Tier 2 (Full Documents) concurrently fetches all stored chunks from Qdrant for the top-ranked URLs (default: up to 6 documents) [S6][S12]. Tier 3 (Supplemental Chunks) backfills additional chunks from the remaining pool if the context is under 85% capacity [S6].

Finally, the assembled context is formatted with a `Sources:` header and injected directly into the LLM prompt alongside the question [S2][S9][S12]. Axon runs Gemini headless (or an OpenAI-compatible backend) at a fixed temperature of 0.1 to generate the response [S7][S12]. The system prompt (`axon-rag-synthesize`) enforces that the LLM must answer exclusively using the retrieved context, use inline citations like `[S#]`, state coverage gaps, and generate a `## Sources` footer [S4][S9][S12]. Before printing, Axon runs response-quality gates to check citation compliance, such as requiring at least 2 unique citations for non-trivial answers [S3][S8].

## Sources
- [S1] example.com/api/reference
- [S2] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L1-L60
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L83-L118
- [S4] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-05-12-ask-quality-skill-prompt.md#L23-L71
- [S5] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L68-L115
- [S6] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L141-L176
- [S7] https://github.com/jmagar/axon/blob/main/docs/ASK.md#L1-L47
- [S8] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L212-L228
- [S9] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-02-19-ask-prompt-redesign-plan.md#L1-L42
- [S10] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L225-L258
- [S11] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L253-L281
- [S12] https://github.com/jmagar/axon/blob/main/docs/guides/context-injection.md#L176-L225
  Timing: retrieval=3046ms | context=1ms | llm=6892ms | total=9947ms | ttft=8750ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 36 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/guides/context-injection.md#L1-L60 ≈ https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L1-L60)


_Status: `0`, elapsed: `10s`_

### Q3

**Question:** What is the services-first contract in Axon, and why should CLI, MCP, and web routes share service functions?

**Answer:**

Conversation
  You: What is the services-first contract in Axon, and why should CLI, MCP, and web routes share service functions?
  Assistant:
The "services-first" contract in Axon dictates that all feature and business logic must reside within the shared services layer (`src/services/*` or `crates/services/`) rather than within interface-specific adapters [S2][S6]. Under this design, the CLI, MCP, and Web layers act as thin presentation shells [S2][S4]. Each service function accepts typed input parameters, executes the logic, and returns a typed result struct (defined in `src/services/types/service.rs`) without producing stdout side-effects [S3][S5][S6]. The underlying contracts—including request structs, event enums for progress streaming, and error types—are defined once within the services layer and reused directly across all adapters to maintain field name stability [S2].

CLI, MCP, and Web routes share these service functions to prevent behavioral drift across different entry points, ensuring consistent execution regardless of how a command is triggered [S1][S5]. Sharing services also eliminates the significant performance overhead of the Web layer's historical subprocess proxy pattern, which previously incurred a 200–500ms tax per command by shelling out to the CLI binary [S1][S6]. Furthermore, a shared services layer enforces a strict boundary that prevents adapters from independently managing runtime execution, direct store access, or backend selection [S7]. Instead, a single service orchestrator handles the execution lifecycle, timing, metrics, and retry policies for all surfaces [S2].

Gaps: The provided sources do not specify the exact performance measurements of the direct service calls compared to the local runtime, nor do they detail any developer friction or compilation overhead associated with maintaining the shared `ServiceContext` and typed contracts.

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/reports/services-layer-refactor-scope.md#L1-L40
- [S2] https://github.com/jmagar/axon/blob/main/docs/FEATURE-DELIVERY-FRAMEWORK.md#L1-L62
- [S3] https://github.com/jmagar/axon/blob/main/GEMINI.md#L186-L212
- [S4] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-05-09-true-client-server-mode.md#L1-L31
- [S5] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L1-L70
- [S6] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-03-services-layer-design.md#L1-L42
- [S7] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-03-25-service-boundary-refactor.md#L1-L53
  Timing: retrieval=3763ms | context=1ms | llm=4616ms | total=8387ms | ttft=7766ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 59 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/mcp/TRANSPORT.md#L71-L141 ≈ https://github.com/jmagar/axon/blob/main/docs/mcp/TRANSPORT.md#L71-L142)


_Status: `0`, elapsed: `9s`_

### Q4

**Question:** How do Axon's SQLite-backed async jobs work, and what does --wait true change for crawl, extract, embed, and ingest commands?

**Answer:**

Conversation
  You: How do Axon's SQLite-backed async jobs work, and what does --wait true change for crawl, extract, embed, and ingest commands?
  Assistant:
### How Axon's SQLite-Backed Async Jobs Work

Axon’s async jobs use SQLite persistence and in-process Tokio workers to manage task execution without needing a message broker, Postgres, or Redis [S2]. The framework supports four main job families—Crawl, Extract, Embed, and Ingest—with each kind stored in its own dedicated SQLite table (such as `axon_crawl_jobs` or `axon_extract_jobs`) [S2, S16]. 

The async job lifecycle follows a state machine transitioning from `Pending` $\rightarrow$ `Running` $\rightarrow$ `Completed`, `Failed`, or `Cancelled` [S1]. Workers are spawned using `SqliteJobBackend::new_with_workers` [S2, S7]. Each worker lane runs a sequential loop, claiming pending rows from SQLite, updating a heartbeat, and executing the job [S15]. To prevent duplicate worker ownership under concurrent claims, SQLite write-locks the database during a `BEGIN IMMEDIATE` transaction, serializing claim attempts across tokio tasks [S15, S18]. 

If a process dies mid-job, a watchdog periodically sweeps the database and reclaims stale running jobs (determined by a heartbeat check against `AXON_JOB_STALE_TIMEOUT_SECS` plus a grace period) and reverts them to `pending` [S1, S11, S20, S21]. Job cancellation is handled by updating the SQLite row status to `canceled` and triggering in-memory cancellation tokens (`CancellationToken`), which workers check at safe intervals [S1, S11, S22].

---

### What `--wait true` Changes for Axon Commands

By default, Axon runs in an asynchronous, fire-and-forget mode (`--wait false`), where it writes the job payload to a SQLite table, prints a unique job ID, and exits immediately [S1, S7, S12]. 

Specifying `--wait true` changes this behavior by forcing the CLI process to block until the submitted job completes, bounded by the `AXON_JOB_WAIT_TIMEOUT_SECS` limit [S1, S17]:

*   **Runtime & Worker Initialization:** Instead of exiting, the CLI initiates a backend with workers (`new_with_workers`) to drain the queue locally, polling the job state until a terminal status is reached [S7, S12, S17]. If the CLI is configured in server mode via `AXON_SERVER_URL`, `--wait true` instead instructs the CLI to poll the remote server's job state rather than spawning host-local workers [S3, S13, S14].
*   **Crawl Command:** The crawl runs inline [S4]. Instead of queueing a separate, dependent embed job upon completing its sitemap backfill (as it does in async mode), sync mode performs the subsequent document embedding synchronously and inline [S3, S6].
*   **Extract Command:** Structured data extraction runs inline [S10]. In addition to blocking, sync mode aggregates metrics and outputs physical artifacts to the local directory, specifically generating a summary JSON file (`extract-summary.json` or a custom `--output` path) and an NDJSON items file (`extract-items.ndjson`) [S14].
*   **Embed Command:** The command blocks the process until the text, file, directory, or URL is fully embedded and upserted into Qdrant [S5, S19].
*   **Ingest Command:** Synchronously executes the source-code, Reddit, or YouTube adapters, keeping the process alive until the full ingest pipeline and subsequent embedding operations finish [S8, S9, S13].

Gaps: The context mentions a JobBackend trait that abstracts SQLite storage, but does not provide its complete code-level struct definitions or full method signatures.

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L65-L131
- [S2] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L1-L43
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/crawl.md#L55-L98
- [S4] https://github.com/jmagar/axon/blob/main/docs/commands/crawl.md#L1-L59
- [S5] https://github.com/jmagar/axon/blob/main/docs/OPERATIONS.md#L102-L161
- [S6] https://github.com/jmagar/axon/blob/main/docs/commands/crawl.md#L98-L105
- [S7] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L172-L199
- [S8] https://github.com/jmagar/axon/blob/main/docs/commands/ingest.md#L52-L94
- [S9] https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L170-L232
- [S10] https://github.com/jmagar/axon/blob/main/docs/commands/extract.md#L1-L66
- [S11] https://github.com/jmagar/axon/blob/main/docs/OPERATIONS.md#L156-L223
- [S12] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L156-L179
- [S13] https://github.com/jmagar/axon/blob/main/docs/commands/ingest.md#L91-L147
- [S14] https://github.com/jmagar/axon/blob/main/docs/commands/extract.md#L57-L92
- [S15] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L215-L257
- [S16] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L253-L299
- [S17] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L312-L327
- [S18] https://github.com/jmagar/axon/blob/main/docs/superpowers/specs/2026-03-24-lite-mode-design.md#L86-L152
- [S19] https://github.com/jmagar/axon/blob/main/docs/reports/2026-06-01-stale-docs-refresh/ground-truth/axon-embed--help.txt#L1-L28
- [S20] https://github.com/jmagar/axon/blob/main/docs/OPERATIONS.md#L344-L410
- [S21] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L224-L242
- [S22] https://github.com/jmagar/axon/blob/main/docs/superpowers/specs/2026-03-24-lite-mode-design.md#L178-L234
  Timing: retrieval=2989ms | context=1ms | llm=6656ms | total=9655ms | ttft=8150ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 109 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/stack/arch.md#L65-L130 ≈ https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L65-L131)


_Status: `0`, elapsed: `10s`_

### Q5

**Question:** What does Axon's watch scheduler do today, and which watch subcommands are implemented versus only parsed?

**Answer:**

Conversation
  You: What does Axon's watch scheduler do today, and which watch subcommands are implemented versus only parsed?
  Assistant:
### Axon's Watch Scheduler Today

Axon's watch scheduler is an in-process loop (`src/jobs/workers/watch_scheduler.rs`) spawned under `axon serve` or `axon mcp` [S1]. It functions as a URL change detector that periodically checks watched URLs [S2][S6]. During each tick, the scheduler atomically leases due enabled watches, advances their `next_run_at` timestamps, and runs them [S1]. The system is made crash-safe by utilizing a `reclaim_stale_watch_leases` sweep to clear expired leases so they can be re-acquired [S1][S10]. The scheduler's cadence and lease times are tunable via the environment variables `AXON_WATCH_TICK_SECS` (default 15 seconds) and `AXON_WATCH_LEASE_SECS` (default 300 seconds) [S1].

When a watch fires, the scheduler scrapes and compares the target URLs against stored snapshots, using a diffing service to detect content changes [S5][S6][S8]. If a change is deemed "meaningful" (e.g., meeting word-count or link change thresholds), the scheduler generates an AI summary of the diff using an LLM, records a `url-change` run artifact, and enqueues a depth-bounded crawl job for the changed subtrees [S6][S7][S9].

---

### Watch Subcommands Implementation Status

The subcommands under the `axon watch` CLI are split between those that are fully implemented and those that are defined in the schema (parsed) but not yet functional [S2][S4]:

**Implemented Subcommands:**
*   `create`: Creates a new watch definition [S2][S3]. It validates the `task_type` (which must be `watch`) [S9], the interval bounds (30 to 604,800 seconds) [S11], and the regex structure of any `ignore_patterns` inside the JSON payload [S2][S7].
*   `list`: Lists active watch definitions [S2][S3].
*   `run-now`: Dispatches an immediate run for a specified watch definition ID [S2][S3].
*   `history`: Displays the run history for a watch definition [S2][S3].

**Parsed Only (Return "not yet implemented" errors):**
*   `get` [S2][S3]
*   `update` [S2][S3]
*   `pause` [S2][S3]
*   `resume` [S2][S3]
*   `delete` [S2][S3]
*   `artifacts` [S2][S3]

## Sources
- [S1] https://github.com/jmagar/axon/pull/149
- [S2] https://github.com/jmagar/axon/blob/main/docs/reference/commands/watch.md#L1-L63
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/watch.md#L1-L77
- [S4] https://github.com/jmagar/axon/blob/main/docs/reports/2026-06-01-stale-docs-refresh/agent-reports/E-ingest.md#L37-L48
- [S5] https://github.com/jmagar/axon/blob/main/docs/superpowers/specs/2026-05-31-url-watch-change-detection-design.md#L36-L59
- [S6] https://github.com/jmagar/axon/blob/main/docs/superpowers/specs/2026-05-31-url-watch-change-detection-design.md#L1-L40
- [S7] https://github.com/jmagar/axon/blob/main/docs/superpowers/specs/2026-05-31-url-watch-change-detection-design.md#L133-L182
- [S8] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-05-31-url-watch-change-detection.md#L1-L19
- [S9] https://github.com/jmagar/axon/blob/main/docs/reference/commands/watch.md#L57-L108
- [S10] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L187-L219
- [S11] https://github.com/jmagar/axon/blob/main/docs/reports/2026-06-01-stale-docs-refresh/agent-reports/E-ingest.md#L22-L37
  Timing: retrieval=3732ms | context=1ms | llm=5005ms | total=8747ms | ttft=7264ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 32 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/reference/job-lifecycle.md#L187-L219 ≈ https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L187-L219)


_Status: `0`, elapsed: `8s`_

### Q6

**Question:** How does Axon choose between SearXNG and Tavily for search or research, and what happens to discovered results?

**Answer:**

Conversation
  You: How does Axon choose between SearXNG and Tavily for search or research, and what happens to discovered results?
  Assistant:
Axon determines which search backend to use by checking the `AXON_SEARXNG_URL` environment variable [S1][S2][S4]. If `AXON_SEARXNG_URL` is set, Axon queries the self-hosted SearXNG JSON API [S1][S5]. If this variable is unset, Axon falls back to using Tavily, which requires a configured `TAVILY_API_KEY` [S1][S2][S4][S5].

When you perform a standard `search`, Axon returns the ranked results (consisting of the title, URL, and snippet) and automatically enqueues one bounded crawl job per result URL [S3]. These crawl jobs are processed asynchronously by the in-process worker pool to index the pages into Qdrant [S3]. 

When running a `research` command, the pipeline fetches the full-page markdown content of the top sources concurrently—truncating the content to fit the LLM's context budget—and falls back to search snippets only if a page fetch fails or if `AXON_RESEARCH_FULL_CONTENT` is set to `false` [S1][S4][S5]. A configured LLM then synthesizes these source materials into a coherent summary [S4]. Finally, the `research` command enqueues bounded crawl jobs for the discovered result URLs so they are indexed asynchronously into Qdrant, though you can bypass the indexing stage by passing the `--skip-embed` flag [S2][S4].

## Sources
- [S1] https://github.com/jmagar/axon/pull/158
- [S2] https://github.com/jmagar/axon/blob/main/docs/reference/commands/research.md#L1-L46
- [S3] https://github.com/jmagar/axon/blob/main/docs/reference/commands/search.md#L1-L66
- [S4] https://github.com/jmagar/axon/blob/main/docs/reference/commands/research.md#L44-L80
- [S5] https://github.com/jmagar/axon/blob/main/CHANGELOG.md#L46-L58
  Timing: retrieval=7565ms | context=1ms | llm=4408ms | total=11984ms | ttft=11335ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 34 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L1-L67 ≈ https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L1-L65)


_Status: `0`, elapsed: `12s`_

### Q7

**Question:** What are Axon's LLM backend options, and how should AXON_OPENAI_BASE_URL be formatted for openai-compat?

**Answer:**

Conversation
  You: What are Axon's LLM backend options, and how should AXON_OPENAI_BASE_URL be formatted for openai-compat?
  Assistant:
Axon's LLM backend options are configured via the `AXON_LLM_BACKEND` environment variable, which supports two main options: `gemini-headless` and `openai-compat` [S2][S3][S4]. The default backend is `gemini-headless`, which uses the Gemini CLI for headless synthesis [S2][S4]. The `openai-compat` backend allows Axon to use llama.cpp or other OpenAI-compatible `/v1/chat/completions` servers [S1][S3][S4].

For the `openai-compat` backend, the `AXON_OPENAI_BASE_URL` environment variable must be formatted as the OpenAI-compatible API root, such as `http://127.0.0.1:8080/v1` [S1][S3]. It must not include `/chat/completions` because Axon appends that path automatically [S1][S2][S3]. However, `/v1` should be included at the end of the base URL if the endpoint serves `/v1/chat/completions` [S2].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/guides/configuration.md#L190-L212
- [S2] https://github.com/jmagar/axon/blob/main/GEMINI.md#L263-L317
- [S3] https://github.com/jmagar/axon/blob/main/docs/guides/configuration.md#L158-L191
- [S4] https://github.com/jmagar/axon/blob/main/docs/reference/mcp/patterns.md#L173-L193
  Timing: retrieval=9304ms | context=1ms | llm=3664ms | total=12977ms | ttft=12637ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 33 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/env-migration-matrix.md#L228-L270 ≈ https://github.com/jmagar/axon/blob/main/docs/env-migration-matrix.md#L229-L272)


_Status: `0`, elapsed: `13s`_

### Q8

**Question:** What are the important Spider feature-flag gotchas in Axon, especially firewall, balance, and glob?

**Answer:**

Conversation
  You: What are the important Spider feature-flag gotchas in Axon, especially firewall, balance, and glob?
  Assistant:
In Axon, the `firewall`, `balance`, and `glob` Spider feature flags are intentionally disabled or excluded due to specific integration gotchas:

*   **`firewall`**: This flag is disabled because the `spider_firewall` crate's `build.rs` file attempts to fetch blocklists from `api.github.com` unauthenticated, which triggers panics when GitHub rate-limits CI runners [S1][S4]. Since the build script cannot read `GITHUB_TOKEN` for external authentication, the feature is omitted [S1][S4]. Axon instead relies on its own `validate_url()` logic in `src/core/http/ssrf.rs` as the primary defense-in-depth SSRF guard [S1][S4][S6].
*   **`balance`**: This flag is disabled because it silently throttles concurrency with no logging output [S2][S4]. Axon bypasses this by managing concurrency configurations explicitly through its own internal performance profiles [S2][S4][S5].
*   **`glob`**: The native `glob` feature was removed because it changes `crawl_establish` to utilize a budget-aware `is_allowed()` check instead of `is_allowed_default()` [S4]. When using `with_limit(1)`, this budget check immediately returns a `BudgetExceeded` error on the very first URL, producing zero pages during Chrome crawls [S2][S4]. To avoid this, Axon handles `--url-glob` input expansion programmatically on the client side before initializing the Spider `Website` object, rather than delegating it to Spider [S3][S4].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/reference/spider-feature-flags.md#L1-L46
- [S2] https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L56-L94
- [S3] https://github.com/jmagar/axon/blob/main/docs/reports/spider-alignment-core-api.md#L369-L438
- [S4] https://github.com/jmagar/axon/blob/main/GEMINI.md#L409-L432
- [S5] https://github.com/jmagar/axon/blob/main/docs/stack/TECH.md#L97-L132
- [S6] https://github.com/jmagar/axon/blob/main/CHANGELOG.md#L19-L40
  Timing: retrieval=5477ms | context=1ms | llm=4053ms | total=9539ms | ttft=8890ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 36 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/reference/spider-feature-flags.md#L77-L114 ≈ https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L56-L94)


_Status: `0`, elapsed: `10s`_

### Q9

**Question:** How does Axon's hybrid vector search work with Qdrant dense and BM42 sparse vectors, and when does it fall back to dense-only?

**Answer:**

Conversation
  You: How does Axon's hybrid vector search work with Qdrant dense and BM42 sparse vectors, and when does it fall back to dense-only?
  Assistant:
Axon’s hybrid vector search combines dense semantic embeddings with BM42 sparse vectors to capture both contextual meaning and precise keyword matches [S6, S10]. For new collections configured in "Named" mode, dense vectors are generated using a Text Embeddings Inference (TEI) model [S1, S6]. Concurrently, BM42 sparse vectors are computed locally by tokenizing target text into lowercase alphanumeric terms of three or more characters (filtering out stopwords) [S2, S3]. These terms are mapped into 65,536 buckets using FNV-1a hashing to yield raw Term Frequency (TF) weights [S3, S10]. When these points are upserted, Qdrant applies Inverse Document Frequency (IDF) correction on the server side using the `"modifier": "idf"` configuration [S1, S2].

During a query, Axon dispatches a request to Qdrant's `/points/query` endpoint with two prefetch arms [S4, S7, S8]. One arm retrieves dense vector candidates via HNSW search, and the other retrieves sparse vector candidates using the BM42 index [S6]. The candidate window size for these prefetches is controlled by configurations: the default query prefetch limit is set to 100 (`AXON_HYBRID_CANDIDATES`), while the ask pipeline defaults to 150 (`AXON_ASK_HYBRID_CANDIDATES`) [S6, S10]. Qdrant fuses and re-ranks the retrieved candidate pools using Reciprocal Rank Fusion (RRF) with a fusion constant of `k=60` [S2, S6].

Axon falls back to dense-only retrieval under the following conditions:
* **Legacy "Unnamed" Collections:** If the collection is detected to be in legacy unnamed mode (having only a single unnamed dense vector schema), Axon skips sparse vector computation and issues standard cosine queries to the `/points/search` endpoint [S1, S7, S10].
* **Disabled via Environment Configuration:** When the environment variable `AXON_HYBRID_SEARCH` is set to `false` [S5, S6].
* **Command-line Flag Override:** When a query or ask operation is executed with the explicit `--no-hybrid-search` flag (e.g., `axon query --no-hybrid-search` or `axon ask --no-hybrid-search`) [S9, S11].

Gaps: The exact implementation details of Qdrant's server-side IDF calculation algorithm and the complete code structure of the FNV-1a hashing function are not detailed in the provided sources.

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L1-L27
- [S2] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L24-L58
- [S3] https://github.com/jmagar/axon/pull/47
- [S4] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L1178-L1236
- [S5] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L1611-L1646
- [S6] https://github.com/jmagar/axon/blob/main/docs/stack/TECH.md#L46-L101
- [S7] https://github.com/jmagar/axon/blob/main/docs/reference/qdrant-payload-schema.md#L218-L255
- [S8] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-16-temporal-search.md#L620-L703
- [S9] https://github.com/jmagar/axon/blob/main/docs/reference/commands/query.md#L1-L63
- [S10] https://github.com/jmagar/axon/blob/main/src/vector/CLAUDE.md#L89-L114
- [S11] https://github.com/jmagar/axon/blob/main/docs/reference/commands/ask.md#L1-L43
  Timing: retrieval=6371ms | context=2ms | llm=5937ms | total=12317ms | ttft=11047ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 43 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/stack/tech.md#L46-L99 ≈ https://github.com/jmagar/axon/blob/main/docs/stack/TECH.md#L46-L101)


_Status: `0`, elapsed: `12s`_

### Q10

**Question:** What configuration and runtime safeguards prevent Axon ask from overloading smaller-context local models?

**Answer:**

Conversation
  You: What configuration and runtime safeguards prevent Axon ask from overloading smaller-context local models?
  Assistant:
Axon prevents overloading smaller-context local models through a combination of model-aware configuration scaling and runtime context assembly constraints.

### Model-Tier Configuration Scaling
Axon implements model-tier scaling to automatically adjust retrieval and context depth based on the LLM's capability tier [S1]. When a "Small" model tier is configured (representing typical smaller-context local models), Axon scales down its defaults to the following protective limits [S1]:
*   **Max Context Characters (`ask_max_context_chars`)**: 40,000 characters (compared to 1,000,000 for Large models) [S1].
*   **Chunk Limit (`ask_chunk_limit`)**: 10 chunks (compared to 50 for Large models) [S1].
*   **Candidate Limit (`ask_candidate_limit`)**: 60 candidates [S1].
*   **Hybrid Candidates (`ask_hybrid_candidates`)**: 60 candidates [S1].

These settings can still be manually overridden using environment variables or `config.toml` settings (such as `AXON_ASK_MAX_CONTEXT_CHARS` and `ask.chunk-limit`) [S1][S2][S4].

### Runtime Safeguards
During context assembly, Axon enforces several safeguards to ensure the payload does not exceed the model's limits:
*   **Hard Context Cap**: Axon enforces `AXON_ASK_MAX_CONTEXT_CHARS` as a strict hard cap on the final assembled context length passed to the LLM [S4].
*   **Full-Doc Fallback Policy**: While full documents are prioritized during context insertion, Axon uses an all-or-nothing check: if a planned full document's size exceeds the `ask_max_context_chars` budget, the pipeline falls back to chunk coverage for that document rather than overloading the context window [S1].
*   **Document Chunk Limits**: The number of chunks extracted per document is limited by `ask_doc_chunk_limit` (configured to default to 48 chunks to control size) [S3].
*   **Adaptive Skip Gate**: Axon enables an adaptive skip gate (`ask_fulldoc_skip_enabled`) to bypass unnecessary full-document fetches, protecting system performance and context slot consumption [S3].

## Sources
- [S1] https://github.com/jmagar/axon/pull/157
- [S2] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L263-L287
- [S3] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-05-12-ask-quality-skill-prompt.md#L1-L25
- [S4] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L225-L258
  Timing: retrieval=4518ms | context=1ms | llm=3913ms | total=8441ms | ttft=7707ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 32 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/PERFORMANCE.md#L88-L155 ≈ https://github.com/jmagar/axon/blob/main/docs/PERFORMANCE.md#L88-L158)


_Status: `0`, elapsed: `9s`_

