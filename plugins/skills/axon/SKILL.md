---
name: axon
description: Use whenever the user wants to crawl, scrape, or extract a website; ingest a GitHub repo, Reddit, YouTube, or local AI sessions; embed content into Qdrant; run semantic search; ask grounded RAG questions; or manage axon's async job queues. Also use when the user mentions axon, the crawler, hybrid search, Qdrant, Tavily, or the MCP tool surface.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon

axon is a self-hosted RAG engine. Two surfaces, same backend
(Spider.rs/Chrome -> Qdrant, SQLite jobs, Tavily for web search):

- **MCP (preferred)** — single tool `mcp__axon__axon`, routed by `action` (and `subaction` for lifecycle families). Default `response_mode=path` writes artifacts to `.cache/axon-mcp/` and returns a compact `shape` summary.
- **CLI (fallback)** — `axon <command> [flags]`. Use for shell scripting, cron, or when the MCP server is down.

Both surfaces accept the same operations and parameters. This skill leads with MCP request shapes; CLI equivalents are listed alongside.

## Parameter discipline (hard rule)

Only pass parameters the user explicitly asked for. Defaults exist for a reason — do NOT add `max_pages`, `max_depth`, `render_mode`, `include_subdomains`, `since`, `before`, `hybrid_search`, `diagnostics`, `format`, `root_selector`, `exclude_selector`, `limit`, `collection`, or any other knob unless the user named it or the task literally cannot complete without it. The example JSON blocks below show what's *available*, not what to send by default. A bare `{ "action": "scrape", "url": "…" }` or `{ "action": "crawl", "urls": [...] }` is almost always the right call. Same rule for the CLI: never add flags the user didn't ask for.

## When to fall back to the CLI

- The MCP server is offline (`mcp__axon__axon { "action": "doctor" }` fails or the tool is missing).
- You're authoring a shell script, systemd unit, or cron job that runs outside Claude Code.
- You need axon's built-in `--cron-every-seconds`/`--cron-max-runs` loop.
- The user explicitly asks for a CLI command.

In every other case, use the MCP tool.

## The pipeline

```
URL or query → discover → fetch + embed → query / ask
```

| Starting point | Discover | Fetch + embed (auto-embeds into Qdrant) | Query |
|---|---|---|---|
| Single URL | — | `action: "scrape", url` | `query` / `ask` |
| Whole site / docs | `action: "map", url` | `action: "crawl", urls` | `query` / `ask` |
| Topic / question | `action: "search", query` (Tavily, auto-queues crawl) | (auto) | `action: "ask", query` |
| Existing local file/dir | — | `action: "embed", input` | `query` / `ask` |
| GitHub repo | — | `action: "ingest", source_type: "github", target: "owner/repo"` | `query` / `ask` |
| Reddit thread or subreddit | — | `action: "ingest", source_type: "reddit", target: "r/name"` | `query` / `ask` |
| YouTube video | — | `action: "ingest", source_type: "youtube", target: "<url>"` | `query` / `ask` |
| Past Claude/Codex/Gemini sessions | — | CLI only: `axon sessions` | `query` / `ask` |

`scrape`, `crawl`, `embed`, and the `ingest` paths all auto-embed unless you set `embed: false`.

## Bootstrap: `help` and `doctor`

Once per session, confirm the live action map and that services are healthy:

```json
{ "action": "help" }
{ "action": "doctor" }
```

`help` returns the full action/subaction map and current defaults — authoritative when names look wrong. `doctor` pings Qdrant, Chrome, Tavily, configured LLM backend readiness, and the embedding service.

CLI equivalents: `axon doctor`. (No CLI `help` for the action map — use the MCP one.)

## Discovery

```json
{ "action": "search", "query": "rust async patterns", "search_time_range": "month" }
{ "action": "map", "url": "https://docs.example.com" }
{ "action": "research", "query": "kubernetes ingress patterns" }
```

- `search` — Tavily web search; auto-queues crawl jobs for results. `search_time_range` ∈ `day|week|month|year`.
- `map` — sitemap-first URL discovery, falls back to fetching the root page and extracting anchors. Fast.
- `research` — search + LLM synthesis in one shot.

CLI: `axon search "…"`, `axon map <url>` (use `--map-fallback crawl` only when you need a full Spider walk; the structure-fallback default is fast), `axon suggest "…"` (LLM-suggested URLs to crawl next; not exposed via MCP).

## Fetch + embed

```json
{ "action": "scrape", "url": "https://example.com/article" }
{ "action": "scrape", "url": "https://example.com",
  "root_selector": "article, main",
  "exclude_selector": ".sidebar, .ads",
  "format": "markdown" }
{ "action": "scrape", "url": "https://example.com", "format": "html", "embed": false }

{ "action": "crawl", "urls": ["https://docs.example.com"],
  "max_pages": 200, "max_depth": 3, "include_subdomains": true }
{ "action": "crawl", "urls": ["https://docs.example.com"], "render_mode": "chrome" }

{ "action": "embed", "input": "./docs" }
{ "action": "embed", "input": "https://example.com" }
```

Render modes: `http` (fast, no JS), `chrome` (full browser), `auto_switch` (default — start HTTP, escalate to Chrome on JS gate).

Output formats: `markdown` (default), `html`, `raw_html`, `json`.

CLI: `axon scrape <url>` / `axon crawl <url> --max-pages N --max-depth N` / `axon embed <input>`. Chrome knobs (`--chrome-anti-bot`, `--chrome-stealth`, `--chrome-intercept`, `--chrome-headless`, `--chrome-proxy`, `--chrome-remote-url`) are pre-tuned and rarely need overriding. Output dir defaults to `.cache/axon-rust/output/` (env `AXON_OUTPUT_DIR`).

## Extract structured data

```json
{ "action": "extract", "urls": ["https://example.com/pricing"],
  "prompt": "Extract plan name, price, and features as JSON" }
{ "action": "extract", "subaction": "status", "job_id": "<uuid>" }
```

LLM-powered. Pass a natural-language prompt describing the schema you want.

CLI: `axon extract <url> --query "…"` (the `--query` flag carries the extraction prompt).

## Ingest external sources

```json
{ "action": "ingest", "source_type": "github", "target": "owner/repo" }
{ "action": "ingest", "source_type": "github", "target": "owner/repo", "include_source": false }

{ "action": "ingest", "source_type": "reddit", "target": "r/rust" }
{ "action": "ingest", "source_type": "reddit", "target": "https://reddit.com/r/rust/comments/abc123/..." }

{ "action": "ingest", "source_type": "youtube", "target": "https://youtube.com/watch?v=abc" }

```

`source_type` ∈ `github | reddit | youtube`. Lifecycle subactions (`status`, `cancel`, `list`, `cleanup`, `clear`, `recover`) work the same as crawl/embed/extract.

CLI: `axon ingest <target>` with source-specific flags. GitHub: `--include-source`/`--no-source`, `--max-issues`, `--max-prs` (default 100; `0` = unlimited). Reddit: `--sort hot|top|new|rising`, `--time hour|…|all`, `--max-posts`, `--min-score`, `--depth`, `--scrape-links`. Use `axon sessions` for Claude/Codex/Gemini local history; in server mode the CLI uploads prepared, redacted documents to the server-side async ingest queue. MCP legacy sessions ingest is rejected to avoid scanning server-local history paths.

## Query and RAG

```json
{ "action": "query", "query": "embedding pipeline", "limit": 10, "collection": "cortex" }
{ "action": "query", "query": "rate limiting", "since": "7d" }

{ "action": "ask", "query": "How does axon handle Chrome auto-switching?" }
{ "action": "ask", "query": "...", "since": "7d" }
{ "action": "ask", "query": "...", "since": "2026-01-01", "before": "2026-03-01" }
{ "action": "ask", "query": "...", "diagnostics": true }
{ "action": "ask", "query": "...", "hybrid_search": false }
```

- `query` — pure semantic vector search (top-K chunks).
- `ask` — RAG: retrieve, then synthesize an answer with citations.
- **Hybrid search** (dense + BM42 sparse + RRF) is on by default; `hybrid_search: false` forces dense-only for A/B comparison or when sparse is misbehaving. Server default: env `AXON_HYBRID_SEARCH`.
- Temporal filters (`since`/`before`) accept `7d`, `30d`, `YYYY-MM-DD`, or RFC3339. They filter on **indexing date**, not publication date.
- `collection` overrides the default `cortex` collection per request.

```json
{ "action": "retrieve", "url": "https://example.com/article" }
```

CLI: `axon query "…"` / `axon ask "…" --since 7d --diagnostics` / `axon retrieve <url>`.

`evaluate` is CLI-only: `axon evaluate "<question>" --retrieval-ab` compares hybrid-RAG vs dense-only with an independent LLM judge scoring accuracy/relevance/completeness.

## Inspect the index

```json
{ "action": "sources" }
{ "action": "domains" }
{ "action": "stats" }
{ "action": "status" }                                 // global queue snapshot
```

CLI: `axon sources` / `axon domains` / `axon stats` / `axon status`.

## Async jobs

`crawl`, `extract`, `embed`, `ingest` are async by default. `subaction` defaults to `start` — `{ "action": "crawl", "urls": [...] }` is enough to enqueue. Poll with `subaction: "status"`; cancel with `subaction: "cancel"`. Full lifecycle subactions (`list`, `cleanup`, `clear`, `recover`) and CLI-only surfaces (`errors`, `worker`) are in [`references/async-job-lifecycle.md`](references/async-job-lifecycle.md).

## Response handling (MCP)

Default `response_mode` is `path` — responses include a `shape` summary and an `artifact` file path. **Read `shape` first**; escalate to `artifacts` subactions (`head`, `grep`, `search`, `read`) only when needed. Full artifact ops, cleanup commands, and error codes: [`references/mcp-response-protocol.md`](references/mcp-response-protocol.md).

## MCP resources

- `axon://schema/mcp-tool` — full JSON schema and routing contract (read this when you need exact field types/enums).
- `ui://axon/status-dashboard` — interactive MCP App widget for live queue status.

## Choosing parameters — quick guide

| Situation | Reach for |
|---|---|
| User pastes a single URL | `action: "scrape"` |
| User says "the docs", "the whole site" | `action: "crawl"` with `max_pages` + `max_depth` |
| User asks a question without naming a source | `action: "ask"` (retrieves over whatever's indexed) |
| User wants only recent content | `ask` / `query` with `since: "7d"` |
| User wants citations / verification | `ask` with `diagnostics: true` |
| Ranking looks wrong | Try `hybrid_search: false` and compare |
| Need entity/relationship reasoning | `ask` with `diagnostics: true`; graph retrieval is not available in the current runtime |
| Crawled the wrong thing | `crawl` `subaction: "clear"` or per-family `cleanup` |
| RAG quality regression | CLI `axon evaluate <q> --retrieval-ab` |
| Debug "nothing happened" | `action: "doctor"` first, then `action: "status"` |

## Tips and gotchas

- **Read `shape` before opening artifacts.** That's the whole point of `path` mode — it keeps multi-megabyte crawl results out of the conversation.
- **Don't paste raw `axon <cmd> --help` output.** Most CLI subcommands inherit the entire Chrome flag set even when irrelevant. Use the action tables here instead.
- **`help` and `doctor` are cheap.** Call `help` once per session to confirm the live action map; call `doctor` whenever something looks wrong.
- **Async by default.** Lifecycle actions return a `job_id`; poll with `subaction: "status"`. CLI users can pass `--wait true` for synchronous one-shots.
- **Cache reuse is opt-in (CLI flag).** `--cache true` reuses prior crawl artifacts; combine with `--cache-skip-browser true` to force the HTTP path even when the cached job used Chrome.
- **`graph: true` is deprecated.** Use hybrid search diagnostics and source coverage checks for retrieval debugging.
- **Temporal filters use indexing date**, not document publication date — useful for "what did I add this week", not "what was published this week".
- **`evaluate` uses an independent LLM judge.** The judge is not the model that generated the answer, so its scores are usable as-is. Currently CLI-only.
- **`subaction` defaults to `start`** for lifecycle families — `{ "action": "crawl", "urls": [...] }` is enough to enqueue a job.
