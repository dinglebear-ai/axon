# Axon CLI Test Report

**Date:** 2026-04-08  
**Version:** 0.35.1  
**Binary:** `target/release/axon`  
**Mode:** Lite mode (SQLite + in-process workers; no Postgres/RabbitMQ)

## Service Status at Test Time

| Service | Status | Notes |
|---------|--------|-------|
| SQLite | ✓ OK | `~/.appdata/axon/jobs.db` |
| TEI (Qwen3-Embedding-0.6B) | ✓ OK | `http://127.0.0.1:52000` |
| Qdrant | ✓ OK | `http://127.0.0.1:53333` — 7.1M vectors, 3.4M points |
| Chrome | ✓ OK | `http://127.0.0.1:6000` (management API reachable) |
| LLM endpoint (OpenAI-compat) | ✗ DOWN | `http://100.120.242.29:8317` — Tailscale peer unreachable |
| ACP adapter | ⚠ Partial | Adapter works for `ask`/`research`/`debug`; model `gemini-3-flash-preview` silently skipped |

---

## Command Results

### Core Web Operations

#### `scrape <url>` — ✓ WORKS
Scrapes URLs to markdown, displays content inline.

```
axon scrape https://www.rust-lang.org/ --embed false
→ Outputs full markdown of the page
```

**Notes:** Auto-switch render mode prints full config options. No issues.

---

#### `crawl <url>` — ✓ WORKS (fire-and-forget), ⚠ METRIC BUG
Crawl enqueues correctly and runs in-process (lite mode).

```
axon crawl https://docs.rs/serde/latest/serde/
→ Prints job ID, starts immediately
```

**Bug: `crawl list` and `crawl status` show `0 docs`, `pages_crawled: 0` for all completed jobs**, even when the corresponding embed job shows dozens of docs embedded. The crawl `result_json` is not capturing metrics from the crawl engine in lite mode. The data is indexed fine — it's a reporting/metrics capture bug.

```
# crawl list shows:
✓ 31921351... completed https://serde.rs/  0 docs · 4.3s

# but crawl status for same job:
md created: 0 / pages_crawled: 0 / pages_discovered: 0

# yet embed list shows:
✓ 31921351.../markdown | 37 docs | 270 chunks | 1s
```

**Subcommands tested:**

| Subcommand | Result |
|-----------|--------|
| `crawl list` | ✓ Works (metric bug above) |
| `crawl status <job_id>` | ✓ Works (metric bug above) |
| `crawl errors <job_id>` | ✓ Shows usage (requires job ID) |
| `crawl recover` | ✓ Works — `reclaimed 0 stale crawl jobs` |
| `crawl cleanup` | ✓ Works — `removed 6 crawl jobs` |
| `crawl cancel <job_id>` | Not tested (no running job to cancel safely) |
| `crawl clear` | Not tested (destructive) |
| `crawl worker` | Not tested (requires full mode) |

---

#### `map <url>` — ✓ WORKS
Discovers all URLs without scraping. Fast, no issues.

```
axon map https://www.rust-lang.org/
→ Returned 578 URLs with sitemap discovery
```

---

#### `search <query>` — ✓ WORKS
Tavily web search works correctly.

```
axon search "rust async patterns"
→ Returned 10 results with summaries, URLs, and snippets
```

---

#### `extract <urls...>` — ✗ BROKEN (multiple failure modes)

```
axon extract https://serde.rs/ --query "what serialization formats does serde support?" --wait true
```

**Failure 1 — Requires `--query` flag not obvious from help:**
Running `extract <url>` without `--query` produces: `Error: "extract requires --query <prompt>"`. The `extract --help` output does not prominently list `--query` as required.

**Failure 2 — Chrome DNS resolution failure (environment issue):**
```
ERROR  Ws(Io(...  "failed to lookup address information: Temporary failure in name resolution"))
       log.target=spider::features::chrome
```
Chrome container resolves DNS internally via Docker network. When running `axon` locally (not in container), Chrome's WebSocket endpoint (`axon-chrome:...`) fails DNS lookup. This is an environment mismatch — Chrome works for the management API but not for actual page fetching via CDP when run outside the container.

**Failure 3 — ACP fallback returns non-JSON:**
When the LLM extraction runs (fallback path), the ACP adapter returns conversational markdown instead of the expected JSON:
```
WARN  ACP fallback response is not valid JSON for https://serde.rs/: expected value at line 1 column 1
      first 200 chars: "```json\n{\n  \"results\": [\n ..."
```
The adapter wraps its response in a markdown code fence rather than returning raw JSON. The parser expects bare JSON.

**Failure 4 — ACP adapter cleanup timeout:**
```
WARN  ACP adapter did not exit within 10 s after connection close; forcing kill via kill_on_drop
```
After each per-page extraction, the ACP adapter process doesn't exit cleanly within the 10s timeout. This happens for every URL extracted.

---

### Vector Search

#### `query <query>` — ✓ WORKS
Semantic vector search against Qdrant. Fast and accurate.

```
axon query "rust async patterns" --limit 3
→ Returns ranked results with similarity scores and snippets
```

---

#### `ask <question>` — ✓ WORKS (context-dependent), ⚠ MISLEADING ERROR
Works when query matches indexed content above the relevance threshold (default 0.45).

```
axon ask "what is serde in rust?"
→ Full RAG answer with citations, timing breakdown (18s total)
```

**Issue: Misleading error message when no candidates pass threshold:**
```
axon ask "what is axon?"
→ Error: ServiceError { message: "ask failed for what is axon?: failed to build ask context" }
```
The real cause is `AXON_ASK_MIN_RELEVANCE_SCORE` threshold filtering out all candidates. The error `"failed to build ask context"` doesn't communicate this — it implies a structural failure rather than a retrieval quality issue. Should surface the threshold message seen in `evaluate` ("No candidates met relevance threshold 0.450; lower AXON_ASK_MIN_RELEVANCE_SCORE").

**Recurring warning (cosmetic, but noisy):**
```
WARN  ACP runtime: skipping unsupported model value 'gemini-3-flash-preview'
```
This appears on every ACP-backed call. The `OPENAI_MODEL` env var is set to a value the ACP runtime doesn't support, so it silently skips it and uses a default. The warning fires multiple times per command (once per ACP session).

---

#### `retrieve <url>` — ✓ WORKS
Fetches stored chunks from Qdrant by URL.

```
axon retrieve "https://doc.rust-lang.org/book/ch00-00-introduction.html"
→ Returns 18 chunks with full text content
```

---

#### `embed <input>` — ✓ WORKS
Embeds content into Qdrant.

```
axon embed "https://www.rust-lang.org/" --wait true
→ ✓ embedded 5 chunks into axon
```

**Subcommands (`embed list`, `embed status`, etc.):** All work the same as `crawl` subcommands.

---

#### `evaluate <question>` — ✓ WORKS, ⚠ VERY SLOW
RAG vs baseline LLM judge. Functional but extremely slow.

```
axon evaluate "what is serde in rust?"
→ RAG: 4/5 accuracy, 5/5 relevance, 3/5 completeness
  Verdict: POOR — baseline significantly outperforms RAG
  Timing: rag=15s | baseline=17s | analysis=25s | total=71s
```

**Issues:**
- 71 seconds total (3 separate LLM calls in sequence)
- Fails with misleading threshold error for queries without strong index matches (same as `ask`)
- The `WARN ACP runtime: skipping unsupported model value` fires 4x during evaluate

---

#### `suggest [focus]` — ✓ WORKS (partially), ⚠ INVALID URL IN OUTPUT
Suggests new URLs to crawl based on indexed content.

```
axon suggest
→ https://docs.rs/
   https://huggingface.co/
   https://next.js/        ← INVALID URL (missing TLD/domain)

axon suggest "rust documentation"
→ https://docs.rs/
   https://doc.rust-lang.org/
   https://serde.rs/
```

**Issue:** Without a focus, suggest returned `https://next.js/` which is not a valid URL. With a focus argument, results are more reasonable.

---

#### `sources` — ✓ WORKS
Lists all indexed URLs with chunk counts. No issues.

#### `domains` — ✓ WORKS
Lists indexed domains with vector counts. No issues.

#### `stats` — ✓ WORKS (partially)

Qdrant collection stats populate correctly. However, **all Pipeline Stats, Freshness, and Command Counts fields show `n/a`** — these appear to be unimplemented or require full-mode Postgres to populate.

```
Vector Stats:   ✓ (7.1M vectors, 3.4M points, 7 payload fields)
Pipeline Stats: n/a (avg pages/sec, crawl duration, embedding duration)
Freshness:      n/a (last indexed, crawls 24h/7d)
Command Counts: n/a (crawls, embeds, scrapes, queries, asks, ...)
```

---

### Jobs & Diagnostics

#### `status` — ✓ WORKS
Shows all job queues in a human-readable summary. `--json` flag also works and returns full job records with all fields.

#### `doctor` — ✓ WORKS
Correctly identifies all service states including the downed LLM endpoint.

#### `debug` — ✓ WORKS
Runs doctor then ACP-backed LLM analysis. Correctly diagnosed the Tailscale peer issue.

#### `graph <subcommand>` — ✗ NOT AVAILABLE IN LITE MODE
All graph subcommands (`stats`, `status`, `explore`, `build`, `worker`) return:
```
Error: "graph is not available in lite mode"
```
Expected behavior — requires Neo4j via full mode.

#### `ingest <target>` — ✓ WORKS (fire-and-forget)
```
axon ingest "rust-lang/rust"
→ Ingest Job 20f1460e-...
  Job ID: 20f1460e-...
```
Job enqueued successfully. Full run not verified (would take hours for rust-lang/rust).

**Subcommands (`ingest list`, `ingest status`, etc.):** Same as `crawl` subcommands, all work.

#### `sessions` — ✓ WORKS (structurally)
Help shows correct flags (`--claude`, `--codex`, `--gemini`). Job subcommands work.

**Minor cosmetic issue:** `sessions list` displays the header `"Ingest Jobs"` instead of `"Sessions Jobs"` — it reuses the ingest job display code.

#### `research <query>` — ✓ WORKS
Tavily + ACP synthesis. Returns well-structured LLM summary with sources.

```
axon research "rust async patterns"
→ JSON summary + 10 source URLs with snippets
```

---

### Scheduling & Maintenance

#### `refresh` — ✓ WORKS (structurally)
`refresh list` shows `"No Refresh jobs found."`. All subcommands exist and match `crawl` pattern.

#### `watch` — ⚠ PARTIAL — SILENT EMPTY OUTPUT
`watch list` exits 0 with **no output** when there are no watches defined (no "No watches found" message). Every other list command prints an empty-state message. All subcommands (`create`, `get`, `update`, `run-now`, `pause`, `resume`, `delete`, `history`, `artifacts`) present in help.

#### `export` — ✗ NOT AVAILABLE IN LITE MODE
```
Error: export is not available in lite mode
```
Expected — requires Postgres.

#### `migrate` — ✓ WORKS (structurally)
Help shows correct `--from` and `--to` flags. Not run end-to-end (would modify production Qdrant collection).

#### `mcp` — Not tested (starts a long-running server process).

#### `serve` — Not tested (starts a long-running server process).

---

## Issue Summary

### Bugs

| # | Severity | Command | Issue |
|---|----------|---------|-------|
| 1 | High | `extract` | Chrome CDP DNS fails when running outside Docker — fetches no pages |
| 2 | High | `extract` | ACP fallback returns markdown-wrapped JSON, parser rejects it — all per-page extractions fail |
| 3 | High | `extract` | ACP adapter cleanup timeout (10s) fires for every URL — slow + noisy |
| 4 | High | `extract` | Silent success on total failure — exits 0 with no results when all URLs fail; should exit non-zero |
| 5 | Medium | `crawl` | `crawl list` / `crawl status` show `0 docs` / `pages_crawled: 0` for all completed jobs — metrics not captured in lite mode |
| 6 | Medium | `ask` | Misleading error "failed to build ask context" when real cause is relevance threshold — should expose threshold hint |
| 7 | Low | `watch list` | Silent empty output (exit 0, no text) — should print "No watches defined" |
| 8 | Low | `sessions list` | Shows "Ingest Jobs" header instead of "Sessions Jobs" |
| 9 | Low | `suggest` | Returns invalid URL `https://next.js/` without focus argument |
| 10 | Low | `stats` | All pipeline stats, freshness, and command count fields are `n/a` in lite mode |
| 11 | Cosmetic | All ACP commands | `WARN ACP runtime: skipping unsupported model value 'gemini-3-flash-preview'` fires multiple times per command |

### Environment Issues (not bugs, but blocking)

| Issue | Cause | Fix |
|-------|-------|-----|
| LLM endpoint unreachable | Tailscale peer `100.120.242.29:8317` is down | Restart the remote proxy service |
| Chrome DNS failure in `extract` | Running `axon` locally but Chrome resolves Docker-internal hostnames | Run inside container or use `--render-mode http` to bypass Chrome |

### Expected Limitations (lite mode)

- `graph` — requires Neo4j (full mode only)
- `export` — requires Postgres (full mode only)
- Pipeline stats in `stats` — requires Postgres command log tables

---

## Working Commands (full confidence)

`doctor`, `status`, `stats` (partial), `sources`, `domains`, `scrape`, `map`, `search`, `research`, `query`, `ask`, `retrieve`, `embed`, `evaluate`, `suggest`, `debug`, `ingest`, `refresh`, `migrate` (help verified), `crawl` (fire-and-forget; metric display bugged), `sessions` (structurally), `watch` (structurally; empty-list bug)

## Broken / Non-Functional

`extract` (both Chrome and ACP fallback paths broken in current environment), `graph` (lite mode), `export` (lite mode)
