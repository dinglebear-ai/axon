# Agent A — root foundational & architecture docs report

Date: 2026-06-01 · Worktree branch `docs/comprehensive-refresh` · Ground truth: code @ v4.16.0

## Files reviewed

- `docs/README.md` — minor fixes (broken `services/` dir link; doc-tree drift)
- `docs/ARCHITECTURE.md` — minor fixes (stale `axon_rust` name, stale 02/2026 doc-version header, misleading LLM label)
- `docs/INVENTORY.md` — **major fixes** (CLI table missing 15 commands; MCP action tables incomplete; ingest provider list stale)
- `docs/FEATURES.md` — accurate (verified against `[features]` in Cargo.toml; no edits)
- `docs/CLAUDE.md` — minor fixes (stale date; directory layout listed non-existent `screenshots/`, omitted real dirs)
- `docs/CHECKLIST.md` — minor fix (version-bearing file list)
- `docs/FEATURE-DELIVERY-FRAMEWORK.md` — minor fixes (stale header; `CommandKind` path; removed `src/web/actions.rs`)
- `docs/RUST.md` — minor fixes (stale `axon_rust` owner/name)
- `docs/CONTEXT-INJECTION.md` — minor fix (LLM-endpoint label); defaults table fully re-verified accurate

## Fixes made

1. **INVENTORY.md CLI command table** — was 24 commands; rewrote into 4 grouped tables matching the
   `CommandKind` enum (`src/core/config/types/enums.rs`, 39 variants). Added the 15 missing commands:
   `endpoints`, `screenshot`, `diff`, `brand`, `train`, `summarize`, `dedupe`, `monitor`, `sync`,
   `setup`, `preflight`, `smoke`, `compose`, `completions`, `config`. (Verified against
   `ground-truth/axon--help.txt` and the enum.)
2. **INVENTORY.md MCP action tables** — reconciled to the `AxonRequest` enum in `src/mcp/schema.rs`.
   Added direct actions (`endpoints`, `summarize`, `brand`, `diff`, `evaluate`, `suggest`, `dedupe`,
   `migrate`, `debug`, `setup`) and lifecycle families (`watch`, `vertical_scrape` discovery-only).
3. **INVENTORY.md ingest descriptions** (worker table, source-module table, MCP `ingest` family) —
   was "GitHub/Reddit/YouTube"; now "GitHub/GitLab/Gitea/Git/Reddit/YouTube" per the enum/root CLAUDE.md.
4. **INVENTORY.md web module row** — was "`/v1/ask` and client/server action routes"; `/v1/actions`
   was **removed** (`routing.rs:103` → `v1_actions_removed` 404 stub). Rewrote to list real `/v1/*` routes.
5. **ARCHITECTURE.md** — `axon_rust`→`axon` (line 27); collapsed stale `Version: 1.0.0 / 02/25/2026`
   header to `Version: 4.16.0` + `Last Modified: 2026-06-01`; relabeled `LLM[OpenAI-compatible API]`
   to reflect Gemini-headless default + OpenAI-compatible optional (both backends are live —
   `LlmBackendKind` in `src/services/llm_backend/types.rs`, default `GeminiHeadless`).
6. **CONTEXT-INJECTION.md** — line 198 "OpenAI-compatible endpoint" → backend-agnostic phrasing
   (Gemini default / `AXON_LLM_BACKEND=openai-compat` optional). Re-verified all 11 `ask_*` defaults in
   the config table against `src/core/config/types/config_impls.rs` — every value matches (no drift).
7. **CLAUDE.md** — date bump; directory layout: dropped non-existent `screenshots/`, added real
   `auth/`, `config/`, `contracts/`, `specs/`, `superpowers/` (verified via `ls docs/`).
8. **README.md** — replaced broken `services/` guide link (dir doesn't exist) and listed real
   subdirs (`config/`, `contracts/`, `specs/`). Trimodal port table (8001) verified vs `config_impls.rs:187`.
9. **CHECKLIST.md** — version-bearing files: was `Cargo.toml, apps/web/package.json, CHANGELOG.md`;
   `apps/web/package.json` has no version field. Now `Cargo.toml, .claude-plugin/plugin.json, README.md,
   CHANGELOG.md` per root CLAUDE.md bump policy. (`npm run build` confirmed correct — apps/web has
   package-lock.json, not pnpm.)
10. **FEATURE-DELIVERY-FRAMEWORK.md** — date bump; `CommandKind` location `types/config.rs`→`types/enums.rs`;
    web wiring path `src/web/actions.rs` (gone) → `src/web/server/routing.rs` + `src/web/server/handlers/`.
11. **RUST.md** — `owner: "axon_rust"`→`"axon"` and the cross-compile narrative `axon_rust`→`axon`.
    `.cargo/config.toml` snippet (xtask alias + windows linker) verified accurate.

## Code bug noticed (not a doc — flag for maintainers)

- `axon --help` top-level text says **"vector collection (default cortex)"** and the Quick-Start example
  uses `--collection cortex`, but the actual code default is **`axon`** (`config_impls.rs:55`,
  `build_config.rs:71`). The help string is stale. My docs correctly document `axon`. (Also affects
  several `ground-truth/axon-*--help.txt` dumps — they reflect the binary's own stale help text.)

## Gaps / missing docs (for Phase 2)

- No root-doc coverage of the **new commands** `endpoints`, `train`, `monitor`, `sync`, `preflight`,
  `smoke`, `compose`. These need `docs/commands/<name>.md` entries (out of my lane). `docs/ENDPOINTS.md`
  exists at root and should likely move under `commands/`.
- ARCHITECTURE.md has **no section** for the web `/v1/*` REST surface, `monitor` event stream, or the
  `train` preference-collection flow — all are live subsystems absent from the architecture map.
- INVENTORY.md "App services" lists only `axon` server; no mention of the `train`/`monitor`/`sync`
  runtime surfaces.

## Reorg observations (for Phase 2)

- **README.md vs docs/CLAUDE.md disagree on the doc tree.** README lists a "Structured documentation"
  set (SETUP/CONFIG/CHECKLIST/GUARDRAILS/INVENTORY + mcp/repo/stack) while docs/CLAUDE.md lists a
  different root-file inventory. Neither enumerates the ~33 actual root `.md` files (API.md, ASK.md,
  ENDPOINTS.md, REINDEX-GUIDE.md, env-migration-matrix.md, production-readiness-sprint-report-*, etc.).
  Root `docs/` is cluttered — many of these belong in subdirs (`commands/`, `reports/`, `mcp/`).
- `docs/ENDPOINTS.md`, `docs/ASK.md`, `docs/SHELL-COMPLETIONS.md` are command-level and likely belong
  under `commands/`. `docs/DESKTOP-PALETTE-TESTING.md` + `docs/palette-demo/` are a testing concern.
- FEATURES.md title is "Axon Feature Flags" but README/orchestrator may conflate it with the command
  set — it is strictly the **Cargo feature matrix**. Naming could be clearer (e.g. CARGO-FEATURES.md).
- RUST.md uses YAML frontmatter (doc_type/owner/audience) — the only root doc that does; inconsistent
  with the rest.

## Cross-reference notes

- README.md links to: SETUP, CONFIG, CHECKLIST, GUARDRAILS, INVENTORY, mcp/*, repo/*, stack/*,
  ARCHITECTURE, DEPLOYMENT, OPERATIONS, PERFORMANCE, SECURITY, JOB-LIFECYCLE, TESTING, MCP, MCP-TOOL-SCHEMA,
  commands/, ingest/, auth/, config/, contracts/, sessions/, specs/, superpowers/, plans/, reports/.
  (Fixed the dead `services/` link.) All other targets verified present.
- docs/CLAUDE.md references `src/jobs/migrations/`, `src/jobs/store.rs`, `docs/commands/`, `docs/ingest/`
  — all valid.
- ARCHITECTURE.md Key Source Map paths all verified present (incl. `src/jobs/runtime.rs`, `workers.rs`,
  `ops/{enqueue,lifecycle}.rs`, `vector/ops.rs`, `services/llm_backend.rs`, `mcp/server/artifacts.rs`).
- CONTEXT-INJECTION.md source refs (`retrieval.rs`, `build.rs`, `streaming.rs`, `evaluate.rs`,
  `tei/tei_client.rs`) all resolve under `src/vector/ops/commands/ask/` and `.../evaluate/`.
- FEATURE-DELIVERY-FRAMEWORK.md references `docs/commands/<feature>.md`, `docs/README.md`,
  `docs/MCP-TOOL-SCHEMA.md`, `docs/MCP.md` — all present.
- `docs/AGENTS.md` and `docs/GEMINI.md` are symlinks to `docs/CLAUDE.md`; edits flow through automatically.
