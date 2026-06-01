# Agent B — config, deployment, operations, performance, stack report

Verified against current source (4.16.0): `src/core/config/**`, `config.example.toml`,
`.env.example`, `docker-compose*.yaml`, `Cargo.toml`, `Justfile`, `scripts/dev-setup.sh`,
and the v4.16.0 `--help` ground-truth dumps.

## Files reviewed

- `docs/CONFIG.md` — minor fixes (accurate; 2 corrected defaults)
- `docs/env-migration-matrix.md` — major fixes (stale 2026-05-15 audit artifact)
- `docs/config/env-migration-matrix.toml` — accurate (already current; 0 edits)
- `docs/DEPLOYMENT.md` — minor fixes
- `docs/OPERATIONS.md` — minor fixes (well-cited; 2 corrections)
- `docs/SETUP.md` — minor fixes
- `docs/PERFORMANCE.md` — minor fixes
- `docs/perf/README.md` — accurate (0 edits)
- `docs/perf/quality-parity-2026-05-07.md` — minor fix (stale env var)
- `docs/perf/thin-page-rate.md` — accurate (historical query snapshot; 0 edits)
- `docs/stack/CLAUDE.md` — accurate (index file; 0 edits)
- `docs/stack/ARCH.md` — minor fixes
- `docs/stack/TECH.md` — major fixes (firewall flag + versions)
- `docs/stack/PRE-REQS.md` — minor fixes

## Fixes made

**docs/CONFIG.md**
- `search.collection` default `cortex` → `axon` (config_impls.rs:55, build_config.rs, global_args.rs all default to `axon`; only the in-code `help.rs` string still says cortex, which is a code bug out of my lane).
- `AXON_TEST_QDRANT_URL` default `…:53333` → `…:53335` (matches `scripts/dev-setup.sh:373`, which is what users actually get).

**docs/env-migration-matrix.md** (the `.md` was stale; the sibling `.toml` is already current)
- `OPENAI_MODEL/BASE_URL/API_KEY`: reclassified `delete (migration.rs DeleteOnMigration)` → `external/test-only` — these are no longer in `migration.rs` (verified absent); only referenced by scripts/tests. Matches the `.toml`.
- "Delete (migration.rs — legacy removed paths)" table: AMQP/PG/Redis/queue/lite keys are NOT in `migration.rs` anymore (verified no non-test src reads). Removed the false `migration.rs` source attribution; reframed as fully-removed runtimes to scrub manually.
- Removed all `axon setup repair --migrate-env` references (4 sites) — no such command exists; current `setup` subcommands are `init/check/targets/plugin-hook`, and `setup init` does not prune unknown keys.
- ACP section: "pending axon_rust-387" → "removed; done" (AXON_ACP_*/AXON_ASK_AGENT/AXON_ASK_BACKEND have no src reads).
- Left historical "Acceptance Criteria"/ztqd process narration intact (per scope: don't rewrite past-audit records).

**docs/DEPLOYMENT.md**
- `AXON_BIN=/workspace/axon_rust/…` → `/workspace/axon/…` (repo renamed).
- Corrected the Compose publish claim: `${AXON_MCP_HTTP_PUBLISH:-8001}` binds **all interfaces** (`0.0.0.0:8001`) by default, NOT `127.0.0.1`. Doc previously said loopback-by-default — security-relevant. Now matches CONFIG.md and compose `"…:8001"` semantics.

**docs/OPERATIONS.md**
- Source-map path `src/core/health/doctor/runtime.rs` → `sqlite.rs` (the real file; line 93 already cited sqlite.rs).
- Qdrant snapshot examples: `${AXON_COLLECTION:-cortex}` → `:-axon` (correct default). Left `migrate --from cortex --to cortex_v2` as illustrative names (matches root CLAUDE.md convention).

**docs/SETUP.md**
- Clone path `~/workspace/axon_rust` → `~/workspace/axon` (×2).
- Rewrote the "spider_agent path error" troubleshooting: `Cargo.toml` no longer uses a local `spider_agent` path (it's crates.io `2.47.89`); only `lab-auth` is path-patched to `vendor/lab-auth`.

**docs/PERFORMANCE.md**
- "Sitemap cap fixed at 512 (not a CLI flag)" → clarified it's a `512` default configurable via `scrape.max-sitemaps` in config.toml (+ `--sitemap-since-days`). Confirmed `--max-sitemaps` is NOT a crawl CLI flag (help dump + global_args.rs).
- Source map `src/web/actions.rs` (missing) → `src/web/server/handlers/rest/*`.
- Confirmed `AXON_QDRANT_UPSERT_BATCH_SIZE` (default 256) is a REAL env var (qdrant_store.rs:398) distinct from `AXON_QDRANT_POINT_BUFFER` — left as-is.

**docs/stack/TECH.md**
- Removed `firewall` from "Spider features enabled" and added it to "explicitly NOT enabled" with reason — `Cargo.toml:63-67` confirms firewall is OFF (build.rs GitHub-rate-limit panic). This was a direct contradiction of root CLAUDE.md.
- Replaced the incomplete enabled-features list with the actual Cargo.toml set.
- `rmcp 1.1+` → `1.5+`; `text-splitter 0.29` → `0.30`; TEI image `HuggingFace latest` → `ghcr.io/huggingface/text-embeddings-inference:89-1.9` (compose pin); Node `22+` → `24+` (dev-setup.sh `REQUIRED_NODE_MAJOR=24`).

**docs/stack/ARCH.md**
- Ask context limit `120K` → `300K` (default `ask_max_context_chars: 300_000`).

**docs/stack/PRE-REQS.md**
- Clone path `axon_rust` → `axon`; Node `20.9+` → `24+` (×2); TEI image pinned to `:89-1.9`.

**docs/perf/quality-parity-2026-05-07.md**
- Judge model env `$OPENAI_MODEL` → `JUDGE_MODEL` / `AXON_HEADLESS_GEMINI_MODEL` (matches `scripts/evaluate-ask-golden.sh:42`; OPENAI_MODEL removed in 3.0.0).

## Gaps / missing docs (for Phase 2)

- **No doc for the new operational commands** `endpoints`, `train`, `monitor`, `sync`,
  `smoke`, `compose`, `preflight`. They appear in the binary (`axon --help`) and ground-truth
  dumps but have no `docs/commands/` entries (commands are Agent B-adjacent but out of my file lane).
  DEPLOYMENT/OPERATIONS/SETUP reference `compose`/`preflight`/`smoke` correctly but there's no
  reference page describing their subcommands/flags.
- **`docs/env-migration-matrix.md` is a frozen 2026-05-15 snapshot** while the `.toml` is the live,
  more-complete registry mirror (214 entries, includes `AXON_ENDPOINT_*`, correct OPENAI class).
  Phase 2 should decide whether the `.md` is regenerated from the `.toml`/registry or demoted to
  `reports/`. I fixed the `.md`'s current-state lies but did not reconcile the two as source-of-truth.

## Reorg observations (for Phase 2)

- `docs/env-migration-matrix.md` + `docs/config/env-migration-matrix.toml` overlap heavily with
  `docs/CONFIG.md`'s env tables. Three docs describe the same env surface with different fidelity.
  Consider: `.toml` = generated source of truth; `CONFIG.md` = curated human reference; retire/auto-gen
  the `.md`.
- `docs/stack/PRE-REQS.md` and `docs/SETUP.md` duplicate the prerequisites table almost verbatim.
  SETUP already links to PRE-REQS — the inline duplicate in SETUP could become a pure link.
- The two `docs/perf/*` snapshot files (quality-parity, thin-page-rate) are dated bead-decision
  artifacts, not living guides — they read like `reports/` content. Consider moving under `reports/`.
- DEPLOYMENT.md header has stale doc-internal metadata ("Version 1.1.0 | 10:25:00 | 03/11/2026 EST")
  with a malformed date; PERFORMANCE.md/OPERATIONS.md similar `Last Modified` lines are inconsistent.

## Notes left intentionally unedited (verified, defensible as-is)

- **`CHROME_URL`**: CONFIG.md frames it as a live Spider-rs-native CDP var (correct —
  `src/crawl/engine/runtime.rs:77` + `src/crawl/CLAUDE.md` confirm spider reads it raw as the
  `CHROM_BASE` fallback and instruct setting `CHROME_URL=http://127.0.0.1:6000`). The
  env-migration-matrix.md classifies it `delete`/stale alias, mirroring `migration.rs:275`. These
  two framings conflict in the codebase itself (registry says Delete; crawl runtime still relies on
  it). Left both: CONFIG.md = runtime reality, matrix = faithful registry mirror. Phase 2 should
  reconcile `migration.rs`'s Delete classification with the crawl runtime's actual dependence.
- **`--max-sitemaps` cross-ref drift**: root `CLAUDE.md` lists `--max-sitemaps <n>` as a global CLI
  flag, but it is NOT a clap flag (absent from `global_args.rs` and the crawl `--help` dump); it's a
  `scrape.max-sitemaps` config.toml key only. Out of my file lane (root CLAUDE.md), flagged for fixing.

## Cross-reference notes

Links FROM my docs TO others (for reorg link-fixing):
- CONFIG.md → `docs/mcp/ENV.md`.
- SETUP.md → `stack/PRE-REQS.md`, `CONFIG.md`, `mcp/DEPLOY.md`.
- OPERATIONS.md → `PERFORMANCE.md`, `SECURITY.md`, `JOB-LIFECYCLE.md`, `DEPLOYMENT.md`, `CONFIG.md`, `MCP.md`.
- DEPLOYMENT.md → `mcp/DEPLOY.md`, `OPERATIONS.md`, `JOB-LIFECYCLE.md`.
- stack/CLAUDE.md → `repo/REPO.md`, `repo/RECIPES.md`, `CONFIG.md`, `mcp/DEPLOY.md`.
- stack/ARCH.md → `mcp/PATTERNS.md`, `ARCHITECTURE.md`. stack/TECH.md + PRE-REQS.md → `repo/RECIPES.md`.
- perf/README.md → `perf/quality-parity-2026-05-07.md`.

Code→doc path references I corrected and re-verified all exist:
`src/core/health/doctor/sqlite.rs`, `src/cli/commands/crawl/subcommands.rs`, `src/cli/commands/migrate.rs`,
`src/jobs/ops/enqueue.rs`, `src/jobs/store.rs`, `src/core/paths.rs`, `src/core/logging/size_rotating.rs`,
`src/vector/ops/tei/tei_client.rs`, `src/vector/ops/tei/qdrant_store.rs`, `src/web/server/handlers/rest/*`.
