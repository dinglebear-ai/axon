---
date: 2026-06-26 15:46:24 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 05e1e6c3
plan: docs/superpowers/plans/2026-06-26-axon-env-config-drift-alignment.md
session id: 34fb82ca-bbd6-4c0c-9a6a-2a467ee97e15
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/34fb82ca-bbd6-4c0c-9a6a-2a467ee97e15.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
beads: none changed
---

# Axon env and config drift alignment

## User Request

Review live `/home/jmagar/.axon/.env` and `/home/jmagar/.axon/config.toml`, diff them against repo examples, audit the implemented env/config surface, and align live files, examples, and docs so only URLs, secrets, and runtime/bootstrap live in `.env` while all other knobs live in `config.toml`. The user also explicitly asked to back up both live files before editing and to avoid losing performance by accidentally lowering tuned values.

## Session Overview

The session backed up live config, added registry and TOML support for previously env-only tuning keys, migrated live performance knobs into TOML, pruned TOML-owned values from live `.env`, refreshed root examples and docs, added drift gates, verified the live Axon stack, and committed the work in three repo commits.

## Sequence of Events

1. Backed up `/home/jmagar/.axon/.env` and `/home/jmagar/.axon/config.toml` to timestamped `060626-025901` backups before making any live edits.
2. Audited live config with `./scripts/axon config list` and confirmed the original drift: low ask env overrides were suppressing richer TOML ask settings, while TEI/embed throughput env values were intentionally higher than examples.
3. Added an env registry coverage test and registered implemented env keys, then committed `cb306c40`.
4. Added typed TOML sections and runtime resolver wiring for embed, chunking, Qdrant, code search, watch, endpoints, and MCP guards, then committed `f718a017`.
5. Updated live config to preserve high-throughput values in TOML, moved OpenAI model names into `[llm]`, and removed migrated tuning/model keys from live `.env`.
6. Updated `.env.example`, `config.example.toml`, added `config.toml.example` as an alias, refreshed `docs/guides/configuration.md`, added example drift tests, and committed `05e1e6c3`.

## Key Findings

- Live `.env` contained TOML-owned values such as `AXON_ASK_*`, `AXON_TEI_MAX_CONCURRENT`, `AXON_TEI_MAX_IN_FLIGHT_INPUTS`, `AXON_EMBED_*`, and Qdrant upsert tuning.
- Some live env values lowered performance or recall relative to TOML, especially `AXON_ASK_FULL_DOCS=1`, `AXON_ASK_BACKFILL_CHUNKS=1`, `AXON_ASK_DOC_FETCH_CONCURRENCY=1`, and `AXON_ASK_DOC_CHUNK_LIMIT=24`.
- Some live env values were desirable high-throughput settings and were preserved in TOML instead of dropped: `tei.max-client-batch-size = 256`, `embed.tei-max-in-flight-inputs = 512`, `embed.pool-max-inputs = 1024`, and `embed.prep-concurrency = 12`.
- The code-search defaults in the new resolver were corrected back to the implemented defaults: reindex timeout `300` seconds and changed-file batch size `5`.
- `AXON_CHROME_USER_AGENT` was caught by the new `.env.example` drift gate and removed from `.env.example` because `chrome.user-agent` is TOML-owned.

## Technical Decisions

- Kept URLs, secrets, OAuth tokens, host/runtime paths, Docker Compose interpolation, and selected process bootstrap in `.env`.
- Moved non-secret OpenAI model names to `[llm]` because TOML support already existed.
- Preserved current live throughput rather than falling back to lower example defaults.
- Added tests that fail when TOML-owned keys reappear in `.env.example` and when the root `config.example.toml` stops parsing.
- Left existing sibling worktrees and branches untouched because ownership and merge safety were not proven.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.env.example` | -- | Restrict env example to URLs, secrets, runtime/bootstrap, and Compose interpolation | commit `05e1e6c3` |
| modified | `config.example.toml` | -- | Add full TOML tuning surface for newly wired sections | commit `05e1e6c3` |
| created | `config.toml.example` | -- | Root example alias requested by user | commit `05e1e6c3` |
| modified | `docs/guides/configuration.md` | -- | Document current `.env`/TOML split and all new TOML sections | commit `05e1e6c3` |
| created | `docs/superpowers/plans/2026-06-26-axon-env-config-drift-alignment.md` | -- | Execution plan for the drift alignment | commit `05e1e6c3` |
| modified | `crates/axon-core/src/config/parse/env_registry_tests.rs` | -- | Registry coverage and `.env.example` drift gate | commits `cb306c40`, `f718a017`, `05e1e6c3` |
| modified | `crates/axon-core/src/config/parse/toml_config.rs` | -- | TOML schema for new sections | commit `f718a017` |
| modified | `crates/axon-core/src/config/parse/tuning.rs` | -- | Central env/TOML/default resolvers | commit `f718a017` |
| modified | `crates/axon-core/src/config/parse/toml_config_tests.rs` | -- | TOML parse coverage for extended sections and root example | commits `f718a017`, `05e1e6c3` |
| modified | `crates/axon-vector/src/ops/input.rs` and TEI/Qdrant pipeline files | -- | Consume TOML-backed tuning resolvers instead of raw env reads | commit `f718a017` |
| modified | `crates/axon-code-index/src/config.rs`, `crates/axon-code-index/src/indexer.rs` | -- | TOML-backed code-search tuning | commit `f718a017` |
| modified | `crates/axon-jobs/src/watch.rs`, `crates/axon-jobs/src/workers/watch_scheduler.rs` | -- | TOML-backed watch scheduler tuning | commit `f718a017` |
| modified | `crates/axon-services/src/endpoints*.rs` | -- | TOML-backed endpoint concurrency | commit `f718a017` |
| modified | `crates/axon-mcp/src/server/tasks.rs` and config literal builder | -- | TOML-backed MCP task/local embed guards | commit `f718a017` |
| modified | `/home/jmagar/.axon/.env` | -- | Live env pruned to URL/secret/runtime/bootstrap values | `./scripts/axon config list` output |
| modified | `/home/jmagar/.axon/config.toml` | -- | Live TOML fully populated with current tuning knobs | `./scripts/axon config list` output |

## Beads Activity

No bead mutations were performed by this session. Maintenance reads were run with `bd list --all --sort updated --reverse --limit 100 --json` and `tail -200 .beads/interactions.jsonl`; the output was historical/noisy and did not show a directly relevant bead created or closed for this config drift session.

## Repository Maintenance

- Plans: `docs/plans/` and `docs/plans/complete/` were inspected. No plan was moved because none of the top-level active plan files could be proven completed from this session's evidence.
- Beads: Reads were performed; no bead was created, edited, claimed, or closed because the session work had already been completed and no directly relevant active bead was identified from the noisy historical output.
- Worktrees and branches: `git worktree list --porcelain`, local branches, and remote branches were inspected. No cleanup was performed because sibling worktrees are active or ownership was unclear.
- Stale docs: `docs/guides/configuration.md`, `.env.example`, and `config.example.toml` were updated because the implemented code contradicted the old env-only documentation.
- Transparency: The repository was clean before this save-note artifact; `main` was ahead of `origin/main` by three commits before the session-note commit.

## Tools and Skills Used

- Skills: `superpowers:writing-plans`, `superpowers:executing-plans`, and `vibin:save-to-md`.
- Shell commands: Used for git state, config listing, backups, tests, doctor, Beads reads, and session maintenance evidence.
- File edits: Used `apply_patch` for repo and live config edits where practical; used a shell filter to safely remove exact keys from live `.env` while preserving secrets.
- External CLIs: `cargo`, `git`, `gh`, `bd`, `./scripts/axon`.
- MCP/Labby/Lumen: No MCP tool calls were required for the implementation. A developer hint requested Lumen semantic search for code discovery, but no callable `mcp__lumen__semantic_search` tool was available in this Codex tool surface.
- Transcript: A Claude transcript path existed, but it contained only an older short cut-off prompt and was not material evidence for this Codex session.

## Commands Executed

| command | result |
|---|---|
| `cp -p ~/.axon/.env ~/.axon/.env.bak-20260626-025901` and `cp -p ~/.axon/config.toml ~/.axon/config.toml.bak-20260626-025901` | Live backups created with mode `600` |
| `./scripts/axon config list` | Confirmed live drift before edits and resolved live config after edits |
| `./scripts/axon doctor` | Overall completed; SQLite, TEI, Qdrant, Chrome, Gemini, crawl, extract, embed, and ingest passed |
| `cargo test -p axon-core env_registry` | Passed |
| `cargo test -p axon-core root_config_example_parses` | Passed |
| `cargo test -p axon-core extended_toml_tuning_sections_parse` | Passed |
| `cargo test -p axon-vector qdrant_upsert_splits_into_configured_batches` | Passed |
| `cargo check -p axon-core` | Passed |
| `cargo check -p axon-code-index -p axon-vector -p axon-jobs -p axon-services -p axon-mcp` | Passed |
| `git commit -m "test: guard axon env registry completeness"` | Created `cb306c40` |
| `git commit -m "feat: support toml tuning surfaces"` | Created `f718a017` |
| `git commit -m "docs: align axon env and config examples"` | Created `05e1e6c3` |

## Errors Encountered

- `cargo test -p axon-code-index config` failed because a test helper in `axon-vector` is gated behind test-util feature wiring; this was not caused by the config changes and was not used as final verification.
- A combined Cargo test command with two filter names failed with `unexpected argument`; each test was rerun individually and passed.
- The first new drift gate failed because `AXON_CHROME_USER_AGENT` remained in `.env.example`; removing it made the gate pass.
- Rust warnings appeared after moving env reads to central resolvers because several helper constants/functions became dead in non-test builds; the implementation was cleaned up and checks passed.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Live `.env` | Mixed URLs/secrets/bootstrap with ask/embed/Qdrant tuning and model names | Contains URLs, secrets, auth/runtime/bootstrap, host/Compose values only |
| Live `config.toml` | Missing many implemented non-secret knobs | Contains ask, LLM model, TEI, embed, chunking, Qdrant, code-search, watch, endpoints, MCP, worker, scrape, Chrome, vertical, antibot, and payload knobs |
| Runtime tuning | Several modules read raw env vars directly | Runtime consumers use central TOML/env/default resolvers |
| Examples | `.env.example` still included TOML-owned tuning | `.env.example` pruned; `config.example.toml` is the full tuning reference; `config.toml.example` aliases it |
| Drift prevention | No gate for example/env split | Tests now fail if TOML-owned keys reappear in `.env.example` or root TOML example stops parsing |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -p axon-core env_registry` | Registry and `.env.example` gates pass | 4 passed | pass |
| `cargo test -p axon-core root_config_example_parses` | Root TOML example parses | 1 passed | pass |
| `cargo test -p axon-core extended_toml_tuning_sections_parse` | Extended TOML sections parse | 1 passed | pass |
| `cargo test -p axon-vector qdrant_upsert_splits_into_configured_batches` | Qdrant batch tuning behavior stays covered | 1 passed | pass |
| `cargo check -p axon-core` | Core compiles | Finished dev profile | pass |
| `./scripts/axon config list` | Live files parse and show migrated knobs in TOML | Live `.env` redacted, live TOML shows all migrated values | pass |
| `./scripts/axon doctor` | Current stack healthy | Overall completed | pass |
| `git status --branch --short` | Clean except branch ahead before session-note artifact | `main...origin/main [ahead 3]` before this save note | pass |

## Risks and Rollback

- Risk: A live deployment process that still expects old env tuning names could behave differently if it does not read `~/.axon/config.toml`. Mitigation: the Rust binary still accepts env overrides, and `./scripts/axon config list` plus `doctor` confirmed the live host resolves the new TOML values.
- Rollback: Restore `/home/jmagar/.axon/.env.bak-20260626-025901` and `/home/jmagar/.axon/config.toml.bak-20260626-025901`, then revert commits `05e1e6c3`, `f718a017`, and `cb306c40` if the code/docs changes need to be backed out.

## Decisions Not Taken

- Did not delete sibling worktrees or local branches because active ownership and merge safety were not proven.
- Did not move old `docs/plans/` files because completion was not established from this session.
- Did not force every env var into TOML; URLs, secrets, OAuth, host paths, process command/home bootstrap, and Compose/runtime values intentionally remain env-owned.

## References

- `docs/superpowers/plans/2026-06-26-axon-env-config-drift-alignment.md`
- `docs/guides/configuration.md`
- `.env.example`
- `config.example.toml`
- `/home/jmagar/.axon/.env.bak-20260626-025901`
- `/home/jmagar/.axon/config.toml.bak-20260626-025901`

## Open Questions

- Whether `AXON_CODEX_COMPLETION_CONCURRENCY`, `AXON_LLM_COMPLETION_CONCURRENCY`, and `AXON_LLM_COMPLETION_TIMEOUT_SECS` should eventually gain TOML support, or remain runtime/bootstrap because they shape backend process execution.
- Whether a dedicated Beads issue should be created for future env/TOML policy enforcement beyond examples and docs.

## Next Steps

- Push the session-note commit as required by `vibin:save-to-md`.
- Consider adding a future setup/migration command that can automatically move TOML-owned values out of a live `.env` into `config.toml` with a dry-run diff.
- If production Compose consumes a different env/config pair than `/home/jmagar/.axon`, run the same `./scripts/axon config list` and `./scripts/axon doctor` checks in that environment.
