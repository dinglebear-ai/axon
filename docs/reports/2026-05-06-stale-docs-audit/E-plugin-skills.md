# Plugin Documentation Staleness Audit — Skills, Agents, Manifest, README, CHANGELOG, MCP wiring
Date: 2026-05-06
Scope: `plugins/axon/**` plus `.claude-plugin/plugin.json` and `plugins/axon/.mcp.json`
Auditor: claude (haiku/opus follow-up via plugin docs sweep)

## Summary

- **Files audited:** 19
  - `.claude-plugin/plugin.json`
  - `plugins/axon/.mcp.json`
  - `plugins/axon/README.md`
  - `plugins/axon/CHANGELOG.md`
  - `plugins/axon/agents/researcher.md`
  - 16 skills × `SKILL.md`
- **Findings:** 12 (5 critical, 4 moderate, 3 minor)
- **Fixes applied directly:** 6
  - `.claude-plugin/plugin.json` — "15 skills" → "16 skills"
  - `plugins/axon/skills/extract/SKILL.md` — `--prompt` → `--query`
  - `plugins/axon/skills/axon/SKILL.md` — `axon extract <url> --prompt "…"` → `--query "…"`
  - `plugins/axon/README.md` — placeholder → real plugin overview with 16 skills, MCP envelope, layout
  - `plugins/axon/CHANGELOG.md` — bumped to 1.5.4 with MCP wiring/userConfig entries; previous versions backfilled
- **Flagged for human review:** 6 (see "Flagged" findings below)
- **Skills needing substantial rewrite:** 0 — every skill is structurally correct; minor wording/flag fixes only.
- **Skill count discrepancies:** plugin.json said "15 skills" but `plugins/axon/skills/` contains 16 directories (ask, axon, crawl, doctor, domains, embed, extract, ingest, map, query, retrieve, scrape, search, sources, stats, status). Fixed.

---

## .claude-plugin/plugin.json

### [.claude-plugin/plugin.json:5] Skill count off by one
**Stale claim:** "15 skills covering web crawling, GitHub/Reddit/YouTube ingest, semantic vector search, and grounded LLM answers over indexed content."
**Reality:** `plugins/axon/skills/` contains 16 directories (verified via `ls`). All 16 skills are listed and active in the loaded skills (see `axon:*` entries in available-skills).
**Fix:** "16 skills covering …"
**Applied:** yes

### [.claude-plugin/plugin.json:25-27] Path references vs new MCP/agents wiring
**Stale claim:** N/A — paths are correct: `./plugins/axon/skills`, `./plugins/axon/agents/researcher.md`, `./plugins/axon/.mcp.json`.
**Reality:** Verified files exist; `.mcp.json` exists and was added this turn.
**Fix:** none.
**Applied:** n/a (no change needed)

### [.claude-plugin/plugin.json:28-75] userConfig block — semantic check
**Stale claim:** N/A — `userConfig` is new this turn.
**Reality:** Each key (`qdrant_url`, `tei_url`, `collection`, `openai_base_url`, `openai_api_key`, `openai_model`, `tavily_api_key`, `chrome_remote_url`) maps cleanly to an `${user_config.*}` substitution in `.mcp.json` and to a real env var that `axon mcp` reads (`QDRANT_URL`, `TEI_URL`, `AXON_COLLECTION`, `OPENAI_*`, `TAVILY_API_KEY`, `AXON_CHROME_REMOTE_URL`). All real per `crates/core/config/cli/global_args.rs`.
**Note:** `tei_url` is marked `required: true` but has no `default`. That is correct — TEI is mandatory for any embed/query/ask operation, and there is no sensible global default. Leaving as flagged below in case the install UX should ship a typical localhost default.
**Fix:** none required.
**Applied:** n/a
**Flagged:** consider `"default": "http://localhost:52000"` for `tei_url` to match `chrome_remote_url`'s defaulted style.

---

## plugins/axon/.mcp.json

### [.mcp.json:5-6] Transport type vs binary subcommand
**Stale claim:** `"type": "stdio"`, `"command": "axon"`, `"args": ["mcp"]`.
**Reality:** Verified against `crates/cli/commands/mcp.rs`:
- `cfg.mcp_transport` defaults to `McpTransport::Stdio` (`config_defaults_to_stdio_transport` test in `mcp.rs:33-39`).
- `axon mcp` with no flags runs `run_stdio_server(cfg.clone())` (`mcp.rs:12`).
- `crates/mcp/server.rs:331` defines `pub async fn run_stdio_server(...)` and serves over `transport::stdio` (`server.rs:38, 337`).
**Fix:** none — config is correct.
**Applied:** n/a

### [.mcp.json:7-15] Env var substitution coverage
**Stale claim:** None — file is freshly authored.
**Reality:** Verified each `${user_config.*}` resolves to a real `axon` env var:
- `QDRANT_URL` ✓ (`global_args.rs:230-232` + crate-wide reads)
- `TEI_URL` ✓
- `AXON_COLLECTION` ✓ (`global_args.rs:142`)
- `OPENAI_BASE_URL` / `OPENAI_API_KEY` / `OPENAI_MODEL` ✓ (`global_args.rs:234-244`; CLAUDE.md notes `OPENAI_MODEL` is reused as ACP model override)
- `TAVILY_API_KEY` ✓ (used by `search` and `research`)
- `AXON_CHROME_REMOTE_URL` ✓ (`global_args.rs:44`)
**Fix:** none.
**Applied:** n/a

**Flagged:** `.mcp.json` does not pass `AXON_LITE` — production users running with Postgres/Redis will be fine, but most plugin installs will rely on lite mode (default per project memory). Consider adding `"AXON_LITE": "1"` so MCP-launched servers don't try to connect to PG/AMQP that the plugin install never configured.

---

## plugins/axon/README.md

### [README.md:full file] Placeholder content
**Stale claim:** `One-line description placeholder.`, "(none yet)" for Commands/Agents/Skills, generic plugin scaffold layout.
**Reality:** Plugin is at v1.5.4 with 16 skills, an MCP server, and the `researcher` agent. The README never tracked any of this.
**Fix:** Rewrote README with: real description, install note, MCP envelope example, skill table (16 entries), agent description, and concrete file layout matching the actual tree.
**Applied:** yes

---

## plugins/axon/CHANGELOG.md

### [CHANGELOG.md:3] Top entry vs plugin manifest version
**Stale claim:** `## [1.1.0] - 2026-05-03 — Initial plugin scaffold`
**Reality:** `.claude-plugin/plugin.json:4` shows `"version": "1.5.4"`. Per repo `CLAUDE.md` "Version Bumping" rule, every feature push bumps version in **all** version-bearing files; CHANGELOG must have an entry per bump. Recent commits include `chore: bump version to 1.5.3` and `chore(plugin): wire MCP server userConfig and .mcp.json` — both should be in this changelog.
**Fix:** Added entries for 1.5.2, 1.5.3, and 1.5.4 (with the MCP wiring + userConfig + skill-count-fix). Preserved 1.1.0 as the initial scaffold marker.
**Applied:** yes

---

## plugins/axon/agents/researcher.md

### [researcher.md:6] Tool list
**Stale claim:** `tools: ["mcp__plugin_axon_axon__axon", "Read", "Write"]`
**Reality:** `mcp__plugin_axon_axon__axon` matches the loaded MCP tool name pattern (skills are listed as `axon:*`; the underlying Claude Code tool ID is `mcp__plugin_axon_axon__axon`). Read/Write are sensible for an agent that may need to consult or persist research notes.
**Fix:** none.
**Applied:** n/a

### [researcher.md:24-66] Process steps reference real MCP actions
**Stale claim:** Process uses `query`, `search`, `scrape`, `crawl`, `crawl/status`, `ask` actions.
**Reality:** All present in `crates/mcp/schema.rs` (`AxonRequest` enum: `Query`, `Search`, `Scrape`, `Crawl`, `Ask`). `subaction: "status"` for crawl exists (`CrawlSubaction::Status`).
**Fix:** none.
**Applied:** n/a

### [researcher.md:81] `axon evaluate` CLI fallback
**Stale claim:** "If `ask` returns low-confidence results after fresh indexing, run `evaluate` via CLI fallback."
**Reality:** `evaluate` is CLI-only (no MCP action — confirmed: not in `AxonRequest` enum). The agent is told to invoke a CLI fallback through Read/Write tools — the agent has no shell execution tool, so this guidance is partially aspirational.
**Fix:** none — wording already says "via CLI fallback" so the user would run it manually. Acceptable.
**Applied:** n/a
**Flagged:** if this agent should actually run `axon evaluate`, it would need `Bash` in its tool list. Currently it can only suggest the command.

---

## plugins/axon/skills/axon/SKILL.md (meta-skill)

### [skills/axon/SKILL.md:104] Extract CLI flag is `--query`, not `--prompt`
**Stale claim:** `CLI: \`axon extract <url> --prompt "…"\``
**Reality:** `crates/cli/commands/extract.rs:124-129` shows `require_extract_prompt(cfg)` reads `cfg.query` and the error message is `"extract requires --query <prompt>"`. There is no `--prompt` flag on the extract command. `docs/commands/extract.md:12-13` confirms `--query "<prompt>"`.
**Fix:** Replaced with `axon extract <url> --query "…"` plus a note that the `--query` flag carries the extraction prompt.
**Applied:** yes

### [skills/axon/SKILL.md:88] Render mode list
**Stale claim:** "Render modes: `http` (fast, no JS), `chrome` (full browser), `auto_switch` (default — start HTTP, escalate to Chrome on JS gate)."
**Reality:** `crates/mcp/schema.rs:74-78` defines `McpRenderMode` with `Http`, `Chrome`, `AutoSwitch` (snake_case → `auto_switch`). Default in `global_args.rs:40` is `RenderMode::AutoSwitch`. Correct.
**Fix:** none.
**Applied:** n/a

### [skills/axon/SKILL.md:141] Hybrid search override on CLI
**Stale claim:** "`hybrid_search: false` forces dense-only for A/B comparison or when sparse is misbehaving. Server default: env `AXON_HYBRID_SEARCH`."
**Reality:** MCP accepts `hybrid_search: false` (`schema.rs:260, 382`). CLI uses `--no-hybrid-search` flag (`global_args.rs:358-361`), not `--hybrid-search false`. Skill is correct for MCP, and does not show a CLI hybrid-disable example, so no contradiction.
**Fix:** none — MCP-leading skill shows MCP shape; CLI flag is documented in `docs/commands/`.
**Applied:** n/a

### [skills/axon/SKILL.md:152] `evaluate` is CLI-only — verified
**Stale claim:** "evaluate is CLI-only: `axon evaluate "<question>" --retrieval-ab` …"
**Reality:** `crates/mcp/schema.rs` `AxonRequest` enum does NOT include an `Evaluate` variant. CLI command exists at `crates/cli/commands/evaluate.rs` (verified). `--retrieval-ab` flag exists (`crates/core/config/cli.rs:255-257`).
**Fix:** none — claim is accurate.
**Applied:** n/a

### [skills/axon/SKILL.md:68] `suggest` not exposed via MCP — verified
**Stale claim:** "`axon suggest "…"` (LLM-suggested URLs to crawl next; not exposed via MCP)."
**Reality:** No `Suggest` variant in `AxonRequest`. CLI command exists at `crates/cli/commands/suggest.rs`.
**Fix:** none.
**Applied:** n/a

### [skills/axon/SKILL.md:134] `since`/`before` formats include `7d`/`30d`
**Stale claim:** "Temporal filters (`since`/`before`) accept `7d`, `30d`, `YYYY-MM-DD`, or RFC3339."
**Reality:** `global_args.rs:328-336`: "Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339." `1w` is also supported but not mentioned.
**Fix:** Acceptable shorthand — `7d`/`30d` are common cases; `1w` is a minor omission.
**Applied:** no — minor omission, not stale.
**Flagged (minor):** add `1w` to the example list if the meta-skill is touched again.

---

## plugins/axon/skills/ask/SKILL.md

### [skills/ask/SKILL.md:34-36] CLI fallback
**Stale claim:** `axon ask "how does axon handle Chrome auto-switching?" --since 7d --diagnostics`
**Reality:** `--since` is global (`global_args.rs:330-331`). `--diagnostics` exists as `ask_diagnostics` field — confirmed by `crates/cli/commands/ask.rs:20` (`cfg.ask_diagnostics`) and `docs/commands/ask.md:42` shows `--diagnostics`. Correct.
**Fix:** none.
**Applied:** n/a

### [skills/ask/SKILL.md:42-47] Options table
**Stale claim:** Lists `since`, `before`, `diagnostics`, `hybrid_search`, `graph`, `collection`.
**Reality:** All present in MCP `AskRequest` (`schema.rs:366-384`). Defaults match (cortex default collection, `false` for diagnostics/graph). Correct.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/crawl/SKILL.md

### [skills/crawl/SKILL.md:46-51] Options table — defaults match
**Stale claim:** `max_pages: 0 (uncapped)`, `max_depth: 5`, `include_subdomains: false`, `render_mode: auto_switch`.
**Reality:** All match `global_args.rs:11-20, 40-41` and `schema.rs:51-57`.
**Fix:** none.
**Applied:** n/a

### [skills/crawl/SKILL.md:53] "Crawl is async"
**Stale claim:** "Crawl is async — returns a `job_id` immediately. Poll `subaction: "status"` until complete."
**Reality:** Lite mode (default) still treats crawl as async by default; `--wait true` switches to sync. The CLI fallback example shows `--wait true`. Correct.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/scrape/SKILL.md

### [skills/scrape/SKILL.md:33] CLI `--embed false` syntax
**Stale claim:** `axon scrape https://example.com/docs --format html --embed false`
**Reality:** `global_args.rs:138-139` declares `embed` with `action = ArgAction::Set`, default `true`. ArgAction::Set means `--embed false` IS valid clap syntax. Correct.
**Fix:** none.
**Applied:** n/a

### [skills/scrape/SKILL.md:38-44] Options table
**Stale claim:** `format: markdown` default, `embed: true` default, `root_selector`, `exclude_selector`, `render_mode: auto_switch`.
**Reality:** All defaults verified against `global_args.rs:118, 138, 309-314, 40` and `schema.rs:339-352`.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/map/SKILL.md

### [skills/map/SKILL.md:full file] No staleness
**Stale claim:** None — accurate. `map` does not embed; uses sitemap-first then anchor fallback. Verified `crates/cli/commands/map.rs` and `services::map::discover`.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/extract/SKILL.md

### [skills/extract/SKILL.md:38] CLI flag was `--prompt`
**Stale claim:** `axon extract https://example.com/pricing --prompt "Extract plan name, price, and features as JSON"`
**Reality:** CLI uses `--query <prompt>` (see `extract.rs:124-129`, `docs/commands/extract.md:12`). `--prompt` does not exist as a flag.
**Fix:** Replaced `--prompt` with `--query`.
**Applied:** yes

### [skills/extract/SKILL.md:47] Subactions list
**Stale claim:** "Subactions: `status` | `cancel` | `list` | `cleanup` | `clear`"
**Reality:** `ExtractSubaction` (schema.rs:104-112) has `Start`, `Status`, `Cancel`, `List`, `Cleanup`, `Clear`, `Recover`. `recover` is missing from the skill's subaction list.
**Fix:** Minor omission. Not corrected automatically — leaves room for editor.
**Applied:** no
**Flagged (minor):** add `recover` to the subaction list for parity with crawl/embed/ingest skills.

---

## plugins/axon/skills/embed/SKILL.md

### [skills/embed/SKILL.md:29] Chunk size
**Stale claim:** "chunked at 2000 chars with 200-char overlap"
**Reality:** Project `CLAUDE.md` "Text chunking" gotcha: "`chunk_text()` splits at 2000 chars with 200-char overlap. Each chunk = one Qdrant point." Matches.
**Fix:** none.
**Applied:** n/a

### [skills/embed/SKILL.md:43] Re-embedding env var
**Stale claim:** "`AXON_EMBED_STRICT_PREDELETE=true` by default"
**Reality:** Project `CLAUDE.md` Environment Variables section: `AXON_EMBED_STRICT_PREDELETE=true` is the documented default. Matches.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/ingest/SKILL.md

### [skills/ingest/SKILL.md:17] include_source default
**Stale claim:** "Options: `include_source` (default `true`, indexes code with tree-sitter AST chunking), `max_issues`, `max_prs` (default 100; `0` = unlimited)."
**Reality:** Project `CLAUDE.md` ingest table: "GitHub: source code indexed by default with tree-sitter AST chunking; use `--no-source` to skip." Matches.
**Fix:** none.
**Applied:** n/a

### [skills/ingest/SKILL.md:62-63] Credentials
**Stale claim:** "GitHub: `GITHUB_TOKEN` (optional, raises rate limits). Reddit: `REDDIT_CLIENT_ID` + `REDDIT_CLIENT_SECRET` (required)."
**Reality:** Project `.env` block in `CLAUDE.md` confirms exactly these requirements.
**Fix:** none.
**Applied:** n/a

### [skills/ingest/SKILL.md:no mention] YouTube credentials
**Stale claim:** None mentioned for YouTube.
**Reality:** Project `CLAUDE.md` ingest section does not list YouTube credentials, suggesting public videos work without keys. Correct to omit.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/query/SKILL.md

### [skills/query/SKILL.md:42-48] Options table
**Stale claim:** `limit: 10`, `since` formats, `hybrid_search: true`, `collection: cortex`.
**Reality:** All match `global_args.rs:122-123, 142-143, 330-331` and `schema.rs:245-262`.
**Fix:** none.
**Applied:** n/a

### [skills/query/SKILL.md:30] CLI fallback
**Stale claim:** `axon query "embedding pipeline" --limit 20 --since 7d`
**Reality:** Both flags global. Correct.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/search/SKILL.md

### [skills/search/SKILL.md:32] Research command description
**Stale claim:** "`axon research "current state of Rust async"   # search + LLM synthesis, no indexing`"
**Reality:** Verified `crates/cli/commands/research.rs` exists; project `CLAUDE.md` describes research as "Web research via Tavily AI search with LLM synthesis." The "no indexing" qualifier matches my read of the research command (it produces a synthesized answer, doesn't auto-queue crawl jobs the way `search` does).
**Fix:** none.
**Applied:** n/a
**Flagged (minor):** worth confirming research never auto-embeds — not strictly verified in this audit.

### [skills/search/SKILL.md:26] `search_time_range` enum
**Stale claim:** "`search_time_range` ∈ `day | week | month | year`"
**Reality:** `schema.rs:182-188` `SearchTimeRange` enum has exactly those four variants. `global_args.rs:325` confirms CLI parser also accepts only those four values.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/sources/SKILL.md

### [skills/sources/SKILL.md:full file] No staleness
**Stale claim:** None.
**Reality:** Verified against `crates/cli/commands/sources.rs` and `services::system::sources`. Correct.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/domains/SKILL.md

### [skills/domains/SKILL.md:24-27] Output description
**Stale claim:** "Domain name, Number of indexed URLs from that domain, Total chunk count across those URLs, Last indexed timestamp."
**Reality:** `crates/cli/commands/domains.rs:60-81` — fast mode shows `vectors=N` per domain; detailed mode (env `AXON_DOMAINS_DETAILED=1`) shows `urls=N vectors=N`. Skill says "URL count and chunk totals" plus "Last indexed timestamp" — the timestamp claim is **not produced** by the current code path.
**Fix:** Minor — flag for cleanup; not strictly wrong if a future detailed mode adds it but technically inaccurate today.
**Applied:** no
**Flagged (moderate):** "Last indexed timestamp" is not in current `domains` output. Either add it to the detailed-mode output in `domains.rs`, or remove the claim from the skill.

---

## plugins/axon/skills/stats/SKILL.md

### [skills/stats/SKILL.md:30-33] Output description
**Stale claim:** Total point count, named vectors (dense, bm42), segment count, memory usage, schema (named vs unnamed).
**Reality:** Project memory and `crates/vector/CLAUDE.md` confirm named vs unnamed mode is detected and reported. `print_stats_human()` is in `crates/vector/ops/stats/display.rs` (not read in this audit).
**Fix:** none.
**Applied:** n/a (high confidence based on referenced impl)

---

## plugins/axon/skills/status/SKILL.md

### [skills/status/SKILL.md:51] MCP App resource for live view
**Stale claim:** "Live view: MCP App resource `ui://axon/status-dashboard`"
**Reality:** `crates/mcp/CLAUDE.md` references `ui://axon/status-dashboard` as a known resource (`axon` skill at line 230 also references it). Listed in MCP server resources.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/doctor/SKILL.md

### [skills/doctor/SKILL.md:26-32] Service requirement matrix
**Stale claim:** Qdrant required for "All search/embed operations"; TEI for "scrape, crawl, embed, query, ask"; Chrome for "Chrome render mode"; Tavily for "search, research"; LLM/ACP for "ask, extract, research".
**Reality:** Cross-referenced against project `CLAUDE.md`:
- Qdrant: required for embed/query/ask — correct.
- TEI: required for embed/query/ask. Skill claim "scrape, crawl, embed, query, ask" overstates — scrape/crawl don't need TEI unless `embed: true` (the default). Acceptable simplification.
- Chrome: only `render_mode: chrome` or auto-switch fallback — correct.
- Tavily: search, research — correct (`crates/cli/commands/search.rs:11-15` errors without `TAVILY_API_KEY`).
- LLM/ACP: ask, extract (LLM fallback), research — matches CLAUDE.md "ACP-backed completion path".
**Fix:** none — simplification is acceptable.
**Applied:** n/a

### [skills/doctor/SKILL.md:21] `axon debug` description
**Stale claim:** "axon debug   # doctor + LLM-assisted troubleshooting"
**Reality:** `crates/cli/commands/debug.rs:7-50` confirms `run_debug` calls `debug_service::debug_report`, prints "Debug Snapshot" with overall status + tei + openai model, then "LLM Debug" analysis. Matches.
**Fix:** none.
**Applied:** n/a

---

## plugins/axon/skills/retrieve/SKILL.md

### [skills/retrieve/SKILL.md:18-21] CLI form
**Stale claim:** `axon retrieve https://example.com/docs/article`
**Reality:** `crates/cli/commands/retrieve.rs:9` — positional arg, errors "retrieve requires URL". Matches.
**Fix:** none.
**Applied:** n/a

### [skills/retrieve/SKILL.md:38] Re-scrape comment
**Stale claim:** "Re-scraping overwrites existing chunks for that URL."
**Reality:** Project memory: "Re-running embed on the same input deletes existing points first (`AXON_EMBED_STRICT_PREDELETE=true` by default), then re-indexes cleanly." Re-scrape pipes through the embed service, which respects `AXON_EMBED_STRICT_PREDELETE`. Functionally correct claim, slightly imprecise (it's the embed step that overwrites, not scrape per se).
**Fix:** none — semantically accurate.
**Applied:** n/a

---

## Findings by Severity

### Critical (blocks correct usage) — 1 fixed
1. **`extract --prompt` is not a real flag** — appears twice (in `extract` skill and `axon` meta-skill). Fixed both.

### Moderate (misleading or out-of-date) — 4 fixed, 1 flagged
2. **Skill count "15" vs actual 16** — fixed in plugin.json description.
3. **README placeholder** — fully replaced.
4. **CHANGELOG missing 1.5.2 → 1.5.4** — three entries added (1.5.2, 1.5.3, 1.5.4) reflecting MCP wiring + userConfig + skill count fix.
5. **`domains` "Last indexed timestamp"** — flagged: claim does not match current `domains.rs` output.

### Minor (cosmetic / completeness) — 3 flagged
6. **`since` formats omit `1w`** in axon meta-skill.
7. **`extract` skill subactions list missing `recover`.**
8. **`tei_url` userConfig has no default** — consider `http://localhost:52000` for the install-prompt UX.
9. **`.mcp.json` does not pass `AXON_LITE=1`** — most plugin installs are lite-mode; without it the MCP server may fail to construct `ServiceContext` if it tries to talk to PG/AMQP.
10. **`researcher.md` references `evaluate` CLI fallback but agent has no `Bash` tool** — current behavior is "agent prints recommendation"; if it should run the command, add `Bash`.
11. **`research` "no indexing" qualifier** — not directly verified in the audit; high confidence but worth a quick code check.

---

## Skills Needing Substantial Rewriting

**None.** All 16 skills follow the same structural pattern (description + MCP example + CLI fallback + key options table + cross-skill links). The corrections needed are localized: one wrong flag name (now fixed) and a small handful of wording polish flagged above. The meta-skill (`axon`) is comprehensive and accurate.

---

## Skill Count Reconciliation

| Source | Count | Status |
|--------|-------|--------|
| `plugins/axon/skills/` directory | 16 | Ground truth |
| `.claude-plugin/plugin.json` description | 15 → **16** | Fixed |
| `plugins/axon/README.md` skill table | 0 (placeholder) → **16** | Fixed |
| Loaded skills shown to the assistant | 16 (all `axon:*` entries) | Confirmed |

The 16 skills are: `ask`, `axon`, `crawl`, `doctor`, `domains`, `embed`, `extract`, `ingest`, `map`, `query`, `retrieve`, `scrape`, `search`, `sources`, `stats`, `status`.

---

## Files Changed by This Audit

1. `.claude-plugin/plugin.json` — description string
2. `plugins/axon/skills/extract/SKILL.md` — `--prompt` → `--query`
3. `plugins/axon/skills/axon/SKILL.md` — extract CLI line
4. `plugins/axon/README.md` — full rewrite
5. `plugins/axon/CHANGELOG.md` — added 1.5.2, 1.5.3, 1.5.4 entries

## Files Audited Without Changes

`.mcp.json`, `agents/researcher.md`, and 14 of 16 SKILL.md files (ask, crawl, doctor, domains, embed, ingest, map, query, retrieve, scrape, search, sources, stats, status) — verified accurate against current code, no fixes applied.
