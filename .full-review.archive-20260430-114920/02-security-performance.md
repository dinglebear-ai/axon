# Phase 2: Security and Performance

## Findings

- High — `crates/mcp/server/handlers_embed_ingest.rs:67`
  MCP `embed_start` accepts arbitrary `input`, and the embed prep path can read local files or top-level directory files into memory without path allowlists, symlink escape checks, file-size caps, or secret-file deny rules.
  Impact: an MCP client can cause readable local files such as `.env`, keys, logs, or large files to be indexed and later retrieved through RAG/query paths.
  Fix: default MCP embed to URL/text-only; add explicit opt-in local roots, canonicalize paths, enforce root containment, deny dotfiles/secret patterns, cap bytes before reading, and reject symlink escapes.

- High — `crates/services/acp_llm/ws_runner.rs:59`
  ACP WebSocket tokens are appended to URL query strings, and full `ws_url` values are logged on connection/errors.
  Impact: `AXON_ACP_WS_TOKEN` can leak into logs, service events, crash reports, or shell history.
  Fix: never log URL query strings; log sanitized endpoints, prefer auth headers/subprotocols over query tokens where possible, and centralize URL redaction.

- High — `crates/vector/ops/commands/streaming.rs:194`
  Retrieved source text is injected into the user prompt as `Context:\n{context}` without strong source-data delimiting or a system instruction that treats source text as untrusted and non-instructional.
  Impact: indexed pages can contain prompt-injection instructions that compete with the RAG system prompt. This is especially risky because `ask` is exposed through MCP and may retrieve arbitrary crawled web content.
  Fix: wrap retrieved sources in explicit untrusted-data delimiters, update `ASK_RAG_SYSTEM_PROMPT` to ignore instructions inside sources, and add tests that malicious source text does not alter required citation/source behavior.

- High — `crates/services/search.rs:209`
  Research synthesis injects Tavily snippets into a prompt as `Sources:{context}` without delimiting source text or telling the model to ignore source instructions.
  Impact: web search snippets can prompt-inject the research synthesis path and influence summaries or JSON formatting.
  Fix: use the same untrusted-source wrapper as ask/evaluate and harden the research system prompt.

- High — `crates/vector/ops/commands/evaluate.rs:144`
  Evaluate can automatically discover and enqueue crawl jobs after a model-produced underperformance judgment.
  Impact: an LLM judgment can indirectly trigger outbound crawling without an explicit user confirmation or service option. In an MCP context this is a surprising side effect and can create SSRF/crawl-amplification risk if suggestions include attacker-controlled URLs.
  Fix: disable auto-enqueue by default. Return suggested URLs as data, and require an explicit crawl command or explicit opt-in flag before any job is enqueued.

- Medium — `crates/mcp/server/handlers_query.rs:29`
  MCP query/ask can override `cfg.collection`, and collection names are interpolated into Qdrant URL paths without validation or path-segment encoding.
  Impact: malformed collection names can hit unexpected Qdrant paths, and clients may query collections outside the intended data boundary.
  Fix: validate collection names centrally with a strict allowlist such as `^[A-Za-z0-9_-]{1,64}$`, or encode path segments and enforce an allowed-collections list for MCP.

- Medium — `crates/services/acp_llm/pool.rs:153`
  Ask/evaluate prewarm ACP sessions aggressively, and warm-pool refill can spawn background adapters without an obvious global semaphore for completions/prewarms.
  Impact: concurrent MCP/CLI requests can create many adapter subprocesses and saturate CPU, process slots, or model quota.
  Fix: add `AXON_ACP_MAX_CONCURRENT_COMPLETIONS`, bound warm-pool refill concurrency, and queue or fail fast when saturated.

- Medium — `crates/vector/ops/commands/evaluate/scoring.rs:17`
  Judge reference retrieval fetches `cfg.ask_candidate_limit * 2` candidates without the same low-signal and allowlist guards used by ask retrieval.
  Impact: judge prompts can include local session/log-like content or non-authoritative domains even when ask is configured to avoid them. This creates both data-exposure risk and noisy evaluation.
  Fix: apply ask retrieval filters to judge-reference candidates before reranking and selection.

- Medium — `crates/vector/ops/qdrant/filter.rs:46`
  Invalid `since`/`before` values are logged and ignored.
  Impact: a caller can request constrained retrieval but silently receive unconstrained results.
  Fix: return validation errors from filter construction and surface bad filters as CLI/MCP invalid-params errors.

- Medium — `crates/vector/ops/commands/evaluate.rs:106`
  Structured evaluate runs RAG and baseline answer generation sequentially.
  Impact: evaluate latency is unnecessarily high and can stack ACP adapter timeouts. The code already has pre-warming, but after context retrieval it still waits for the RAG answer before starting the baseline answer.
  Fix: run the two answer futures concurrently after context is built.

- Medium — `crates/vector/ops/commands/ask/context/build.rs:175`
  Full-document retrieval can fetch up to `ask_full_docs * ask_doc_chunk_limit` Qdrant points after top chunks have already consumed budget. Defaults permit 768 full-doc chunks; env limits allow up to 40,000 points per ask.
  Impact: a small number of asks can create large Qdrant read bursts and then discard oversized entries because the context budget is checked only after each full document is rendered.
  Fix: cap full-document fetches based on remaining context budget, avoid fetching documents whose URL is already represented, and consider a byte-aware retrieval path.

- Low — `crates/vector/ops/tei/tei_client.rs:92`
  TEI retry logs include the full TEI URL.
  Impact: if operators put credentials in a TEI URL, logs can disclose them. Current recommended config does not do that, but the code does not redact.
  Fix: log a redacted service URL or host-only endpoint.

- Low — `crates/services/search.rs:337`
  Search/research paths log or display full user queries.
  Impact: queries containing tokens, private URLs, or proprietary text may leak to service logs/UI streams.
  Fix: log query length/hash by default, redact token-like substrings, and reserve full query logging for explicit debug mode.

- Low — `crates/ingest/sessions/claude.rs:111`
  Session ingest reads whole JSONL files into memory and indexes session content without a visible redaction pass.
  Impact: very large session files can spike memory, and sensitive tool outputs can become searchable RAG content.
  Fix: add metadata-size caps, stream JSONL parsing, redact common secret patterns before embedding, and consider an opt-in flag for tool outputs.

## Critical Issues for Phase 3 Context

- Tests should cover side-effect-free evaluate behavior, prompt injection hardening for ask/research, judge-reference filtering parity, duplicate-source context suppression, and vector-mode cache keys.
- Documentation must be updated because `docs/CONTEXT-INJECTION.md` currently describes raw query embedding, concurrent evaluate behavior, and default values that do not match current code.
