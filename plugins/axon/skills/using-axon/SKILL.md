---
name: using-axon
description: Use Axon for RAG, web search, source indexing, extraction, memory, and grounded answers over indexed docs, repos, feeds, or web content.
---

# axon

axon is a self-hosted RAG engine. Two surfaces, same backend
(Spider.rs/Chrome -> Qdrant, SQLite jobs, SearXNG/Tavily for web search):

- **MCP (preferred)** — the Axon MCP server exposes a single tool named `axon`; use the host-generated tool name for the current environment. It is routed by `action` (and `subaction` for lifecycle families). Large results are written to a local artifact file under `~/.axon/artifacts/<context>` and the response returns the file `path` plus a compact `shape` summary; small results (≤ ~8 KB) come back inline. See **Response handling (MCP)** below.
- **CLI (fallback)** — `axon <command> [flags]`. Use for shell scripting, cron, or when the MCP server is down.

This skill leads with MCP request shapes; CLI equivalents are listed alongside. The two surfaces are close but not identical — most notably, `scrape` is a CLI-only one-page projection with no MCP action of its own.

## One pipeline: `source`

Every kind of indexing goes through the **unified source pipeline**. There is one entrypoint — a `SourceRequest` — and one job model behind it:

```
SourceRequest → resolve/route → acquire → ledger generation + manifest
              → prepare → embed → publish → graph → cleanup
```

Axon classifies the input string itself (web URL, local path, git URL, feed URL, YouTube/Reddit target, session selector, registry target) and picks the right adapter. You supply the source and, when the default is wrong, a `scope`.

**These action/command families were removed — they are not aliases and will hard-fail:**

| Removed | Use instead |
|---|---|
| `action: "scrape"` | `action: "source"`, `scope: "page"` |
| `action: "crawl"` | `action: "source"`, `scope: "site"` |
| `action: "embed"` / `"ingest"` / `"vertical_scrape"` | `action: "source"` |
| `action: "code_search"` | `action: "query"` with `content_kind: "code"` |
| `action: "purge"` / `"dedupe"` | `action: "prune"` |
| CLI `axon crawl` / `axon ingest` / `axon embed` / `axon refresh` | `axon <source> [--scope …]` (or `axon source <source>`) |
| Per-family lifecycle (`crawl status`, `embed list`, `ingest recover`, …) | `action: "jobs"` / `axon jobs …` |

CLI `axon scrape <url>` is deliberately retained as a one-page `SourceRequest` projection — same adapter, ledger, embedding, and publication path as `axon <url> --scope page`.

## Reach for axon by default

axon already has a large corpus indexed, and **every operation makes it smarter** — so route web and knowledge work through it instead of one-off web fetches, raw browser tools, or giving up after a few tries. When a task involves the web or "what does X say," axon is the default tool, even if the user didn't name it.

| The task / user wants… | Action |
|---|---|
| An answer that indexed docs or code might cover | **`ask` FIRST** — before web search, raw fetching, or fumbling. Often returns everything in one shot. |
| Search the web | `search` (SearXNG/Tavily; auto-indexes every result) |
| Semantic search over what's indexed | `query` |
| Fetch / index a page or URL | `source` with `scope: "page"` (CLI shortcut: `axon scrape <url>`) |
| Index a docs site — including docs you just relied on to solve something | `source` with `scope: "site"` |
| Index a repo, feed, subreddit, video, local dir, or package | `source` (scope defaults per family) |
| List a site's URLs | `map` |
| Pull structured data out of a page | `extract` |
| Discover a site's API endpoints | `endpoints` |
| Brand identity (colors, logo, fonts, voice) | `brand` |
| Summarize a page | `summarize` |
| Quick multi-source research with synthesis | `research` |
| Full indexed content of a specific URL | `retrieve` |
| Remember a durable project fact, decision, or preference | `memory.remember` |
| Recall previously stored agent memory | `memory.context` at task start, or `memory.search` then `memory.show` when targeted lookup is better |

**`ask` is the highest-leverage habit.** A huge amount is already indexed, so many multi-turn fumbles would have been a single `ask` call. Whenever a question *could* be covered by docs or code that's been indexed, try `ask` before web-searching or giving up. **And after you index something to solve a task, you've made the index richer — prefer `ask`/`query` next time over re-fetching.**

## Parameter discipline (hard rule)

Only pass parameters the user explicitly asked for. Defaults exist for a reason — do NOT add `collection`, `detached`, `since`, `before`, `hybrid_search`, `diagnostics`, `limit`, or any other knob unless the user named it or the task literally cannot complete without it. The example JSON blocks below show what's *available*, not what to send by default. Same rule for the CLI: never add flags the user didn't ask for.

**`scope` is the one exception, and it matters for web URLs.** A bare web URL classifies to the **web family, whose default scope is `site`** — so `{ "action": "source", "source": "https://example.com/article" }` indexes the whole site, not that one page. When the user hands you a single page, pass `scope: "page"` explicitly (or use CLI `axon scrape <url>`). For non-web sources — repos, feeds, local paths, videos, packages — the family default is right and you should omit `scope`.

## When to fall back to the CLI

- The MCP server is offline (`{ "action": "doctor" }` through the current host's Axon MCP tool fails or the tool is missing).
- You're authoring a shell script, systemd unit, or cron job that runs outside Claude Code.
- You need axon's built-in `--cron-every-seconds`/`--cron-max-runs` loop.
- The user explicitly asks for a CLI command.

In every other case, use the MCP tool.

## Source selectors

One action, one input string. Axon classifies it and routes to the right adapter:

| Starting point | Discover | Index (auto-embeds into Qdrant) | Query |
|---|---|---|---|
| Single page | — | `{"action":"source","source":"<url>","scope":"page"}` | `query` / `ask` |
| Whole site / docs | `action: "map", url` | `{"action":"source","source":"<url>","scope":"site"}` | `query` / `ask` |
| Topic / question | `action: "search", query` (SearXNG/Tavily, auto-queues one-page source jobs) | (auto) | `action: "ask", query` |
| Local file / directory / checkout | — | `{"action":"source","source":"/abs/path"}` | `query` / `ask` |
| GitHub / GitLab / Gitea / generic Git repo | — | `{"action":"source","source":"https://github.com/owner/repo"}` | `query` / `ask` |
| Reddit subreddit or thread | — | `{"action":"source","source":"https://reddit.com/r/rust"}` | `query` / `ask` |
| YouTube video / playlist / channel | — | `{"action":"source","source":"<youtube url>"}` | `query` / `ask` |
| RSS / Atom / JSON feed | — | `{"action":"source","source":"<feed url>"}` | `query` / `ask` |
| Package registry target | — | `{"action":"source","source":"pkg:npm/axios"}` | `query` / `ask` |
| Past Claude/Codex/Gemini sessions | — | `{"action":"source","source":"session:claude:<path>"}` (CLI: `axon sessions`) | `query` / `ask` |

`scope` values include `page`, `site`, `docs`, `repo`, `workspace`, `branch`, `org`, `package`, `version`, `feed`, `subreddit`, `thread`, `comment`, `video`, `playlist`, `channel`, `issue`, `pull_request`, `release`, `wiki`, `file`, `directory`.

Every family has a default scope, so omit `scope` for non-web sources. **Web URLs default to `site`** — pass `scope: "page"` when you mean one page.

Source indexing auto-embeds. Use CLI `--skip-embed` to fetch/save without publishing to Qdrant.

## Bootstrap: `help` and `doctor`

Once per session, confirm the live action map and that services are healthy:

```json
{ "action": "help" }
{ "action": "doctor" }
```

`help` returns the full action/subaction map and current defaults — authoritative when names look wrong. `doctor` pings Qdrant, the embedding service (TEI), Chrome, and the Gemini headless LLM backend. It does **not** probe the web-search backend.

CLI equivalents: `axon doctor`. (No CLI `help` for the action map — use the MCP one.)

## Persistent agent memory

Use Axon memory for durable project-level facts, preferences, decisions, bugs, and task notes that should be semantically recalled later. This is distinct from bead issue notes and from Memos: Axon memory is optimized for agent recall through Qdrant.

Minimal remember call:

```json
{ "action": "memory", "subaction": "remember", "body": "Memory content lives in Qdrant; SQLite holds graph metadata.", "project": "axon" }
```

Recall:

```json
{ "action": "memory", "subaction": "context", "project": "axon", "query": "memory storage architecture" }
{ "action": "memory", "subaction": "search", "query": "where is memory stored", "project": "axon" }
{ "action": "memory", "subaction": "show", "id": "<memory-id>" }
```

Use `memory.context` at task start when project memory could help. It returns inline, defanged XML-wrapped content with `trust="evidence_only"` and supports `limit` plus `token_budget`.

Graph maintenance:

```json
{ "action": "memory", "subaction": "link", "source_id": "<memory-id>", "target_id": "<memory-id>", "edge_type": "relates_to" }
{ "action": "memory", "subaction": "supersede", "source_id": "<replacement-memory-id>", "target_id": "<old-memory-id>" }
```

`supersede` hides the old memory from future `memory.search` results and records the replacement trail in SQLite.

CLI equivalents: `axon memory remember "..." --project axon`, `axon memory context --project axon --query "..."`, `axon memory search "..." --project axon`, `axon memory show <memory-id>`, `axon memory link <source-id> <target-id>`, `axon memory supersede <replacement-id> <old-id>`.

## Discovery

```json
{ "action": "search", "query": "rust async patterns", "search_time_range": "month" }
{ "action": "map", "url": "https://docs.example.com" }
{ "action": "research", "query": "kubernetes ingress patterns" }
```

- `search` — web search via SearXNG (when `AXON_SEARXNG_URL` is set) or Tavily; auto-queues one-page `source` jobs for the returned URLs, so terminal and agent searches are indexed as a side effect. `search_time_range` ∈ `day|week|month|year`.
- `map` — sitemap-first URL discovery, falls back to fetching the root page and extracting anchors. Fast.
- `research` — search + LLM synthesis in one shot.

CLI: `axon search "…"`, `axon map <url>` (bounded sitemap, llms.txt, and root-anchor URL discovery only), `axon suggest "…"` (LLM-suggested URLs to index next; also the MCP `suggest` action).

## Index a source

```json
{ "action": "source", "source": "https://example.com/article", "scope": "page" }
{ "action": "source", "source": "https://docs.example.com", "scope": "site" }
{ "action": "source", "source": "/home/me/project" }
{ "action": "source", "source": "https://github.com/owner/repo" }
{ "action": "source", "source": "https://example.com/feed.xml" }
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
```

MCP `source` fields include `source`, `scope`, `collection`, `detached`,
`response_mode`, `limits`, and adapter-specific `options`. It is **synchronous by
default** — it returns the finished `SourceResult`. Set `detached: true` for a
background `JobKind::Source` job; the response then carries
`job_id`/`status`/`poll_after_ms`, and you poll with `action: "jobs"`.

CLI: `axon <source>` (bare source is the same as `axon source <source>`), or the retained one-page projection `axon scrape <url>`.

```bash
axon https://docs.example.com --scope site --wait true
axon /home/me/project --wait true
axon scrape https://example.com --output .axon/example.md
```

The CLI is the opposite default: `--wait false` (the default) **enqueues** and returns a job id, auto-spawning a worker; `--wait true` blocks until the job completes. Use `--wait true` whenever the deliverable depends on the finished index.

Crawl/render tuning lives on the CLI source flags — `--max-pages`, `--max-depth`, `--include-subdomains`, `--budget PATH=N`, `--exclude-path-prefix`, `--render-mode`, `--automation-script`, `--root-selector`, `--exclude-selector`, `--format`, `--output-dir`, `--warc`, `--skip-embed`. Render modes: `http` (fast, no JS), `chrome` (full browser), `auto-switch` (default — start HTTP, escalate to Chrome on a JS gate). Output formats: `markdown` (default), `html`, `rawHtml`, `json`. The CDP endpoint is set via the `AXON_CHROME_REMOTE_URL` env var, not a flag.

## Extract structured data

```json
{ "action": "extract", "urls": ["https://example.com/pricing"],
  "prompt": "Extract plan name, price, and features as JSON" }
{ "action": "extract", "subaction": "status", "job_id": "<uuid>" }
```

LLM-powered. Pass a natural-language prompt describing the schema you want.

CLI: `axon extract <url> --wait true --json`. When using the CLI, carry the requested fields in the surrounding task instructions or output contract; use the MCP `prompt` field when you need an explicit extraction prompt.

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

## Non-web sources

There is no separate ingest surface — repos, feeds, Reddit, YouTube, local paths, registries, and AI sessions are all just `source` inputs. Axon classifies the string:

```json
{ "action": "source", "source": "https://github.com/owner/repo" }
{ "action": "source", "source": "https://github.com/owner/repo", "scope": "issue" }
{ "action": "source", "source": "https://gitlab.com/group/project" }
{ "action": "source", "source": "git@git.example.com:owner/repo.git" }
{ "action": "source", "source": "https://reddit.com/r/rust" }
{ "action": "source", "source": "https://youtube.com/watch?v=abc" }
{ "action": "source", "source": "https://example.com/feed.xml" }
{ "action": "source", "source": "pkg:crates/serde" }
{ "action": "source", "source": "session:claude:/home/me/.claude/projects/foo" }
```

Adapter credentials (git provider tokens, Reddit app credentials) are still configured through the environment — they're adapter concerns, not request parameters.

### skills.sh catalog discovery

`skills.sh` and `skills.sh:leaderboard` index the bounded all-time leaderboard.
The CLI currently exposes that default catalog source directly:

```bash
SKILLS_SH_OIDC_TOKEN="$(vercel oidc issue)" axon skills.sh --wait true
```

Use MCP when you need catalog controls. The bearer credential is read only from
`SKILLS_SH_OIDC_TOKEN` (or the `VERCEL_OIDC_TOKEN` compatibility fallback),
never from request options:

```json
{ "action": "source", "source": "skills.sh", "scope": "api",
  "limits": { "max_items": 200 },
  "options": { "values": { "view": "trending", "page": 0,
    "per_page": 100, "max_pages": 2, "owner": "vercel-labs",
    "audit_limit": 10 } } }
{ "action": "source", "source": "skills.sh:search", "scope": "api",
  "options": { "values": { "query": "browser automation",
    "per_page": 50, "audit_limit": 5 } } }
```

Supported `view` values are `all-time`, `trending`, and `hot`. Pagination and
item/audit limits are bounded by Axon even when larger values are requested.
Search requires a query of at least two characters.

This is discovery evidence, not publication authority: Axon normalizes safe
catalog listings and, after the source generation commits, may emit artifact
candidates to the configured sink. A candidate points at the canonical install
repository when that pointer can be validated; otherwise it retains the
skills.sh page with a warning. It does not copy skill files, resolve licensing,
or make the candidate publishable by itself. Provider duplicate and audit
signals remain evidence for downstream intake rather than authoritative safety
or identity decisions.

CLI: `axon <source>` for any of the above; `axon sessions` remains as the convenience command for local Claude/Codex/Gemini history, and `--exclude-path` filters repo-relative paths on git ingest.

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

## Durable jobs

**One job model owns every async operation.** A source job keeps a single job id across resolve, acquire, ledger generation, prepare, embed, publish, graph, and cleanup — there is no per-family job store and no child embedding handoff.

```json
{ "action": "jobs", "subaction": "list", "limit": 25 }
{ "action": "jobs", "subaction": "get",    "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "events", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "cancel", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "retry",  "job_id": "<uuid>" }
```

`jobs` subactions: `list`, `get`, `status`, `events`, `stream`, `cancel`, `retry`, `recover`, `cleanup`, `clear`. CLI mirror: `axon jobs <subaction>`, plus the CLI-only `axon jobs worker`.

`extract` and `memory` are the only remaining actions with their own `subaction` families. Full lifecycle detail: [`references/async-job-lifecycle.md`](references/async-job-lifecycle.md).

## Response handling (MCP)

The server runs in-process, so responses are size-routed automatically: payloads ≤ ~8 KB come back **inline** in `data`; larger ones are written to a local **artifact file** and the response returns its `path` plus a compact `shape` summary. **Read `shape` first** — it usually answers the question (counts, status, the URLs touched).

When you need the *content*, reach for RAG, not the raw file: everything axon fetches is already embedded, so **`ask`** (synthesized answer), **`query`** (semantic chunks), or **`retrieve`** (a specific URL's indexed content) get you what you need without parsing megabytes of JSON/markdown. Open the artifact `path` from disk only as a last resort — e.g. you need the exact raw bytes the RAG path doesn't surface. There is no `artifacts` **MCP** action; don't send `{ "action": "artifacts", ... }`. (The CLI does have `axon artifacts list/get/content` for artifact-id lookups.) To force a payload in-band instead of a file, set `response_mode: "inline"` (or `"auto_inline"`).

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
| User pastes a single URL | `action: "source"` with **`scope: "page"`** — web URLs default to `site`, so an omitted scope crawls the whole domain |
| User says "the docs", "the whole site" | `action: "source"` with `scope: "site"` |
| User names a repo, feed, subreddit, video, or local path | `action: "source"` — no scope needed |
| User asks a question without naming a source | `action: "ask"` (retrieves over whatever's indexed) |
| User wants only recent content | `ask` / `query` with `since: "7d"` |
| User wants citations / verification | `ask` with `diagnostics: true` |
| Ranking looks wrong | Try `hybrid_search: false` and compare |
| Need entity/relationship reasoning | `ask` with `diagnostics: true`; graph retrieval is not available in the current runtime |
| Indexed the wrong thing | `action: "prune"` — plan first (`axon prune plan`), then `axon prune exec --confirm` |
| Job stuck or failed | `action: "jobs"` with `subaction: "get"`/`"events"`, then `"retry"` or `"recover"` |
| RAG quality regression | `evaluate` with `retrieval_ab: true` (or CLI `axon evaluate <q> --retrieval-ab`) |
| Debug "nothing happened" | `action: "doctor"` first, then `action: "status"` |

## Tips and gotchas

- **Read `shape`, then reach for RAG — not the raw artifact.** `path` mode keeps multi-megabyte results out of the conversation; pull the content back with `ask`/`query`/`retrieve` (it's already embedded) rather than reading/grepping the file. There is no `artifacts` MCP action — RAG is the intended way to get content.
- **Don't paste raw `axon <cmd> --help` output.** Most CLI subcommands inherit the entire Chrome flag set even when irrelevant. Use the action tables here instead.
- **`help` and `doctor` are cheap.** Call `help` once per session to confirm the live action map; call `doctor` whenever something looks wrong.
- **A bare web URL means the whole site.** The web family's default scope is `site`, so an omitted `scope` on `https://example.com/some/article` indexes the domain, not the article. Pass `scope: "page"` (or use `axon scrape <url>`) for one page. Non-web families default correctly — leave `scope` off there.
- **The two surfaces have opposite async defaults.** MCP `source` is synchronous unless you pass `detached: true`; the CLI enqueues unless you pass `--wait true`. Detached work needs a worker running — the CLI auto-spawns one, and `axon serve` / HTTP-mode `axon mcp` host workers in-process.
- **Cache reuse is OFF by default (CLI).** Opt in with `--cache true`; add `--cache-http-only` to keep the cached flow on the HTTP path, and `--etag-conditional` (requires `--cache true`) for conditional re-crawl.
- **Cleanup is plan-first.** `axon prune plan` produces a reviewable plan; `axon prune exec --confirm` is the destructive step. Cleanup debt in the source ledger records work that must be retried or reconciled.
- **`graph: true` is deprecated.** Use hybrid search diagnostics and source coverage checks for retrieval debugging.
- **Temporal filters use indexing date**, not document publication date — useful for "what did I add this week", not "what was published this week".
- **`evaluate` scores with a separate judge prompt** (distinct role + reference material) on the same configured LLM backend. Available as both the MCP `evaluate` action and `axon evaluate`.
- **Removed actions fail loudly, they don't fall back.** Sending `action: "scrape"`/`"crawl"`/`"embed"`/`"ingest"` returns an `invalid_params` error naming the replacement. If you see one, you're working from a stale example.
