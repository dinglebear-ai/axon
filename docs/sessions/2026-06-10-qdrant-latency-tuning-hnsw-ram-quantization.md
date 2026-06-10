# Qdrant latency tuning — HNSW + quantized vectors into RAM, branch cleanup, container refresh

```yaml
date: 2026-06-10 14:40:34 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: c2579fe3
session id: 48743621-23c8-4c82-a27b-225a451518d4
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/48743621-23c8-4c82-a27b-225a451518d4.jsonl
working directory: /home/jmagar/workspace/axon
beads: none created/closed; one `bd remember` entry (qdrant-axon-collection-latency-tuning-2026-06-10)
```

## User Request

"How much mem is qdrant currently using" → "test a couple queries and check the latency" (hitting Qdrant directly) → "put it back in RAM" → switch checkout to main, clean up branches → "patch that and push it" (collection-creation defaults) → save session log.

## Session Overview

Diagnosed and fixed severe Qdrant search latency on the 4.5M-point `axon` collection: fresh dense queries took 12–42 s because both the HNSW graph and the int8-quantized vectors lived on disk with a cold page cache. PATCHed the live collection to `hnsw_config.on_disk: false` + `quantization.scalar.always_ram: true`, bringing fresh-query latency to 0.2–2.1 s at ~10 GiB RSS. Discovered the source fix already existed on main (commit `49ad61f5`, landed the same morning) — the running container simply predated it — so no code change was needed; instead rebuilt the debug binary and restarted the `axon` container. Also switched the checkout from the defunct `feat/qdrant-affinity-tei-burst` branch to main and cleaned up merged branches.

## Sequence of Events

1. **Memory check.** `docker stats` showed `axon-qdrant` at 5.84 GiB / 16 GiB (36.5%) — down from the historical ~15/16 GiB, indicating a cold cache after a restart ~11 h earlier.
2. **Direct latency test.** Query-by-point-ID via `POST /collections/axon/points/query` (`using: dense`): fresh queries 12–42 s, repeated query 34 ms, with `qdrant-internal` time ≈ wall time (not network).
3. **First diagnosis.** Collection green and fully indexed (9.05 M vectors, 27 segments) but `hnsw_config.on_disk: true` and `vectors.dense.on_disk: true` — cold HNSW traversal as random disk reads.
4. **HNSW → RAM.** `PATCH {"hnsw_config":{"on_disk":false}}` → status went yellow, then **grey** (pending, optimizer never started). Nudged with `PATCH {"optimizers_config":{}}` → yellow, rebuild ran (27→14 segments).
5. **Still slow.** Post-rebuild fresh queries remained 5–32 s. Found the real bottleneck: int8 scalar quantization existed but `always_ram: false`, so even quantized vectors were read from disk (collection is 28 GB on disk).
6. **Quantized vectors → RAM.** `PATCH {"quantization_config":{"scalar":{"type":"int8","quantile":0.99,"always_ram":true}}}` → rebuild → fresh queries 0.22–2.1 s, memory ~10.3 GiB / 16 GiB.
7. **Branch switch + cleanup.** Verified `feat/qdrant-affinity-tei-burst` content was in main (PR #197 closed; landed squashed via another PR — branch tip vs `origin/main` differed only by version number, main ahead). Checked out main, fast-forwarded. Deleted `chore/rust-toolchain-1.96` local+remote (PR #198 merged). Left `feat/axon_rust-8mu8` worktree alone (had ~188 files of unmerged file-ingest-engine work at the time).
8. **"Patch and push" → already fixed upstream.** Found `memory_settings()` and `maybe_patch_memory_settings()` in `src/vector/ops/tei/qdrant_store.rs` (landed in `49ad61f5` at 05:20 the same day) already implement exactly the applied settings as defaults, including auto-reconcile of diverged collections. The 18-hour-old `axon` container was running a pre-fix binary.
9. **Binary refresh.** `cargo build --bin axon` (v5.8.1), `docker restart axon` — healthy. Restored `plugins/axon/bin/axon` (tracked 260 MB plugin binary, hardlinked to `target/debug/axon`, dirtied by the build; release pipeline owns syncing it). Nothing committed or pushed — no source change existed to make.

## Key Findings

- **Root cause was two-layer:** `hnsw_config.on_disk: true` (graph traversal from disk) *and* `quantization.scalar.always_ram: false` (distance calculations from disk). Fixing only HNSW left 5–32 s latency; both were required.
- **Qdrant "grey" status after a config PATCH means pending-but-not-running optimizations** — it needs `PATCH {"optimizers_config":{}}` to kick the optimizer. Without the nudge, the rebuild never starts and memory never climbs.
- **The fix already existed on main**: `src/vector/ops/tei/qdrant_store.rs:310` (`memory_settings()`, defaults HNSW-in-RAM + `always_ram: true`, env overrides `AXON_QDRANT_HNSW_ON_DISK` / `AXON_QDRANT_QUANTIZATION_ALWAYS_RAM`) and `:457` (`maybe_patch_memory_settings()`, auto-reconciles existing collections). Landed in `49ad61f5` (2026-06-10 05:20). No env overrides set in `~/.axon/.env`.
- **The container runtime lags the repo**: `axon:dev-runtime` bind-mounts `target/debug`; a fix merged to main does nothing until someone rebuilds the debug binary *and* restarts the container.
- **`plugins/axon/bin/axon` is hardlinked to `target/debug/axon`** (link count 2) — every `cargo build` dirties this tracked 260 MB binary. Last synced intentionally in `49ad61f5` / PR #193's release-pipeline work.
- **PR #197 (`feat/qdrant-affinity-tei-burst`) closed-not-merged is benign** — its content landed squashed in main; the branch tip differed from `origin/main` only by `Cargo.toml` version (5.7.9 vs 5.8.0).

## Technical Decisions

- **int8 `always_ram` over raw-f32-in-RAM**: full f32 vectors need ~19 GB (over the 16 GiB container cap); int8 quantized are ~4.7 GiB and rescoring reads only the top candidates' f32 vectors from disk. Matches the documented "memory-bounded large-collection recipe" comment in `ensure_collection`.
- **Live PATCH instead of code change**: settings persist in Qdrant's collection config across restarts, and main's `maybe_patch_memory_settings()` now keeps them reconciled — so hand-patching the live collection plus refreshing the running binary was the complete fix.
- **Did not commit the rebuilt plugin binary**: pushing a 260 MB blob refresh was not the requested work; the release pipeline owns that sync. Restored it with `git restore`.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `docs/sessions/2026-06-10-qdrant-latency-tuning-hnsw-ram-quantization.md` | — | this session log | this file |

No source files were changed. `plugins/axon/bin/axon` was transiently dirtied by `cargo build` (hardlink) and restored via `git restore`; `git status` clean afterward.

## Beads Activity

- No beads created, closed, claimed, or commented during the session (ops/diagnostics work; the upstream code fix pre-existed under its own bead, `axon_rust-o9y2`, referenced in `memory_settings()` doc comments).
- One `bd remember` entry recorded: `qdrant-axon-collection-latency-tuning-2026-06-10` — captures the two-layer root cause, the grey-status optimizer nudge, the rebuild+restart requirement, and the plugin-binary hardlink gotcha.

## Repository Maintenance

- **Branches**: deleted `chore/rust-toolchain-1.96` local + remote (evidence: PR #198 `MERGED` via `gh pr list`). `feat/qdrant-affinity-tei-burst` was already absent locally and remotely (delete attempts returned "not found"; PR #197 closed with content squashed into main, verified by `git cherry` + content diff). `feat/axon_rust-8mu8` was deliberately left alone mid-session (unmerged work, no PR); by session end it and its worktree no longer exist (`git worktree list` / `git branch -a` show only `main`) — removed externally, not by this session. Main also advanced externally `b4f5cf4d` → `c2579fe3` (PR #200) during the session.
- **Plans**: no plan files touched by this session; none moved. The injected "active plan" pointer references the deprecated `~/workspace/axon_rust` copy and was ignored.
- **Stale docs**: none found stale by this session's work; `src/vector/CLAUDE.md` and the `qdrant_store.rs` comments already document the memory recipe. No-op.
- **Working tree**: clean at session end (`git status --short` empty), HEAD `c2579fe3` matching `origin/main`.

## Tools and Skills Used

- **Bash / curl / jq**: all Qdrant REST diagnostics and PATCHes, docker stats/logs/restart, git/gh branch forensics, cargo build. No failures beyond expected ones noted in Errors.
- **Background tasks (Monitor-style until-loops)**: two watchers polling collection status back to green; both completed and triggered re-tests.
- **MCP — lumen semantic search + octocode localSearchCode**: located `ensure_collection` / `memory_settings` / `maybe_patch_memory_settings` in `src/vector/ops/tei/qdrant_store.rs`.
- **gh CLI**: PR merge-state verification for #197 and #198.
- **bd (beads)**: `bd remember` for the durable insight.
- **Skill**: `vibin:save-to-md` (this artifact).

## Commands Executed

| command | result |
|---|---|
| `docker stats --no-stream axon-qdrant` | 5.84 GiB/16 GiB before; 10.27 GiB after rebuilds; 9.90 GiB at close |
| `POST /collections/axon/points/query` (by-ID, dense, ×5 fresh points) | before: 12.3–41.7 s; after both fixes: 0.22–2.1 s |
| `PATCH /collections/axon {"hnsw_config":{"on_disk":false}}` | ok; status yellow → grey (stalled) |
| `PATCH /collections/axon {"optimizers_config":{}}` | kicked optimizer; yellow → rebuild ran (27→14 segments) |
| `PATCH /collections/axon {"quantization_config":{"scalar":{...,"always_ram":true}}}` | ok; rebuild → green |
| `gh pr list --head feat/qdrant-affinity-tei-burst --state all` | PR #197 CLOSED, mergedAt null |
| `git cherry origin/main HEAD` + content diff | only toolchain/version commits differed; file contents identical |
| `git checkout main && git pull --ff-only` | fast-forwarded to b4f5cf4d (later advanced externally to c2579fe3) |
| `git push origin --delete chore/rust-toolchain-1.96` | deleted (PR #198 merged) |
| `cargo build --bin axon` | Finished dev profile, axon v5.8.1, 1m 17s |
| `docker restart axon` | healthy in 8 s; serve + workers + OAuth router started |

## Errors Encountered

- **Qdrant status stuck `grey` after the HNSW PATCH** — optimizer registered the pending change but never started; resolved with an empty `optimizers_config` PATCH.
- **First fix insufficient** — HNSW-in-RAM alone still left 5–32 s queries; root cause was the quantized vectors also being disk-resident (`always_ram: false`).
- **`git branch -D feat/qdrant-affinity-tei-burst` → "not found"** — branch already auto-cleaned when its PR closed; no action needed.
- **`cargo build` dirtied tracked `plugins/axon/bin/axon`** via hardlink — restored with `git restore`.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Fresh dense query, `axon` collection | 12–42 s | 0.2–2.1 s |
| Repeated query | 34 ms | ~30 ms (unchanged) |
| Collection config | `hnsw on_disk: true`, `always_ram: false` | `hnsw on_disk: false`, `always_ram: true` (persisted in Qdrant) |
| `axon-qdrant` RSS | 5.8 GiB (cold) | ~9.9–10.3 GiB of 16 GiB |
| Running `axon` container | pre-`49ad61f5` binary (no auto-reconcile) | current main v5.8.1 (self-heals memory settings via `ensure_collection`) |
| Local checkout | detached on defunct feature branch history | `main` @ `c2579fe3`, only branch remaining |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `GET /collections/axon` after rebuilds | green, `hnsw_on_disk: false`, `always_ram: true` | exactly that | pass |
| 5 fresh by-ID dense queries post-fix | sub-second-ish | 2.11 s, 0.66 s, 0.31 s, 0.30 s, 0.22 s | pass |
| `docker stats axon-qdrant` | ~10–11 GiB (predicted) | 10.27 GiB | pass |
| `docker ps` after `docker restart axon` | healthy | `Up 8 seconds (healthy)` | pass |
| `git status --short` at close | clean | clean | pass |
| `git diff HEAD origin/main -- rust-toolchain.toml` before switch | no content difference | only `Cargo.toml` version differed | pass |

## Risks and Rollback

- Memory headroom: collection now holds ~10 GiB of a 16 GiB cap with 4.5 M points; continued index growth will push toward the cap (prior OOM-crashloop history at 12 G/16 G caps). Watch `docker stats` as ingest continues.
- Rollback: `PATCH /collections/axon` with `{"hnsw_config":{"on_disk":true}}` and `always_ram: false`, or set `AXON_QDRANT_HNSW_ON_DISK=true` / `AXON_QDRANT_QUANTIZATION_ALWAYS_RAM=false` in the env (the code's "lever B" — trades ~5–6 GiB RSS for latency). The collection rebuild is automatic after the PATCH (nudge optimizers if grey).

## Decisions Not Taken

- **Raw f32 vectors in RAM** — needs ~19 GB, exceeds the container cap.
- **Committing/pushing the rebuilt `plugins/axon/bin/axon`** — release pipeline owns the plugin-binary sync; a 260 MB blob refresh was out of scope.
- **Source patch for collection defaults** — already present on main (`49ad61f5`); writing it again would have been a no-op.

## Open Questions

- Whether `maybe_patch_memory_settings()` has since executed in the restarted container (it runs on the next `ensure_collection()` call, i.e. next embed/query through the server — settings currently match, so it will no-op either way).
- Who/what removed the `feat/axon_rust-8mu8` worktree and advanced main to `c2579fe3` mid-session (external to this session; presumed another session or the user).

## Next Steps

- None required for the latency fix — it is live, persisted, and self-healing under the new binary.
- Optional: keep an eye on `axon-qdrant` RSS as the collection grows past ~5 M points; the next pressure-relief levers are documented in the `qdrant-oom-crashloop` memory and the `memory_settings()` doc comment ("lever B").
- If plugin-binary distribution matters soon, run the release pipeline so `plugins/axon/bin/axon` catches up to v5.8.1+ (it currently reflects `49ad61f5`).
