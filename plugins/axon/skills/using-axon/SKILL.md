---
name: using-axon
description: >-
  Self-hosted RAG engine and web toolkit. Use it to: answer questions indexed docs/code might cover (ask — a large corpus is already indexed; try ask BEFORE web-searching or giving up); search the web (search, auto-indexes results); semantic-search the index (query); scrape/fetch a page (scrape); crawl a docs site or pages you just used (crawl); map a site's URLs (map); extract structured data (extract); discover API endpoints (endpoints); extract brand identity — colors/logo/fonts/voice (brand); summarize a page (summarize); quick multi-source research (research); retrieve a URL's full indexed content (retrieve); embed local files/dirs (embed); ingest GitHub/GitLab/Gitea/Git repos, Reddit, YouTube, and Claude/Codex/Gemini sessions (ingest). Also triggers on axon, RAG, Qdrant, SearXNG, Tavily, hybrid/vector search.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon

axon is a self-hosted RAG engine. Two surfaces, same backend
(Spider.rs/Chrome -> Qdrant, SQLite jobs, SearXNG/Tavily for web search):

- **MCP (preferred)** — single tool `mcp__plugin_axon_axon__axon`, routed by `action` (and `subaction` for lifecycle families). Large results are written to a local artifact file under `~/.axon/artifacts/<context>` and the response returns the file `path` plus a compact `shape` summary; small results (≤ ~8 KB) come back inline. See **Response handling (MCP)** below.
- **CLI (fallback)** — `axon <command> [flags]`. Use for shell scripting, cron, or when the MCP server is down.

Both surfaces accept the same operations and parameters. This skill leads with MCP request shapes; CLI equivalents are listed alongside.

## Reach for axon by default

axon already has a large corpus indexed, and **every operation makes it smarter** — so route web and knowledge work through it instead of one-off web fetches, raw browser tools, or giving up after a few tries. When a task involves the web or "what does X say," axon is the default tool, even if the user didn't name it.

| The task / user wants… | Action |
|---|---|
| An answer that indexed docs or code might cover | **`ask` FIRST** — before web search, raw fetching, or fumbling. Often returns everything in one shot. |
| Search the web | `search` (SearXNG/Tavily; auto-indexes every result) |
| Semantic search over what's indexed | `query` |
| Fetch / scrape a page or URL | `scrape` |
| Crawl a docs site — including docs you just relied on to solve something | `crawl` |
| List a site's URLs | `map` |
| Pull structured data out of a page | `extract` |
| Discover a site's API endpoints | `endpoints` |
| Brand identity (colors, logo, fonts, voice) | `brand` |
| Summarize a page | `summarize` |
| Quick multi-source research with synthesis | `research` |
| Full indexed content of a specific URL | `retrieve` |
| Embed local files / directories | `embed` |
| Index a GitHub/GitLab/Gitea/Git repo, Reddit, YouTube, or AI sessions | `ingest` |

**`ask` is the highest-leverage habit.** A huge amount is already indexed, so many multi-turn fumbles would have been a single `ask` call. Whenever a question *could* be covered by docs or code that's been indexed, try `ask` before web-searching or giving up. **And after you crawl/scrape/ingest something to solve a task, you've made the index richer — prefer `ask`/`query` next time over re-fetching.**

## Parameter discipline (hard rule)

Only pass parameters the user explicitly asked for. Defaults exist for a reason — do NOT add `max_pages`, `max_depth`, `render_mode`, `include_subdomains`, `since`, `before`, `hybrid_search`, `diagnostics`, `format`, `root_selector`, `exclude_selector`, `limit`, `collection`, or any other knob unless the user named it or the task literally cannot complete without it. The example JSON blocks below show what's *available*, not what to send by default. A bare `{ "action": "scrape", "url": "…" }` or `{ "action": "crawl", "urls": [...] }` is almost always the right call. Same rule for the CLI: never add flags the user didn't ask for.

## When to fall back to the CLI

- The MCP server is offline (`mcp__plugin_axon_axon__axon { "action": "doctor" }` fails or the tool is missing).
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
| Topic / question | `action: "search", query` (SearXNG/Tavily, auto-queues crawl) | (auto) | `action: "ask", query` |
| Existing local file/dir | — | `action: "embed", input` | `query` / `ask` |
| GitHub / GitLab / Gitea / Git repo | — | `action: "ingest", source_type: "github", target: "owner/repo"` | `query` / `ask` |
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

`help` returns the full action/subaction map and current defaults — authoritative when names look wrong. `doctor` pings Qdrant, the embedding service (TEI), Chrome, and the Gemini headless LLM backend. It does **not** probe the web-search backend.

CLI equivalents: `axon doctor`. (No CLI `help` for the action map — use the MCP one.)

## Discovery

```json
{ "action": "search", "query": "rust async patterns", "search_time_range": "month" }
{ "action": "map", "url": "https://docs.example.com" }
{ "action": "research", "query": "kubernetes ingress patterns" }
```

- `search` — web search via SearXNG (when `AXON_SEARXNG_URL` is set) or Tavily; auto-queues crawl jobs for results. `search_time_range` ∈ `day|week|month|year`.
- `map` — sitemap-first URL discovery, falls back to fetching the root page and extracting anchors. Fast.
- `research` — search + LLM synthesis in one shot.

CLI: `axon search "…"`, `axon map <url>` (use `--map-fallback crawl` only when you need a full Spider walk; the structure-fallback default is fast), `axon suggest "…"` (LLM-suggested URLs to crawl next; also the MCP `suggest` action).

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

Output formats: `markdown` (default), `html`, `rawHtml`, `json`.

CLI: `axon scrape <url>` / `axon crawl <url> --max-pages N --max-depth N` / `axon embed <input>`. Chrome rendering is pre-tuned and rarely needs overriding; the CDP endpoint is set via the `AXON_CHROME_REMOTE_URL` env var, not a CLI flag. Output dir defaults to `.cache/axon-rust/output/` (env `AXON_OUTPUT_DIR`).

## Extract structured data

```json
{ "action": "extract", "urls": ["https://example.com/pricing"],
  "prompt": "Extract plan name, price, and features as JSON" }
{ "action": "extract", "subaction": "status", "job_id": "<uuid>" }
```

LLM-powered. Pass a natural-language prompt describing the schema you want.

CLI: `axon extract <url> --query "…"` (the `--query` flag carries the extraction prompt).

## Web utilities: summarize, endpoints, brand, diff, screenshot

Page-level analysis actions — each takes a `url` (except `diff`) and a bare call is the right default:

```json
{ "action": "summarize",  "url": "https://example.com/long-article" }
{ "action": "endpoints",  "url": "https://app.example.com" }
{ "action": "brand",      "url": "https://example.com" }
{ "action": "diff",       "url_a": "https://example.com/v1", "url_b": "https://example.com/v2" }
{ "action": "screenshot", "url": "https://example.com" }
```

- **`summarize`** — fetch a page and return a concise summary (also accepts `urls` for several at once; `root_selector`/`exclude_selector` to scope).
- **`endpoints`** — discover a site's API endpoints by scanning its JavaScript bundles. Optional knobs (`verify`, `capture_network`, `probe_rpc`, `first_party_only`) — omit unless asked.
- **`brand`** — extract brand identity (colors, logo, fonts, voice/tone) from a URL.
- **`diff`** — compare two URLs (`url_a`, `url_b`); reports content/metadata/link changes.
- **`screenshot`** — full-page capture via headless Chrome (`full_page`, `viewport`, `output` optional).

CLI: `axon summarize <url>` / `axon endpoints <url>` / `axon brand <url>` / `axon diff <url-a> <url-b>` / `axon screenshot <url>`.

## Ingest external sources

```json
{ "action": "ingest", "source_type": "github", "target": "owner/repo" }
{ "action": "ingest", "source_type": "github", "target": "owner/repo", "include_source": false }

{ "action": "ingest", "source_type": "gitlab",  "target": "group/project" }
{ "action": "ingest", "source_type": "gitea",   "target": "https://gitea.example.com/owner/repo" }
{ "action": "ingest", "source_type": "git",     "target": "https://git.example.com/owner/repo.git" }

{ "action": "ingest", "source_type": "reddit", "target": "r/rust" }
{ "action": "ingest", "source_type": "reddit", "target": "https://reddit.com/r/rust/comments/abc123/..." }

{ "action": "ingest", "source_type": "youtube", "target": "https://youtube.com/watch?v=abc" }
```

> **Note:** `source_type: "sessions"` is **not** accepted over MCP (it's rejected server-side). Ingest local AI sessions with the CLI `axon sessions` instead — see below.

`source_type` ∈ `github | gitlab | gitea | git | reddit | youtube | sessions`:

- **`github` / `gitlab` / `gitea`** — hosted-forge repos, indexing code + metadata + issues/PRs (or merge requests). `target` is `owner/repo` (or a full URL for self-hosted instances).
- **`git`** — any generic Git remote by clone URL (code only, no forge API).
- **`reddit`** — a subreddit (`r/name`) or a specific thread URL.
- **`youtube`** — a video URL (transcript ingest).
- **`sessions`** — local Claude/Codex/Gemini session transcripts. **CLI-only** via `axon sessions` (enqueues a local sessions ingest job). The MCP `ingest` action rejects `source_type: "sessions"`; remote callers must use the prepared-documents HTTP endpoint (`/v1/ingest/sessions/prepared`).

Lifecycle subactions (`status`, `cancel`, `list`, `cleanup`, `clear`, `recover`) work the same as crawl/embed/extract.

CLI: `axon ingest <target>` with source-specific flags. GitHub: `--include-source`/`--no-source`, `--max-issues`, `--max-prs` (default 100; `0` = unlimited). Reddit: `--sort hot|top|new|rising`, `--time hour|…|all`, `--max-posts`, `--min-score`, `--depth`, `--scrape-links`. Use `axon sessions` for Claude/Codex/Gemini local history (enqueues a local sessions ingest job). MCP sessions ingest is rejected to avoid scanning server-local history paths — remote callers use the `/v1/ingest/sessions/prepared` HTTP endpoint instead.

## Query and RAG

```json
{ "action": "query", "query": "embedding pipeline", "limit": 10, "collection": "axon" }
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
- `collection` overrides the default `axon` collection per request (env `AXON_COLLECTION`).

```json
{ "action": "retrieve", "url": "https://example.com/article" }
```

CLI: `axon query "…"` / `axon ask "…" --since 7d --diagnostics` / `axon retrieve <url>`.

`evaluate` — `{ "action": "evaluate", "query": "<question>", "retrieval_ab": true }` (CLI: `axon evaluate "<question>" --retrieval-ab`) compares hybrid-RAG vs dense-only, scored by a separate judge prompt on the configured LLM backend (accuracy/relevance/completeness).

## Inspect the index

```json
{ "action": "sources" }
{ "action": "domains" }
{ "action": "stats" }
{ "action": "status" }                                 // global queue snapshot
```

CLI: `axon sources` / `axon domains` / `axon stats` / `axon status`.

## Async jobs

`crawl`, `extract`, `embed`, `ingest` are async by default. `subaction` defaults to `start`, so `{ "action": "crawl", "urls": [...] }` is enough to enqueue; poll with `subaction: "status"`. Full lifecycle subactions and CLI-only surfaces: [`references/async-job-lifecycle.md`](references/async-job-lifecycle.md).

## Response handling (MCP)

The server runs in-process, so responses are size-routed automatically: payloads ≤ ~8 KB come back **inline** in `data`; larger ones are written to a local **artifact file** and the response returns its `path` plus a compact `shape` summary. **Read `shape` first** — it usually answers the question (counts, status, the URLs touched).

When you need the *content*, reach for RAG, not the raw file: everything axon fetches is already embedded, so **`ask`** (synthesized answer), **`query`** (semantic chunks), or **`retrieve`** (a specific URL's indexed content) get you what you need without parsing megabytes of JSON/markdown. Open the artifact `path` from disk only as a last resort — e.g. you need the exact raw bytes the RAG path doesn't surface. There is **no `artifacts` MCP action**; don't send `{ "action": "artifacts", ... }`. To force a payload in-band instead of a file, set `response_mode: "inline"` (or `"auto_inline"`).

Full response-mode contract and the JSON-RPC error model: [`references/mcp-response-protocol.md`](references/mcp-response-protocol.md).

## MCP resources

- `axon://schema/mcp-tool` — full JSON schema and routing contract (read this when you need exact field types/enums).
- `ui://axon/status-dashboard` — interactive MCP App widget for live queue status.

## Configuration

This skill names only the handful of env vars that matter at the point of use (`AXON_COLLECTION`, `AXON_SEARXNG_URL`, `AXON_HYBRID_SEARCH`, `AXON_OUTPUT_DIR`, `AXON_DATA_DIR`, …). The **full** surface — every `AXON_*` env var and `~/.axon/config.toml` tuning key — is documented authoritatively and is **not** duplicated here:

- `config.example.toml` (repo root) — all `config.toml` tuning knobs with defaults.
- `.env.example` (repo root) — service URLs, API keys, and secrets.
- `docs/guides/configuration.md` — full environment-variable reference + the two-layer (`.env` + `config.toml`) priority model.

Priority: CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.

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
| RAG quality regression | `evaluate` with `retrieval_ab: true` (or CLI `axon evaluate <q> --retrieval-ab`) |
| Debug "nothing happened" | `action: "doctor"` first, then `action: "status"` |

## Tips and gotchas

- **Read `shape`, then reach for RAG — not the raw artifact.** `path` mode keeps multi-megabyte results out of the conversation; pull the content back with `ask`/`query`/`retrieve` (it's already embedded) rather than reading/grepping the file. There is no `artifacts` action — it was removed precisely because RAG is the intended way to get content.
- **Don't paste raw `axon <cmd> --help` output.** Most CLI subcommands inherit the entire Chrome flag set even when irrelevant. Use the action tables here instead.
- **`help` and `doctor` are cheap.** Call `help` once per session to confirm the live action map; call `doctor` whenever something looks wrong.
- **Async by default.** Lifecycle actions return a `job_id`; poll with `subaction: "status"`. CLI users can pass `--wait true` for synchronous one-shots.
- **Cache reuse is on by default (CLI).** Disable with `--cache false`; add `--cache-http-only` to force the HTTP path even when the cached job used Chrome.
- **`graph: true` is deprecated.** Use hybrid search diagnostics and source coverage checks for retrieval debugging.
- **Temporal filters use indexing date**, not document publication date — useful for "what did I add this week", not "what was published this week".
- **`evaluate` scores with a separate judge prompt** (distinct role + reference material) on the same configured LLM backend. Available as both the MCP `evaluate` action and `axon evaluate`.
- **`subaction` defaults to `start`** for lifecycle families — `{ "action": "crawl", "urls": [...] }` is enough to enqueue a job.
