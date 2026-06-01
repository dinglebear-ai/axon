# Agent G — RAG/vector internals, jobs, contracts, eval, server-mode/vertical specs

## Files reviewed
- docs/ASK.md — minor fix (`--server-url` flag does not exist; env-only)
- docs/JOB-LIFECYCLE.md — minor fixes (incomplete migration/schema list)
- docs/REINDEX-GUIDE.md — major fixes (missed schema v4 entirely)
- docs/contracts/qdrant-payload-schema.md — minor fix (one wrong indexed flag)
- docs/eval/README.md — accurate (no edits)
- docs/specs/server-mode-capability-tiers.md — minor fixes (`/v1/actions` already removed)
- docs/specs/server-mode-routing-contract.md — accurate (already frames cutover in past tense; no edits)
- docs/specs/vertical-extractor-metadata.md — accurate (no edits)

## Fixes made

### docs/REINDEX-GUIDE.md (biggest problem)
The guide was titled "Schema v3 Payload Upgrade" and never mentioned v4, but
`PAYLOAD_SCHEMA_VERSION = 4` (`src/vector/ops/qdrant/utils.rs:28`) and the payload-schema
contract documents v4. Per advisor, v4 is a *different* change, not a mechanical bump.
- Retitled to "Schema v3 / v4", added an intro noting current version is 4.
- Added a "What Changed in Schema v4" section documenting the GitHub fields promoted out of
  the non-indexed `git_meta` blob into indexed top-level keys (`gh_stars`, `gh_forks`,
  `gh_language`, `gh_topics`, `gh_is_fork`, `gh_is_archived`, `gh_file_type`, `gh_line_start`,
  `gh_line_end`) — verified against `payload_indexes.rs` and `qdrant-payload-schema.md` v4 row.
  Noted only the GitHub ingest path writes these (GitLab/Gitea/generic/vertical/crawl gain
  nothing at v4 beyond the stamp).
- Corrected the `--by-schema-version` example output to match the real renderer
  (`src/cli/commands/sources.rs:59-69`): header `Payload schema version breakdown`, rows
  `vN (chunks: COUNT)`, BTreeMap keyed by version, missing-field tallied under v1; added v4 row.
- Added the v4 GitHub-facet reason to the "Highest" prioritization row.

### docs/JOB-LIFECYCLE.md
The Data Model section listed only migrations 0001–0003 and a 9-column common schema, yet the
doc references `attempt_count`/`active_attempt_id`/`last_reclaimed_at`/`last_reclaimed_reason`
throughout. Verified `ls src/jobs/migrations/` → 0001–0006.
- Added 0004 (`idx_<kind>_status_created` `(status, created_at DESC)` on all four tables — for the
  `list_service_jobs` sort, NOT the claim query; verified the migration body), 0005 (attempt-metadata
  columns on all four tables — verified full file), 0006 (`axon_ingest_payloads` table, FK CASCADE).
- Added the four attempt-tracking columns to the common-columns block (tagged `(0005)`).
- Noted the `idx_<kind>_status_created` composite index from 0004 alongside `idx_<kind>_status`.

### docs/contracts/qdrant-payload-schema.md
- `git_file_path` was documented "Not currently indexed" but it IS registered in
  `payload_indexes.rs`. Corrected to indexed=yes. (Spot-checked the whole index table against
  `payload_indexes.rs`; everything else — universal fields, `provider`, `git_*`, `gh_*`, `pkg_*`,
  `hf_*`, `so_*`, `hn_*`, `arxiv_id`, `devto_author`, `reddit_subreddit`, `yt_channel` — matched.)

### docs/specs/server-mode-capability-tiers.md
These are forward-looking *draft* design specs; per advisor I touched only verified
current-reality contradictions, not design intent.
- Service Layer Contract: `/v1/actions` removal is now done — it returns a stub 404
  (`v1_actions_removed`, `src/web/server/routing.rs:103,146`; `src/web/CLAUDE.md:62`). Reworded
  from future-tense "remove" to "the cutover has happened".
- Phase 3 checklist: marked `/v1/actions` removal **Done**; noted `--local`/`AXON_LOCAL_MODE`
  and `src/cli/route.rs` (`plan_command_route`/`FallbackPolicy`) are implemented today; flagged
  `--server-required` as still future (confirmed absent in source).

### docs/ASK.md
- Removed the non-existent `--server-url` flag from the streaming note (`grep` of
  `src/core/config/cli/` finds no such arg; routing is env-only via `AXON_SERVER_URL`, config
  field `server_url`). Left `AXON_SERVER_URL`.

### docs/REINDEX-GUIDE.md (additional fix found on re-verify)
- The "Check a specific source type" example used `axon query "repo" --filter '...'`, but `query`
  has **no `--filter` flag** (confirmed against CLI args). Replaced with `axon retrieve <url> --json`
  (valid per the retrieve help dump) plus a pointer to `sources --by-schema-version`.

## Verification highlights (no edit needed but confirmed)
- ASK.md: every other flag (`--explain`, `--diagnostics`, `--stream`/`--no-stream`, `--follow-up`,
  `--session`, `--reset-session`) matches `ground-truth/axon-ask--help.txt`; all referenced
  scripts/perf docs exist. Pipeline description (retrieval → rerank → context → Gemini, with
  `/v1/ask` buffered) is accurate.
- eval/README.md: golden-set row schema (`id`/`question`/`category`/`expected_traits`) and
  retrieval-fixtures schema (`id`/`domain`/`query`/`expected`/`notes`) match the live `.jsonl`
  files; `--json` `scores` shape and the two scripts are correct.
- vertical-extractor-metadata.md: all 17 extractors, prefixes, indexed-field lists, and
  `auto_dispatch=false` (amazon, ebay) match `src/extract/registry.rs` and `payload_indexes.rs`.
  The `git_meta = {stars, forks, language, topics}` line is correct for the *vertical extractor*
  path (`github_repo.rs:185`) — the v4 gh_* promotion happens in the separate *ingest* path, so
  the spec is not stale.
- routing-contract.md: already describes `/v1/actions` removal in past tense; consistent with code.
- SQLite + in-process workers only (no Postgres/Redis/AMQP/lite-mode) confirmed across all files.

## Gaps / missing docs (for Phase 2)
- **No standalone re-index doc for v4 GitHub facets beyond what I added inline.** Adequate for now.
- **`--server-required` flag** is referenced by both specs but unimplemented — fine as design
  intent, but Phase 2 should track it so docs and code converge.
- **`axon sources --by-schema-version`** has no dedicated commands/ doc entry; it's only described
  in REINDEX-GUIDE and `src/vector/CLAUDE.md`. A `docs/commands/sources.md` flag note would help.

## Reorg observations (for Phase 2)
- The two server-mode specs (`server-mode-capability-tiers.md`, `server-mode-routing-contract.md`)
  overlap heavily — Server Availability, Timeouts, MCP Stdio Fallback, Security Boundary,
  Observability, Cutover Exit Criteria, and Dev Wrapper appear near-verbatim in both. Candidate
  for consolidation into one spec with a normative-rules section + a tiers/phases section.
- REINDEX-GUIDE.md (root of docs/) overlaps with `docs/contracts/qdrant-payload-schema.md`
  (Payload Schema Versioning + Point Lifecycle). The version-history table is duplicated; consider
  making the contract the single source and having the guide link to it.
- JOB-LIFECYCLE.md duplicates a lot of `src/jobs/CLAUDE.md`; fine as a reader-facing doc but worth
  noting for dedupe.

## Cross-reference notes
- REINDEX-GUIDE.md links to: `docs/contracts/qdrant-payload-schema.md`, `README.md` (axon migrate);
  cites code `src/vector/ops/qdrant/utils.rs`, `src/ingest/git_payload.rs`.
- qdrant-payload-schema.md links to: `docs/specs/vertical-extractor-metadata.md`; cites many
  `src/ingest/*` and `src/vector/ops/tei/qdrant_store/payload_indexes.rs`.
- vertical-extractor-metadata.md links to: `docs/contracts/qdrant-payload-schema.md`.
- ASK.md links to: `scripts/bench-ask.sh`, `docs/perf/README.md`, `docs/eval/README.md`,
  `docs/perf/quality-parity-2026-05-07.md` (all exist).
- JOB-LIFECYCLE.md links to: `src/jobs/CLAUDE.md`, `src/services/CLAUDE.md`.
- **Out-of-lane note (do not edit):** `src/vector/CLAUDE.md:170` still says
  `payload_schema_version = 2`; the real constant is 4. Worth fixing in a code-doc pass.
