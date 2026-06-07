# Axon Ask Results - Gemma 4 E4B Q4 via local llama.cpp

- Date: 2026-06-07
- Binary: `axon 5.1.2`
- Env file: `/home/jmagar/workspace/axon/reports/llm-ask-comparison-2026-06-07/env/gemma-e4b-q4.env`
- llama.cpp health: `{"status":"ok"}`

## Effective Model Config

```text
  AXON_ASK_BACKFILL_CHUNKS = 1
  AXON_ASK_DOC_CHUNK_LIMIT = 24
  AXON_ASK_DOC_FETCH_CONCURRENCY = 1
  AXON_ASK_FULL_DOCS = 1
  AXON_LLM_BACKEND = openai-compat
  AXON_OPENAI_BASE_URL = http://127.0.0.1:8080/v1
  AXON_OPENAI_MODEL = ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M
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

## Gemma Context Smoke

```json
{
  "ask_max_context_chars": 300000,
  "ask_chunk_limit": 20,
  "ask_candidate_limit": 120,
  "ask_hybrid_candidates": 100,
  "context_chars": 47983,
  "full_docs_selected": 1,
  "chunks_selected": 20
}
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
The high-level architecture of Axon centers around a unified service layer that acts as the API boundary between all consumers (CLI, MCP, web) and the underlying infrastructure [S9]. This service layer allows various entry points—CLI handlers, MCP handlers, and web routes—to consistently access functionality like querying or asking questions [S8].

The system's operational components map to several subsystems, which are increasingly being formalized into independent crates within a Cargo workspace structure [S1].

Specific subsystems and their roles include:

*   **Crawl:** Handles the crawl engine and sitemap backfill, managing crawl-related jobs [S4], [S5].
*   **Jobs:** Provides an SQLite-backed job framework (`src/jobs/`) for managing asynchronous work across various tasks, such as crawl, extract, embed, and ingest jobs [S5], [S9].
*   **Vector Search:** Manages Qdrant operations, TEI embedding, and hybrid search functionality within the `axon-vector` subsystem [S4], [S5].
*   **MCP (Machine Control Protocol):** Implements the MCP server schema and handlers (`src/mcp/`), allowing external tools to interact with Axon's operations using a single `axon` tool via `action` and `subaction` routing [S2], [S3], [S6], [S8].
*   **Web Serving:** Handles the unified HTTP runtime, including the web panel and REST API routes, such as `/v1/ask` and `/v1/query` [S5], [S7].

For deeper technical context, the architecture is being refactored into an explicit Cargo workspace structure, separating concerns into crates like `axon-core`, `axon-crawl`, `axon-jobs`, `axon-mcp`, and `axon-web` [S1].

## Sources
- [S1] https://github.com/jmagar/axon/issues/18
- [S2] https://github.com/jmagar/axon/blob/main/docs/INVENTORY.md#L1-L53
- [S3] https://github.com/jmagar/axon/blob/main/docs/reference/inventory.md#L1-L45
- [S4] https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L1-L68
- [S5] https://github.com/jmagar/axon/blob/main/docs/INVENTORY.md#L89-L137
- [S6] https://github.com/jmagar/axon/blob/main/docs/mcp/DEV.md#L1-L45
- [S7] https://github.com/jmagar/axon/blob/main/docs/reference/inventory.md#L151-L187
- [S8] https://github.com/jmagar/axon/blob/main/docs/mcp/PATTERNS.md#L1-L66
- [S9] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L1-L70
  Timing: retrieval=145ms | context=2ms | llm=8801ms | total=8954ms | ttft=4153ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 27 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/overview.md#L1-L65 ≈ https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L1-L68)


_Status: `0`, elapsed: `9s`_

### Q2

**Question:** How does Axon's ask pipeline retrieve context and synthesize an answer from indexed documents?

**Answer:**

Conversation
  You: How does Axon's ask pipeline retrieve context and synthesize an answer from indexed documents?
  Assistant:
The `axon ask` pipeline retrieves context and synthesizes an answer through a multi-stage process involving embedding, retrieval from Qdrant, context assembly, and final LLM synthesis [S2].

### Context Retrieval Pipeline
The process follows five key stages before the LLM call:

1.  **Embed the Query:** The user's query text is converted into one or two dense vectors using TEI, which also prepends a `QUERY_INSTRUCTION` prefix when embedding the full user query [S2]. If keyword extraction identifies distinct keywords, those are also embedded as separate vectors to improve recall for specific terms or identifiers [S2].
2.  **Retrieve Candidates from Qdrant:** The system performs an ANN search against Qdrant using the generated vectors, which returns up to $N$ candidate chunks [S2].
3.  **Filter and Rerank:** A filtering step narrows the initial pool using low-signal and allowlist guards [S2]. After filtering, the chunks are re-ordered by a combined score from lexical and domain boosts [S2].
4.  **Build Context:** The assembled context string is built into three potential tiers, in order:
    *   **Tier 1 — Top Chunks:** Selects up to `ask.chunk-limit` chunks, ensuring diversity by limiting selections to at most one chunk per unique URL [S5].
    *   **Tier 2 — Full Documents:** For up to `ask.full-docs` URLs, all stored chunks are fetched from Qdrant. This stage only runs if the running context size is below the maximum context character limit [S5].
    *   **Tier 3 — Supplemental Chunks (Backfill):** These remaining chunks are selected only if the context is under 85% of the maximum context size, and either no full documents were selected or fewer than 6 top chunks were selected [S5].
    The final context size is capped by `AXON_ASK_MAX_CONTEXT_CHARS` (default: 300,000 characters) [S5, S7].
5.  **Synthesis:** The assembled `context` string, along with the original question, is passed to the LLM via a structured message format containing the `system` role, the `user` role (containing both the query and the context), and then the final answer generation occurs [S1, S2].

### Follow-Up and Session Management
For multi-turn conversations, `axon ask` supports a `--follow-up` flag, which enables the use of previous Q&A turns as conversation history [S3, S4]. In this scenario, the system stores session state in Redis to maintain context, sending a multi-turn messages array to the LLM, which includes the system prompt, the initial query/context, the previous assistant answer, and the new follow-up question [S1].

### Key Controls
The entire retrieval depth is controlled by configuration settings such as `ask.candidate-limit` (default: 250), `ask.chunk-limit` (default: 20), and `ask.full-docs` (default: 6) [S5, S7]. Furthermore, the `--explain` flag allows users to run the retrieval, reranking, and context assembly process without calling the LLM, showing the ranking and context decisions for debugging [S6].

## Sources
- [S1] example.com/guide/crawl
- [S2] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L1-L60
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L44-L89
- [S4] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L68-L115
- [S5] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L141-L176
- [S6] https://github.com/jmagar/axon/blob/main/docs/ASK.md#L1-L47
- [S7] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L212-L228
  Timing: retrieval=128ms | context=1ms | llm=11672ms | total=11807ms | ttft=2907ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 24 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/guides/context-injection.md#L1-L60 ≈ https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L1-L60)


_Status: `0`, elapsed: `12s`_

### Q3

**Question:** What is the services-first contract in Axon, and why should CLI, MCP, and web routes share service functions?

**Answer:**

Conversation
  You: What is the services-first contract in Axon, and why should CLI, MCP, and web routes share service functions?
  Assistant:
The services-first contract in Axon mandates that all new feature logic resides within `src/services/*`, making this module the authoritative source for business logic [S2]. Under this architecture, the CLI, MCP, and Web layers are designed to be thin adapters that consume these shared service functions, rather than housing the logic themselves [S2, S4, S5, S6].

The primary reasons for having CLI, MCP, and Web routes share service functions include:

*   **Preventing Behavioral Drift:** Sharing the service layer ensures that the behavior across different entry points remains consistent, thus preventing discrepancies between the CLI and MCP, for example [S1].
*   **Improving Architecture:** By centralizing logic, the system eliminates the Web layer's current pattern of shelling out to the CLI binary as a subprocess, which introduces significant overhead (200-500ms per command) [S1].
*   **Enforcing Consistency:** This pattern enforces that all three entry points map inputs to shared service request/option types and map service outputs to their specific transport contracts—be it text/JSON for CLI, artifact wrappers for MCP, or WebSocket events for Web—thereby achieving "Full unification" [S5, S6].

When implemented, the process involves mapping inputs from CLI/MCP/Web to the shared service functions, which execute the business logic and return typed result structs, which are then formatted for the respective transport mechanism [S3].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/reports/services-layer-refactor-scope.md#L1-L40
- [S2] https://github.com/jmagar/axon/blob/main/docs/FEATURE-DELIVERY-FRAMEWORK.md#L1-L62
- [S3] https://github.com/jmagar/axon/blob/main/CLAUDE.md#L186-L212
- [S4] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L1-L70
- [S5] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-03-services-layer-design.md#L1-L42
- [S6] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-03-services-layer-plan.md#L1-L48
  Timing: retrieval=169ms | context=1ms | llm=5995ms | total=6171ms | ttft=2470ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 37 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/mcp/TRANSPORT.md#L71-L141 ≈ https://github.com/jmagar/axon/blob/main/docs/mcp/TRANSPORT.md#L71-L142)


_Status: `0`, elapsed: `6s`_

### Q4

**Question:** How do Axon's SQLite-backed async jobs work, and what does --wait true change for crawl, extract, embed, and ingest commands?

**Answer:**

Conversation
  You: How do Axon's SQLite-backed async jobs work, and what does --wait true change for crawl, extract, embed, and ingest commands?
  Assistant:
Axon's asynchronous jobs utilize SQLite for persistence, managing job states that flow through cycles such as Submitted $\to$ Pending $\to$ Running $\to$ Completed, or failing/canceling [S1]. There is no message broker, Postgres, or Redis used for the job backend, which instead relies on SQLite-backed job management [S3].

### How Axon's SQLite-backed Async Jobs Work
The core mechanism involves:
*   **Persistence and Runtime:** Jobs are persisted in SQLite tables, with a dedicated SQLite job backend that can spawn in-process workers for long-running processes like `axon serve` or when using `--wait true` [S5], [S3].
*   **Job Kinds:** There are four primary families of async jobs, each corresponding to a specific SQLite table: `Crawl`, `Extract`, `Embed`, and `Ingest` [S3], [S2].
*   **Workflow:** When a job is submitted, it is enqueued to the respective SQLite table [S6]. Workers then claim pending rows from this database table, update its heartbeat status while processing, and record completion or failure [S6].
*   **Job Lifecycle Management:** The system provides standard operational commands for managing these jobs, including:
    *   `status <job_id>`: Reads the job's current state [S9].
    *   `cancel <job_id>`: Flips the job status to `canceled` [S7].
    *   `errors <job_id>`: Reads error details associated with the job [S9].
    *   `list`: Shows a summary of recent jobs [S9].
    *   `cleanup`: Deletes terminal jobs (like completed or failed ones) that are older than a set threshold (e.g., 24 hours) [S9].
    *   `clear`: Destructively removes *all* records from the job table [S9].
    *   `recover`: Reclaims jobs that were in a `running` state when the process died, re-queuing them as `pending` [S7], [S9].

### Effect of `--wait true` vs. `--wait false`
The `--wait` flag controls whether the command blocks execution until the job finishes or exits immediately, triggering asynchronous behavior:

*   **`--wait false` (Default/Fire-and-Forget):** When using `crawl`, `embed`, or `ingest` submits with this setting, the command simply enqueues the job into the SQLite table, prints the job ID, and exits without waiting for the job to complete or draining the table [S5], [S4], [S8].
*   **`--wait true` (Synchronous/Blocking):** Setting this flag initiates an in-process worker where the service path requires queued workers. The command will then block and wait for the submitted job (and any explicit dependent jobs) to complete before returning control to the user [S5], [S4].

This synchronous execution path is necessary for the command to poll the server's job state and wait for the outcome, whereas the default asynchronous mode leaves the job for a separate `axon serve` or `axon mcp` process to handle [S5], [S8].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L65-L131
- [S2] https://github.com/jmagar/axon/blob/main/docs/INVENTORY.md#L89-L137
- [S3] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L1-L43
- [S4] https://github.com/jmagar/axon/blob/main/docs/commands/crawl.md#L55-L98
- [S5] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L172-L199
- [S6] https://github.com/jmagar/axon/blob/main/docs/ARCHITECTURE.md#L170-L232
- [S7] https://github.com/jmagar/axon/blob/main/docs/OPERATIONS.md#L156-L223
- [S8] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L156-L179
- [S9] https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L285-L314
  Timing: retrieval=133ms | context=3ms | llm=10827ms | total=10969ms | ttft=2569ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 60 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/stack/arch.md#L65-L130 ≈ https://github.com/jmagar/axon/blob/main/docs/stack/ARCH.md#L65-L131)


_Status: `0`, elapsed: `12s`_

### Q5

**Question:** What does Axon's watch scheduler do today, and which watch subcommands are implemented versus only parsed?

**Answer:**

Conversation
  You: What does Axon's watch scheduler do today, and which watch subcommands are implemented versus only parsed?
  Assistant:
Axon's watch scheduler now features an **auto-firing** mechanism for recurring watches [S1], [S2]. This functionality was introduced via a new in-process loop located in `src/jobs/workers/watch_scheduler.rs`, which is active when running `axon serve` or `axon mcp` [S1], [S2]. Each tick of this loop atomically leases due, enabled watches using `lease_due_watches` (an `UPDATE ... RETURNING` operation), executes them via `run_watch_now_with_pool`, and advances the watch's `next_run_at` timestamp while clearing the lease [S1].

Regarding watch subcommands, the documentation indicates that the following are **implemented** and functional:
*   `axon watch create <name> --task-type <type> --every-seconds <n> [--task-payload <json>]` [S2], [S3]
*   `axon watch list` [S2], [S3]
*   `axon watch run-now <id>` [S2], [S3]
*   `axon watch history <id> [--limit <n>]` [S2], [S3]
*   `axon watch` with no subcommand defaults to `list` [S2], [S3].

Conversely, the following subcommands are defined in the CLI schema but are noted to return "not yet implemented" errors [S2], [S3]:
*   `axon watch get <id>`
*   `axon watch update <id> [--every-seconds <n>]`
*   `axon watch pause <id>`
*   `axon watch resume <id>`
*   `axon watch delete <id>`
*   `axon watch artifacts <run_id> [--limit <n>]`

Additionally, the API documentation confirms that while HTTP exposes actions for `create`, `list`, and `run-now`, other parsed CLI subcommands remain unimplemented [S4], [S5].

## Sources
- [S1] https://github.com/jmagar/axon/pull/149
- [S2] https://github.com/jmagar/axon/blob/main/docs/reference/commands/watch.md#L1-L63
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/watch.md#L1-L77
- [S4] https://github.com/jmagar/axon/blob/main/docs/reference/api-parity.md#L57-L71
- [S5] https://github.com/jmagar/axon/blob/main/docs/API-PARITY.md#L47-L59
  Timing: retrieval=217ms | context=1ms | llm=7577ms | total=7800ms | ttft=2682ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 19 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/reference/job-lifecycle.md#L187-L219 ≈ https://github.com/jmagar/axon/blob/main/docs/JOB-LIFECYCLE.md#L187-L219)


_Status: `0`, elapsed: `7s`_

### Q6

**Question:** How does Axon choose between SearXNG and Tavily for search or research, and what happens to discovered results?

**Answer:**

Conversation
  You: How does Axon choose between SearXNG and Tavily for search or research, and what happens to discovered results?
  Assistant:
Axon's method for choosing between SearXNG and Tavily for search or research depends on the configuration of the `AXON_SEARXNG_URL` environment variable [S3].

1.  **Selection Logic:**
    *   The system will use the **SearXNG backend** if the `AXON_SEARXNG_URL` environment variable is set [S1].
    *   If `AXON_SEARXNG_URL` is not configured, the process defaults to using **Tavily** [S1], which acts as a fallback mechanism [S3].

2.  **How Discovered Results Are Used (Synthesis Depth):**
    *   **With SearXNG:** The `research` command now synthesizes answers over the **full pages** of the top sources fetched via an HTTP render, instead of relying only on snippets [S1]. This full-content synthesis provides a more comprehensive answer.
    *   **With Tavily (or if full content is bypassed):** When using Tavily, the initial behavior was to synthesize over snippets, which resulted in shallower answers [S1]. However, the general research pipeline fetches ranked results and uses the snippets from those results to synthesize a summary [S2], [S5].
    *   **Controlling Depth:** The `AXON_RESEARCH_FULL_CONTENT` environment variable controls the synthesis source; setting it to `false` limits the research to synthesizing over search snippets only, which is faster [S3].

When running the `research` command, the process involves the search backend (SearXNG or Tavily) fetching results, followed by the LLM synthesizing the evidence gathered from those results [S1], [S4].

## Sources
- [S1] https://github.com/jmagar/axon/pull/158
- [S2] https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L1-L65
- [S3] https://github.com/jmagar/axon/blob/main/docs/guides/configuration.md#L226-L257
- [S4] https://github.com/jmagar/axon/blob/main/docs/reference/commands/research.md#L44-L80
- [S5] https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L61-L73
  Timing: retrieval=109ms | context=1ms | llm=6557ms | total=6673ms | ttft=2302ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 19 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L1-L67 ≈ https://github.com/jmagar/axon/blob/main/docs/commands/research.md#L1-L65)


_Status: `0`, elapsed: `7s`_

### Q7

**Question:** What are Axon's LLM backend options, and how should AXON_OPENAI_BASE_URL be formatted for openai-compat?

**Answer:**

Conversation
  You: What are Axon's LLM backend options, and how should AXON_OPENAI_BASE_URL be formatted for openai-compat?
  Assistant:
Axon has distinct backend options for LLM integration, which are managed via environment variables and configuration.

**LLM Backend Options:**

*   **ACP agents:** This path uses a subprocess-based bridge for full agent capabilities, which is locked to specific CLI tools: Claude Code, Codex, and Gemini CLI [S1].
*   **OpenAI-compatible HTTP:** This method utilizes raw `POST /chat/completions` requests and is exclusively used by `axon ask`, `axon extract`, and `axon suggest` when configured for this backend [S1].
*   **Gemini CLI:** Gemini headless is the default LLM synthesis path, which uses the Gemini CLI [S8], [S9].

**OpenAI-Compatible Endpoint Details:**

When using the OpenAI-compatible backend, the following environment variables/configuration are used for compatibility settings [S1], [S2], [S3], [S4], [S5], [S6], [S7], [S8]:

*   **`AXON_LLM_BACKEND`**: Setting this to `openai-compat` selects this backend [S6], [S7].
*   **`AXON_OPENAI_BASE_URL`**: This specifies the OpenAI-compatible API root, such as `http://127.0.0.1:8080/v1` [S2], [S3], [S6], [S7]. The context notes that Axon appends `/chat/completions` itself, so this value should **not** include it [S2], [S3].
*   **`AXON_OPENAI_MODEL`**: This is the model name sent to the compatible endpoint [S2], [S3].
*   **`AXON_OPENAI_API_KEY`**: This provides an optional bearer token for authentication [S2], [S3].

**Formatting `AXON_OPENAI_BASE_URL` for OpenAI-Compat:**

For OpenAI-compatible endpoints, `AXON_OPENAI_BASE_URL` must be formatted as the API root, and the context explicitly advises **not** to include `/chat/completions` in this value, as the Axon application handles appending it [S2], [S3].

Gaps: The sources detail the configuration variables for the OpenAI-compatible backend but do not provide a definitive, single, up-to-date example of a fully functional, modern `axon.toml` configuration that incorporates all listed endpoints (Ollama, OpenRouter, Groq, etc.) simultaneously.

## Sources
- [S1] https://github.com/jmagar/axon/issues/37
- [S2] https://github.com/jmagar/axon/blob/main/docs/guides/configuration.md#L190-L212
- [S3] https://github.com/jmagar/axon/blob/main/docs/operations/deployment.md#L95-L137
- [S4] https://github.com/jmagar/axon/blob/main/docs/reference/mcp/env.md#L21-L53
- [S5] https://github.com/jmagar/axon/blob/main/docs/reference/commands/research.md#L1-L46
- [S6] https://github.com/jmagar/axon/blob/main/CLAUDE.md#L263-L317
- [S7] https://github.com/jmagar/axon/blob/main/docs/guides/configuration.md#L158-L191
- [S8] https://github.com/jmagar/axon/blob/main/docs/reference/mcp/patterns.md#L173-L193
- [S9] https://github.com/jmagar/axon/blob/main/docs/MCP.md#L1-L54
  Timing: retrieval=123ms | context=2ms | llm=9797ms | total=9927ms | ttft=2998ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 16 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/env-migration-matrix.md#L228-L270 ≈ https://github.com/jmagar/axon/blob/main/docs/env-migration-matrix.md#L229-L272)


_Status: `0`, elapsed: `10s`_

### Q8

**Question:** What are the important Spider feature-flag gotchas in Axon, especially firewall, balance, and glob?

**Answer:**

Conversation
  You: What are the important Spider feature-flag gotchas in Axon, especially firewall, balance, and glob?
  Assistant:
The important Spider feature-flag gotchas regarding `firewall`, `balance`, and `glob` are detailed in the context as follows:

*   **`firewall`**:
    *   The `firewall` feature is explicitly **NOT enabled** in the current build, as its build script panics when rate-limited by `api.github.com` when fetching blocklists unauthenticated.
    *   Because it is disabled, the primary SSRF guard remains `validate_url()` within `src/core/http/ssrf.rs` [S1].
    *   This feature was listed as being disabled in multiple documentation updates [S3], [S6], [S8].

*   **`balance`**:
    *   The `balance` feature is **NOT enabled** because it causes silent concurrency throttling with no associated logging [S2], [S4], [S5].
    *   Instead, concurrency management is handled manually by the developers through performance profiles [S2].

*   **`glob`**:
    *   The `glob` feature is **NOT enabled** for specific reasons [S2], [S6], [S7].
    *   When `glob` is used with `with_limit(1)`, the budget check immediately returns `BudgetExceeded` for the *first* URL it encounters, resulting in zero pages being crawled from Chrome crawls.
    *   The documentation strongly advises against re-enabling this feature [S2].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/reference/spider-feature-flags.md#L1-L46
- [S2] https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L56-L94
- [S3] https://github.com/jmagar/axon/blob/main/docs/reference/spider-feature-flags.md#L209-L242
- [S4] https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L91-L137
- [S5] https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L41-L61
- [S6] https://github.com/jmagar/axon/blob/main/GEMINI.md#L409-L432
- [S7] https://github.com/jmagar/axon/blob/main/docs/reports/2026-05-06-stale-docs-audit/A-root-foundational.md#L185-L210
- [S8] https://github.com/jmagar/axon/blob/main/docs/reports/2026-06-01-stale-docs-refresh/agent-reports/H-security-api-testing-repo.md#L56-L92
  Timing: retrieval=136ms | context=2ms | llm=6970ms | total=7117ms | ttft=3067ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 24 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/reference/spider-feature-flags.md#L77-L114 ≈ https://github.com/jmagar/axon/blob/main/docs/SPIDER-FEATURE-FLAGS.md#L56-L94)


_Status: `0`, elapsed: `8s`_

### Q9

**Question:** How does Axon's hybrid vector search work with Qdrant dense and BM42 sparse vectors, and when does it fall back to dense-only?

**Answer:**

Conversation
  You: How does Axon's hybrid vector search work with Qdrant dense and BM42 sparse vectors, and when does it fall back to dense-only?
  Assistant:
Axon implements a hybrid vector search capability by combining results from two vector types within Qdrant: **dense embeddings** and **BM42 sparse vectors** [S3]. This process is designed to enhance keyword-precise retrieval that standard semantic search might miss [S3].

Here is a detailed breakdown of how the hybrid search works and the fallback mechanism:

### Hybrid Search Implementation (Named Mode)
When a collection is configured for hybrid search (referred to as `Named` mode), the system performs the following steps:

1.  **Vector Combination:** The collection must store both a `dense` vector (containing the TEI embedding) and a `bm42` sparse vector for every point [S3], [S6].
2.  **Search Endpoint:** The search utilizes Qdrant's dedicated `/points/query` endpoint [S1].
3.  **Fusion:** The two search results (one from dense, one from sparse) are combined using **Reciprocal Rank Fusion (RRF)** [S3], which is achieved by setting `"fusion": "rrf"` in the request body [S1].
4.  **Data Sources:** The dense path performs an HNSW search on the dense vector, while the sparse path searches the BM42 index [S3]. The process fetches a specified number of candidates (`AXON_HYBRID_CANDIDATES` is the default, 100) from each arm to feed into RRF [S1].

### Fallback to Dense-Only Search (Unnamed Mode)
The system includes a fallback mechanism for older or unconfigured collections:

*   **Detection:** The vector mode of the Qdrant collection is detected via `ensure_collection()` [S4].
*   **Fallback Trigger:** If the collection is found to use the legacy `Unnamed` mode, the hybrid search is automatically disabled, and the system falls back to a dense-only search path [S2], [S5].
*   **Legacy Method:** This fallback uses the `/points/search` endpoint, which expects a single, flat vector array, instead of the structured request body required for hybrid search [S8].

### Summary of Modes
| VectorMode | Vector Layout | Search Path |
| :--- | :--- | :--- |
| `Named` (Hybrid) | `dense` (float32) + `bm42` (sparse, IDF modifier) | `/points/query` with RRF prefetch |
| `Unnamed` (Legacy) | single unnamed float32 vector | `/points/search` cosine only |

The process for enabling hybrid search generally involves migrating an existing collection from `Unnamed` mode to `Named` mode, which requires re-embedding all points into the new format [S7].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L1178-L1236
- [S2] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L56-L113
- [S3] https://github.com/jmagar/axon/blob/main/docs/stack/TECH.md#L46-L101
- [S4] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L567-L616
- [S5] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L1504-L1561
- [S6] https://github.com/jmagar/axon/blob/main/docs/plans/complete/2026-03-14-hybrid-search.md#L737-L782
- [S7] https://github.com/jmagar/axon/blob/main/docs/commands/migrate.md#L1-L47
- [S8] https://github.com/jmagar/axon/blob/main/docs/superpowers/plans/2026-03-19-qdrant-index-config-tier2.md#L363-L408
  Timing: retrieval=141ms | context=2ms | llm=10961ms | total=11110ms | ttft=3636ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 18 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/architecture/stack/tech.md#L46-L99 ≈ https://github.com/jmagar/axon/blob/main/docs/stack/TECH.md#L46-L101)


_Status: `0`, elapsed: `11s`_

### Q10

**Question:** What configuration and runtime safeguards prevent Axon ask from overloading smaller-context local models?

**Answer:**

Conversation
  You: What configuration and runtime safeguards prevent Axon ask from overloading smaller-context local models?
  Assistant:
The configuration and runtime safeguards that prevent Axon ask from overloading smaller-context local models are primarily managed through several configurable parameters that control the amount and nature of context retrieved and passed to the LLM.

Key safeguards include:

*   **Context Length Limits:** The maximum context size is constrained by several variables, most notably:
    *   `AXON_ASK_MAX_CONTEXT_CHARS`: This sets a hard cap on the total number of context characters passed to the LLM, with a default of 300,000 characters [S4].
    *   `AXON_ASK_CHUNK_LIMIT`: This determines the maximum number of chunks selected for the LLM context, defaulting to 20 chunks [S4], [S1].
*   **Retrieval Candidate Limits:** To prevent excessive retrieval, limits are set on the candidates gathered during the search:
    *   `AXON_ASK_CANDIDATE_LIMIT`: This caps the total number of retrieval candidates fetched from Qdrant, with a default of 250 [S4], [S1], [S2], [S3].
    *   `AXON_ASK_HYBRID_CANDIDATES`: This controls the hybrid prefetch window per arm, with a default of 150 [S4], [S1].
*   **Content Selection Tiers:** The context assembly process is multi-staged to manage context size:
    *   **Full Documents (Tier 2):** The number of full documents included in the context is controlled by `AXON_ASK_FULL_DOCS`, which defaults to 6 [S4], [S1].
    *   **Backfill Chunks (Tier 3):** An additional layer of supplemental context is added via `AXON_ASK_BACKFILL_CHUNKS`, which defaults to 5 [S4], [S1].
*   **Filtering and Scoring:** To ensure only relevant, high-quality information is passed, minimum relevance scores are enforced:
    *   `AXON_ASK_MIN_RELEVANCE_SCORE`: This sets the minimum score required for a candidate to be included, defaulting to 0.45 [S4], [S1], [S3].
*   **Model-Aware Scaling:** The system supports scaling retrieval depth based on the LLM tier, with specific defaults provided for Large, Medium, and Small models (e.g., Small models have a context limit of 40,000 characters) [S1].

These parameters, which can be tuned via `~/.axon/config.toml` or overridden by environment variables, collectively manage context size to suit the capabilities of the underlying models [S4], [S3].

## Sources
- [S1] https://github.com/jmagar/axon/blob/main/docs/CONFIG.md#L263-L287
- [S2] https://github.com/jmagar/axon/blob/main/docs/PERFORMANCE.md#L88-L158
- [S3] https://github.com/jmagar/axon/blob/main/docs/commands/ask.md#L253-L281
- [S4] https://github.com/jmagar/axon/blob/main/docs/CONTEXT-INJECTION.md#L225-L258
  Timing: retrieval=128ms | context=1ms | llm=9718ms | total=9851ms | ttft=2765ms | streamed=yes
  Session: codex-latest-live-final-20260515181733
  Warnings: Collapsed 19 near-duplicate source chunk(s) before context selection (e.g. https://github.com/jmagar/axon/blob/main/docs/PERFORMANCE.md#L88-L155 ≈ https://github.com/jmagar/axon/blob/main/docs/PERFORMANCE.md#L88-L158)


_Status: `0`, elapsed: `10s`_

