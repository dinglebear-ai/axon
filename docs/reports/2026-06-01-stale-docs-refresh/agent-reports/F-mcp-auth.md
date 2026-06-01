# Agent F — MCP server + auth docs report

Ground truth: `src/mcp/schema.rs`, `src/mcp/schema/{requests,utility}.rs`, `src/mcp/auth.rs`,
`src/authz.rs`, `src/core/config/**` (MCP env + defaults), `src/web/**` (routes/panel auth),
`Justfile`, `Cargo.toml`, and the v4.16.0 CLI help dumps
(`ground-truth/axon-mcp--help.txt`, `axon-serve--help.txt`).

## Files reviewed

- docs/MCP-TOOL-SCHEMA.md — minor fixes (regenerated; was current except date)
- docs/MCP.md — accurate
- docs/commands/mcp.md — accurate
- docs/mcp/CLAUDE.md — accurate (one stale cross-ref phrase, noted below; out of lane to fix)
- docs/mcp/CONNECT.md — accurate
- docs/mcp/DEPLOY.md — accurate
- docs/mcp/DEV.md — minor fixes
- docs/mcp/ENV.md — accurate
- docs/mcp/PATTERNS.md — accurate
- docs/mcp/TOOLS.md — major fixes
- docs/mcp/TRANSPORT.md — accurate
- docs/auth/MCP-AUTH.md — minor fixes
- docs/auth/API-TOKEN.md — accurate

## Fixes made

**docs/MCP-TOOL-SCHEMA.md** — Regenerated via `scripts/generate_mcp_schema_doc.py`
(the canonical generator). Only the `Last Modified` date changed (2026-05-23 → 2026-06-01);
the body was already faithful to `src/mcp/schema.rs`. Confirms the doc is the live wire
contract (32 actions; single `axon` tool; `action`+`subaction`).

**docs/mcp/TOOLS.md** (verified against `src/mcp/schema/{requests,utility}.rs` and
`src/core/config/types/config_impls.rs:55`):
- Default collection was wrong in 4 places: `[search].collection` default `cortex` → **`axon`**
  (code: `collection: "axon"`). Fixed all four (`query`, `ask`, `evaluate`, `suggest`).
- `ask` action was missing the `explain` field (`AskRequest.explain: Option<bool>` — returns a
  per-candidate explain trace and skips synthesis). Added.
- `ingest` source-type list was incomplete (`github, reddit, youtube, sessions`) → added
  `gitlab, gitea, git` to match `IngestSourceType` (7 variants).
- `scrape` `format` enum showed CLI spelling `rawHtml` and omitted `llm`; MCP serializes
  `McpScrapeFormat` snake_case → corrected to `markdown, html, raw_html, json, llm`.
- Added a "More direct actions" table documenting actions the file previously omitted but the
  schema exposes: `brand`, `diff`, `endpoints`, `debug`, `dedupe`, `migrate`, `watch`, `setup`,
  `vertical_scrape` (noting `vertical_scrape` is discovery-only — `run` removed). Reworded the
  inaccurate "50+ operations" framing to point at the generated schema as the exhaustive source.

**docs/mcp/DEV.md**:
- `cd axon_rust` → `cd axon` (repo renamed; clone URL is already `jmagar/axon`).
- `RUST_LOG=info,axon::axon::mcp=debug` → `axon::mcp` (crate is `axon` per `Cargo.toml`;
  the doubled segment was a typo). Verified `just gen-mcp-schema`, `just mcp-smoke`,
  `SqliteServiceRuntime`, and the source-tree layout are all still correct.

**docs/auth/MCP-AUTH.md**:
- Scope description said "`axon:write` satisfies read." Refined to match `scope_satisfies`
  in `src/authz.rs`: **either** Axon scope (`axon:read` or `axon:write`) satisfies **any**
  Axon-scoped action; `migrate`/`dedupe` also require an Axon scope; unknown actions fail closed.
- Bumped `Last Modified` to 2026-06-01.

## Notable NON-findings (verified current, did not change)

- The OpenAI-compatible LLM backend (`AXON_LLM_BACKEND=openai-compat`, `AXON_OPENAI_BASE_URL`,
  `AXON_OPENAI_MODEL`, `AXON_OPENAI_API_KEY`) in MCP.md/ENV.md/PATTERNS.md is **current** —
  wired and tested in `src/core/config/parse/build_config/config_literal.rs` +
  `tests/env_required.rs`. The brief's "OpenAI path removed" note refers only to the legacy
  *unprefixed* `OPENAI_*` vars (removed 3.0.0), which the docs already mark as removed.
- MCP `max_depth` default `10` (TOOLS.md, MCP-TOOL-SCHEMA.md) is **correct** — both
  `Config::default()` and the `--max-depth` clap default are `10`. (The root `CLAUDE.md`
  table claiming `5` is wrong, but that's Agent B's lane.)
- Transport defaults verified: `axon mcp` → stdio, `axon serve mcp` → http, bare `axon serve`
  → both, host `127.0.0.1`, port `8001` (`command_dispatch.rs`, `config_impls.rs`).
- Auth env-var inventory in ENV.md/MCP-AUTH.md/API-TOKEN.md exactly matches the `AXON_MCP_*`
  set in code; loopback-tokenless rule, constant-time compare, x-api-key normalization, and
  the startup-refusal error string all match `src/mcp/auth.rs`.
- Web panel password auth (API-TOKEN.md) verified against `src/web/`: `~/.axon/panel-password`,
  `x-axon-panel-token` header, `/api/panel/{config,ops,setup/targets,login}` routes all exist.

## Gaps / missing docs (for Phase 2)

- No per-command CLI doc for several commands the MCP tool exposes as actions:
  `docs/commands/` lacks `brand.md`, `diff.md`, `endpoints.md`, `dedupe.md`, `migrate.md`,
  `watch.md`, `screenshot.md`, `debug.md` (verify against Agent B's lane). TOOLS.md now
  references these actions but there is no command-level reference for them.
- `vertical_scrape` discovery flow (list → scrape) is documented in `src/mcp/CLAUDE.md` and
  now briefly in TOOLS.md, but there is no user-facing doc explaining the extractor catalog
  and which URL patterns auto-route. Could live in `docs/mcp/` or alongside an extract doc.
- No doc covers the MCP `endpoints` action's RPC-probing knobs (`probe_rpc`,
  `probe_rpc_subdomains`) beyond the field list; MCP.md has a short example only.

## Reorg observations (for Phase 2)

- **Heavy overlap** between top-level `docs/MCP.md` and the `docs/mcp/` family
  (TRANSPORT/CONNECT/ENV/TOOLS). MCP.md duplicates transport modes, auth summary, env tables,
  action lists, and the artifact/response-mode model that also live (often more precisely) in
  `docs/mcp/*`. Candidate for consolidation: make `docs/MCP.md` a thin runtime/design overview
  that links into `docs/mcp/`, or fold it in entirely.
- `docs/MCP-TOOL-SCHEMA.md` is auto-generated and should stay the single source of truth; the
  hand-written action tables in MCP.md and TOOLS.md drift from it (this refresh re-synced them).
  Consider having TOOLS.md's tables generated too, or clearly scoping it as "common actions only."
- `docs/auth/` (MCP-AUTH.md, API-TOKEN.md) and the MCP transport docs cover auth from two
  angles with some duplication (loopback rule, header acceptance, constant-time compare appear
  in TRANSPORT.md, MCP-AUTH.md, API-TOKEN.md, and MCP.md). One canonical auth page would help.

## Cross-reference notes

Links FROM my docs that the reorg phase must keep valid:
- docs/mcp/CLAUDE.md → `../CONFIG.md`, `../stack/ARCH.md`, `../repo/REPO.md`, `../MCP.md`,
  `../MCP-TOOL-SCHEMA.md`. **Stale phrase:** it calls `../stack/ARCH.md` the "trimodal
  architecture overview" — trimodal/lite/full runtime was removed (SQLite-only now). The link
  target is in another lane; flagged for the owner of `docs/stack/`.
- docs/mcp/* cross-link heavily to each other (TOOLS↔ENV↔TRANSPORT↔CONNECT↔DEPLOY↔PATTERNS↔DEV)
  and to `../MCP-TOOL-SCHEMA.md`, `../auth/MCP-AUTH.md`, `../repo/RECIPES.md`, `../stack/ARCH.md`.
- docs/auth/API-TOKEN.md → docs/auth/MCP-AUTH.md (intra-section anchors).
- Code→doc references confirmed accurate: `src/mcp/CLAUDE.md` and `src/mcp/schema.rs` point to
  `docs/MCP-TOOL-SCHEMA.md` and `docs/MCP.md` as source-of-truth; the generator is
  `scripts/generate_mcp_schema_doc.py` (Justfile recipe `gen-mcp-schema`).
