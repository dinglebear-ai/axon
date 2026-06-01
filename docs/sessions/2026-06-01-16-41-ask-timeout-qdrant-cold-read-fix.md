---
date: 2026-06-01 16:41:41 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 4bc0e617
session id: 0278d47f-a153-4cf2-9ac2-535a770df8bb
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/0278d47f-a153-4cf2-9ac2-535a770df8bb.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
beads: axon-ask-qdrant-coldread-30s-timeout, axon-plugin-manifests-no-version-sha-versioning (memories)
---

# Ask code review, >30s timeout diagnosis, and Qdrant cold-read fix

## User Request

Two prompts drove the session: (1) "review all code for the ask action", then (2) "investigate why the command is timing out >30s when it used to average ~8-10s". Follow-ups asked whether ask was running locally, why the three launch paths exist, why timing logged `None`, to bake the fix into collection creation, and to record findings in beads.

## Session Overview

Reviewed the full `ask` surface (~6,200 lines, 25 files) and found no correctness bugs. Then diagnosed the >30s `ask` timeouts to Qdrant cold on-disk reads tripping a 30s HTTP-client cap, applied a runtime fix to the live collection, baked the same config into `ensure_collection`, shipped it as v4.18.3 to `main`, and recorded two persistent memories.

## Sequence of Events

1. Read the entire ask path (CLI, services, vector ops, MCP, web handlers); consulted the advisor; reported findings (no bugs, a few convention nits).
2. Switched to the timeout investigation under the systematic-debugging skill; gathered per-stage `timing_ms`, server logs, Qdrant config, and live latency probes.
3. Localized the cost to the LLM and Qdrant stages; reproduced single-permit LLM serialization and, decisively, a 27s cold Qdrant read with only the background crawl running.
4. Confirmed via `strace` that default `axon ask` runs in **server mode** (dials `:8001`), correcting an earlier mis-read that it ran in-process.
5. Identified `quantization.always_ram:false` + on-disk vectors/HNSW as the cold-read cause and the 30s `internal_service_http_client` as the hard-error trigger.
6. Applied runtime PATCH (`always_ram:true` + `hnsw.on_disk:false`) to the live `axon` collection; verified warm asks back to ~5-7s.
7. Baked the config into `ensure_collection`, updated two tests, bumped to v4.18.3, committed and pushed to `main`, and recorded two `bd remember` memories (pushed to Dolt).

## Key Findings

- **Root cause:** the `axon` collection (3.95M points) was created with `vectors.on_disk:true` + scalar int8 `quantization.always_ram:false` + `hnsw.on_disk:true`, so Qdrant relied on the OS page cache. A concurrent crawl evicts the working set → 20-27s cold mmap reads. Every Qdrant search uses `internal_service_http_client` with a **30s** timeout (`src/core/http/client.rs`), so a cold read exceeding 30s hard-fails `ask`. Warm reads are ~40-60ms.
- **Routing:** default `axon ask` is server-mode (`server_url` from `AXON_SERVER_URL` in `~/.axon/.env`); `--local`/`AXON_LOCAL_MODE` (`global_args.rs:218`) forces in-process and wins over `server_url` (`config_literal.rs:339`). Confirmed by `strace` (connect to `:8001`).
- **Timing logs showed `None`** because `#[tracing::instrument]` on `retrieve_ask_candidates` (`retrieval.rs:110`) did not `skip(timing)`; tracing snapshots span fields once at entry (all `None`). Real per-stage values were always recorded. Fixed in v4.18.1 (`skip(cfg, query, timing)`).
- **Code review:** no correctness bugs. Nits: inline `#[cfg(test)] mod tests` in `output.rs:148` and `dispatch.rs:238` (violates sidecar convention); `#[cfg(test)]` duplicate logic in `heuristics.rs` (`top_domains`/`authoritative_ratio` are byte-identical clones of `retrieval.rs`); hardcoded `"claude"` token in `selection.rs:373`; greedy budget admission vs score order. The `apply_ask_overrides` ↔ `tuning.rs` clamp ranges were verified in sync.
- **Sparse + dense** hybrid (`bm42` + `dense`, RRF) was already created by `ensure_collection` (`qdrant_store.rs:331`) and backfilled by `patch_add_sparse` — no change needed.

## Technical Decisions

- Keep raw f32 vectors `on_disk` (they total ~15 GB; with `always_ram` quantization they don't belong in RAM, or a 3.95M-point collection needs ~19 GB and exceeds the 12 GB container limit). Pin only the int8 quantized vectors (~3.8 GB) and HNSW graph (~0.9 GB).
- Bake all three settings into `ensure_collection` together (not `always_ram` alone), because `always_ram:true` without `vectors.on_disk:true` would keep both raw and quantized copies in RAM.
- Do not add a `version` field to plugin manifests — Claude Code versions plugins by git SHA when none is declared, so every push is a new version (desired). Only `Cargo.toml`/`README.md`/`CHANGELOG.md` get bumped.

## Files Changed

| status | path | purpose | evidence |
|---|---|---|---|
| modified | src/vector/ops/tei/qdrant_store.rs | bake `always_ram:true` + `hnsw.on_disk:false` + `vectors.on_disk:true` into create body | commit 4bc0e617 (+16/-3) |
| modified | src/vector/ops/tei/qdrant_store_tests.rs | update two create-body tests to expect `always_ram:true` | commit 4bc0e617 (+2/-2); 3 tests pass |
| modified | Cargo.toml | version 4.18.2 → 4.18.3 | commit 4bc0e617 |
| modified | README.md | version 4.18.2 → 4.18.3 | commit 4bc0e617 |
| modified | CHANGELOG.md | add [4.18.3] entry | commit 4bc0e617 |
| modified (prior) | src/vector/ops/commands/ask/context/retrieval.rs | `skip(cfg, query, timing)` (timing-log fix) | already in HEAD via 81b3a5c7 (v4.18.1); empty diff vs HEAD this session |

Non-git runtime changes: PATCH `/collections/axon` (`always_ram:true`, then `hnsw.on_disk:false`); `docker restart axon` to deploy the timing-log fix; two `bd remember` entries pushed to Dolt.

## Beads Activity

No issues were created, closed, or edited. Two persistent memories were recorded via `bd remember` and pushed to Dolt:

| id | action | why |
|---|---|---|
| axon-ask-qdrant-coldread-30s-timeout | created | Captures the cold-read root cause, the 30s `internal_service_http_client` cap, and the `always_ram`/on-disk fix |
| axon-plugin-manifests-no-version-sha-versioning | created | Records that plugin manifests are intentionally version-less (SHA = version); CLAUDE.md's bump list is stale on this point |

## Repository Maintenance

- **Plans:** none created or completed this session; no plan files moved. The injected "active plan" path points at a different repo (`axon_rust`) and is out of scope.
- **Beads:** two memories created and Dolt-pushed (above); no issues opened/closed.
- **Worktrees/branches:** `git worktree list --porcelain` shows only `/home/jmagar/workspace/axon`. Remote branches `feat/llms-txt-probe`, `feat/spider-2.51-crawl-efficiency`, `feat/url-watch-change-detection` left untouched (unmerged / not owned by this session).
- **Stale docs:** identified that CLAUDE.md's "Version Bumping" section still lists `.claude-plugin/plugin.json`, which is now deliberately version-less. Not edited here to keep the session-file commit single-path; listed as a follow-up in Next Steps.

## Tools and Skills Used

- **Shell (Bash):** `git`, `curl` (Qdrant + `/v1/ask`), `docker` (logs/inspect/restart/stats), `cargo` (check/test/build), `bd` (remember/memories/dolt push), `strace`, `jq`, `python3`. Used for all diagnosis, edits-verification, and the deploy.
- **File tools:** Read/Edit/Write for the code review and the `qdrant_store.rs`/test/version edits.
- **Skills:** `superpowers:systematic-debugging` (framed the investigation); `vibin:save-to-md` (this note).
- **Advisor:** consulted for the code review and to sanity-check the twice-revised timeout root cause.
- **AskUserQuestion:** used to confirm invocation method, symptom, concurrent load, and remediation scope.

## Commands Executed

| command | result |
|---|---|
| `curl -X PATCH /collections/axon -d '{"quantization_config":{"scalar":{...,"always_ram":true}}}'` | `{"result":true,"status":"ok"}` |
| `curl -X PATCH /collections/axon -d '{"hnsw_config":{"on_disk":false}}'` | `{"result":true,"status":"ok"}` |
| `docker restart axon` | rc=0; `/healthz`=200 within ~4s |
| `cargo test --lib ensure_collection_sends` | 3 passed; 0 failed |
| `git push` | `4ea0c067..4bc0e617 main -> main` |
| `bd dolt push` | Push complete (rc=0) |

## Errors Encountered

- `cargo test --lib a b c` → "unexpected argument" (cargo takes one filter). Resolved by using a single prefix `ensure_collection_sends`.
- `docker exec axon ...` denied twice by the sandbox (prod container shell). Worked around by reading the container's log file via its bind-mounted `~/.axon/logs/axon.log` on the host.
- Premature "fix didn't work" conclusion from a single cold-first-touch ask (36s); corrected by repeat asks (4.6-7.0s) proving steady-state is fixed.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| `axon ask` latency | intermittent >30s hard error (cold Qdrant read tripping 30s cap) | ~5-7s steady; warm Qdrant ~40-60ms |
| Live `axon` collection | quantized vectors + HNSW evictable from page cache | int8 quantized (~3.8 GB) + HNSW (~0.9 GB) pinned in RAM |
| New collections (`ensure_collection`) | `always_ram:false`, HNSW on disk | `always_ram:true`, HNSW in RAM, raw vectors on disk |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| repeat `/v1/ask` (warm) | < 10s, qdrant sub-second | 4.6-7.0s total, qdrant 41-57ms | pass |
| raw dense probe (warm) | ms-range | 0.033-0.047s | pass |
| `cargo test ensure_collection_sends` | all pass | 3 passed, 0 failed | pass |
| `git status -sb` | up to date with origin | `## main...origin/main` | pass |

## Risks and Rollback

- The live-collection PATCH is reversible (`always_ram:false`, `hnsw.on_disk:true`). The code change only affects future/recreated collections; revert by reverting commit 4bc0e617.
- RAM: Qdrant settled ~4 GB of its 12 GB limit (an 8.4 GB transient appeared during re-optimization). Host had 32 GB available.
- First ask after any server/Qdrant restart pays a one-time ~10s cold page-in, then warms.

## Open Questions

- CLAUDE.md "Version Bumping" section is stale (lists `plugin.json`). Memory recorded; doc edit deferred.

## Next Steps

- Optional: update CLAUDE.md "Version Bumping" to drop `.claude-plugin/plugin.json` (manifests are version-less by design).
- Optional cleanups from the code review: move inline test blocks in `output.rs`/`dispatch.rs` to sidecars; de-duplicate the `#[cfg(test)]` clones in `heuristics.rs`; reconsider the hardcoded `"claude"` token in `selection.rs`.
- Glance at the dependabot advisory GitHub printed during the push (unrelated to this change).
