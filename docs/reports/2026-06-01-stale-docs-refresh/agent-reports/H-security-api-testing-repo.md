# Agent H — security, HTTP API/web, testing, completions, repo-meta report

Date: 2026-06-01. Verified against current source at worktree
`/home/jmagar/workspace/axon/.worktrees/docs-refresh` (Cargo.toml v4.16.0).

## Files reviewed

- docs/SECURITY.md — **major fixes** (network-exposure section was factually wrong)
- docs/GUARDRAILS.md — minor fixes (firewall + ssrf path)
- docs/SPIDER-FEATURE-FLAGS.md — **major fixes** (firewall mis-stated as enabled; counts wrong)
- docs/API.md — minor fixes (missing routes)
- docs/API-PARITY.md — minor fixes (missing CLI rows, ask/stream contradiction)
- docs/ENDPOINTS.md — minor fixes (missing `--probe-rpc-subdomains`, ACP)
- docs/TESTING.md — minor fixes (stale paths, removed AMQP resolver, verify recipe)
- docs/LIVE-TEST-SCRIPTS.md — accurate (but **misnamed** — content is Monolith Policy; see reorg)
- docs/SHELL-COMPLETIONS.md — accurate, no changes
- docs/DESKTOP-PALETTE-TESTING.md — minor fix (path); references a **missing** script/binary (see gaps)
- docs/repo/CLAUDE.md — accurate, no changes (all cross-refs valid)
- docs/repo/MEMORY.md — accurate, no changes
- docs/repo/RECIPES.md — minor fix (verify recipe steps)
- docs/repo/REPO.md — minor fixes (repo root name, xtask, migrations note)
- docs/repo/RULES.md — minor fixes (version-bearing files, xtask hooks)
- docs/repo/SCRIPTS.md — minor fix (xtask check set)
- docs/specs/android-redesign.md — accurate point-in-time; no rewrite (archival candidate)
- docs/production-readiness-sprint-report-2026-05-12.md — accurate historical report; no rewrite (archival candidate)

## Fixes made

### SECURITY.md (two factual security bugs)
- **§8 Network Exposure was wrong.** It claimed Qdrant/TEI/Chrome/axon ports are
  loopback-bound and that "the `127.0.0.1:` prefix on every Chrome port mapping is
  intentional security posture." Ground truth (`docker-compose.prod.yaml`): every
  mapping is **bare** (`53333:6333`/`53334:6334`, `${TEI_HTTP_PORT:-52000}:80`,
  `9222:9222`/`9223:9223`/`6000:6000`, `${AXON_MCP_HTTP_PUBLISH:-8001}:8001`) —
  published on `0.0.0.0`, no loopback prefix (some via env templates). Worse,
  `scripts/check_compose_port_bindings.py` **forbids** the
  `127.0.0.1:` prefix. Rewrote the table + hardening guidance: the real boundary is
  the in-container process bind (`AXON_MCP_HTTP_HOST` default `127.0.0.1`) plus a
  host firewall / private Docker network, NOT the compose mapping. Removed the
  "add a `127.0.0.1:` prefix" advice (would fail the repo lint).
- **§6 panel route list was stale.** Listed only `state, login, config, ops,
  setup/targets`. Source (`src/web/server/routing.rs`, `src/web/CLAUDE.md`) also
  has `env, status, doctor, command, stack, first-run/crawl, first-run/ask`. Added
  them and clarified `/api/panel/command` is a fixed `ask`/action command runner
  (`handlers/config.rs:179`, `parse_panel_command`), not a raw shell.
- Updated Last Modified → 2026-06-01. (§2-§5, §7 verified accurate, incl. the
  `[ask.cache]`/RLIMIT_CORE claim — `enforce_core_dump_disabled_for_ask_cache`.)

### SPIDER-FEATURE-FLAGS.md (headline accuracy bug)
- Doc asserted `firewall` ENABLED in ≥4 places; Cargo.toml does **not** list it
  (it's explicitly excluded with a comment block). Flipped all: removed from the
  inline Cargo.toml block, Firewall inventory row (✅→—), Summary row, and the
  header. Recounted from Cargo.toml: **18 spider features** (incl. `basic`) + 2
  spider_agent = **20** (was claiming 21/23). Fixed "Flags In Use" table which had
  omitted 8 enabled flags (chrome_store_page, chrome_headless_new, chrome_simd,
  ua_generator, headers, time, control, inline-more) — added them. Removed a
  duplicate contradictory `glob` row. Core enabled count 11→10.

### GUARDRAILS.md
- `validate_url()` path `src/core/http.rs` → `src/core/http/ssrf.rs`. Replaced the
  "Blocked malware/phishing domains (via Spider `firewall` feature)" bullet (the
  feature is off) with a DNS-rebinding-resolver note + a callout that firewall is
  not enabled.

### API.md
- Added missing live routes: `POST /v1/ask/stream` (SSE) and `POST /v1/endpoints`
  (both in `routing.rs` and advertised by `supported_routes()`). Added intro note
  on `/healthz`,`/readyz`,`/api/panel/*` and that `/v1/migrate` also returns 404.
  Date → 2026-06-01.

### API-PARITY.md
- Resolved the ask/stream contradiction: matrix said "Streaming/SSE is not exposed
  as a stable /v1 route" while the advertised block (correctly) lists
  `POST /v1/ask/stream`. Source `client_server.rs:supported_routes()` advertises it
  — updated the `ask` row to "Implemented" for both.
- Added missing CLI-command rows (full set enumerated from
  `src/core/config/types/enums.rs`): `brand`, `diff`, `endpoints` (Implemented →
  `POST /v1/endpoints`), and Deferred locals `config`, `monitor`, `preflight`,
  `smoke`, `compose`, `sync`. Verified advertised-routes block matches source
  exactly (diffed). Date → 2026-06-01.

### ENDPOINTS.md
- Added `--probe-rpc-subdomains` flag (CLI `src/core/config/cli.rs:359`, field
  `endpoints_probe_rpc_subdomains`) to the flags table + switch list with the
  "no-op without --probe-rpc" behavior. Updated the layered-stages table and noted
  RPC probing now covers **ACP** (help string: "JSON-RPC 2.0 / MCP / ACP"). The
  "probe-rpc is CLI-only / no MCP-REST toggle" gap is still accurate. Date → 2026-06-01.

### TESTING.md
- Replaced absolute `/home/jmagar/workspace/axon_rust/...` markdown links with
  relative paths (DESKTOP-PALETTE-TESTING, capture script, config/mcporter.json).
- Fixed stale removed-runtime example: `resolve_test_amqp_url()` no longer exists;
  changed to `resolve_test_qdrant_url()` / `AXON_TEST_QDRANT_URL` (the only
  resolver in source). Updated `just verify` step list to match the actual recipe
  (legacy-runtime-check + validate-plugin + web-check + fmt-check + clippy + check +
  test). Date → 2026-06-01. (mcporter@0.7.3 confirmed against CI.)

### repo/RECIPES.md, REPO.md, RULES.md, SCRIPTS.md
- RECIPES.md: corrected `just verify` description (was "dockerignore check + …").
  All 34 documented recipes verified present via `just --list`.
- REPO.md: tree root `axon_rust/` → `axon/`; added `xtask/`; noted both root
  `migrations/` and `src/jobs/migrations/` exist.
- RULES.md: version-bearing files aligned with root CLAUDE.md (added
  `.claude-plugin/plugin.json`, `README.md`). Pre-commit hooks table: added
  `cargo xtask check` umbrella + the two real sub-checks missing from the doc
  (`check-broken-symlinks`, `check-secrets`) — verified in `xtask/src/checks.rs`.
- SCRIPTS.md: "five enforcement checks" → full xtask set incl. the two above;
  clarified lefthook runs `cargo xtask check`.

### DESKTOP-PALETTE-TESTING.md
- Fixed absolute path to relative. Content left as-is (point-in-time UX checklist).

## Gaps / missing docs (for Phase 2)

- **`axon brand` and `axon diff`** have no `docs/commands/` reference and no MCP/REST
  surface — Agent B's lane for command docs, but flagging the parity gap here.
- **DESKTOP-PALETTE-TESTING.md references artifacts that don't exist in the repo:**
  `scripts/capture-palette-operations.ps1` and `axon-palette.exe`/`axon-palette`
  binary are nowhere in the tree (`find` returns nothing; no source). There IS an
  `apps/palette-tauri/` and `apps/desktop/` — the palette likely moved to a Tauri
  app, so the harness doc is describing an old PowerShell harness. The relative link
  I added is therefore a broken link by intent. Phase 2 should either restore the
  script, repoint the doc at `apps/palette-tauri`, or archive the doc.
- No doc covers the `apps/` multi-surface layout (android, chrome-extension,
  desktop, palette-tauri, web) — REPO.md only mentions `apps/web`.

## Reorg observations (for Phase 2)

- **LIVE-TEST-SCRIPTS.md is grossly misfiled.** Its filename promises live
  integration test scripts, but its entire content is the **Monolith Policy** (file
  index `docs/CLAUDE.md` even lists it as "Live integration test scripts
  reference"). Either rename the file to MONOLITH-POLICY.md (overlaps heavily with
  repo/RULES.md "Monolith policy" — candidate to merge) or repopulate it with the
  live-test content that `scripts/live-test-all-commands.sh` implies. Its current
  content is internally accurate as a monolith-policy doc, so I made no in-content
  edits — the defect is purely the filename↔content mismatch.
- **Monolith policy is documented in 3 places** (LIVE-TEST-SCRIPTS.md, repo/RULES.md,
  root CLAUDE.md) — consolidate.
- **xtask vs scripts enforcement** straddles SCRIPTS.md and RULES.md with overlap;
  consider one canonical "enforcement checks" section.
- **Two dated docs are archival candidates:** `production-readiness-sprint-report-2026-05-12.md`
  (self-labeled historical, with a 2026-05-19 status note) and
  `specs/android-redesign.md` (design-locked snapshot). Both are factually
  self-consistent and not actively misleading — move to `docs/archive/` or
  `docs/reports/`.
- API.md, API-PARITY.md, and ENDPOINTS.md overlap on the `/v1/endpoints` surface;
  ENDPOINTS.md is the deep-dive and should be the single source, with the others
  linking to it (API.md now does).

## Cross-reference notes

- SECURITY.md → `docs/auth/MCP-AUTH.md` (exists), `src/web/CLAUDE.md`, `src/core/http/ssrf.rs`.
- API.md → `docs/SECURITY.md` §6, `src/web/CLAUDE.md`, `docs/ENDPOINTS.md` (added).
- API-PARITY.md → `docs/ENDPOINTS.md` (added in endpoints row).
- ENDPOINTS.md → `docs/SECURITY.md`, `docs/MCP*.md`, `docs/CONFIG.md` (all valid).
- GUARDRAILS.md → `docs/SPIDER-FEATURE-FLAGS.md` (added).
- repo/CLAUDE.md → SETUP.md, CONFIG.md, GUARDRAILS.md, stack/TECH.md, stack/ARCH.md (all verified to exist).
- TESTING.md / DESKTOP-PALETTE-TESTING.md → `scripts/capture-palette-operations.ps1` (**broken — file absent**).
- Code→doc: `src/web/CLAUDE.md` is the authoritative web-surface map; SECURITY.md
  and API.md should keep deferring to it for the panel route tree.
- Authoritative source pointers used: `Cargo.toml` (spider features),
  `src/services/types/client_server.rs:supported_routes()` (advertised REST),
  `src/web/server/routing.rs` (live routes), `src/core/config/types/enums.rs` (CLI
  command set), `xtask/src/checks.rs` (enforcement checks),
  `scripts/check_compose_port_bindings.py` (compose port policy).
