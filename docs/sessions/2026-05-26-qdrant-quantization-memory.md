---
date: 2026-05-26 21:06:06 EST
repo: git@github.com:jmagar/axon.git
branch: feat/openai-compat-palette-polish
head: a1d44643
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
pr: #139 feat: add OpenAI-compatible backend and palette polish (https://github.com/jmagar/axon/pull/139)
---

# Qdrant quantization and memory follow-up

## User Request

Investigate whether Qdrant was configured to avoid keeping everything in RAM, fix the `always_ram` setting, check memory usage, and monitor whether Qdrant optimization was still running.

## Session Overview

The session confirmed that Qdrant already used on-disk payloads, on-disk dense vectors, and on-disk HNSW, but Axon-created collections still pinned scalar quantized vectors in RAM with `always_ram=true`. That setting was changed to `false` for future collections, existing live Qdrant collections were patched, and optimizer state was monitored until the main `axon` collection resumed active optimization.

## Sequence of Events

1. Inspected `docker-compose.prod.yaml`, `config/qdrant/production.yaml`, and live Qdrant collection metadata.
2. Identified `src/vector/ops/tei/qdrant_store.rs` as the Axon collection creation path that set `quantization_config.scalar.always_ram=true`.
3. Updated the quantization tests first, observed the expected failing test state, then changed production code to emit `always_ram=false`.
4. Patched the live `axon` collection and all other live collections that still reported `always_ram=true`.
5. Checked host memory, Docker memory, Qdrant collection state, shard state, logs, and optimizer status while Qdrant rebuilt/optimized.
6. When `axon` moved to `grey`, triggered Qdrant optimizers with a no-op collection update; the collection moved back to `yellow`.

## Key Findings

- `config/qdrant/production.yaml` already sets `storage.on_disk_payload: true`, `storage.hnsw_index.on_disk: true`, and default collection vectors `on_disk: true`.
- Live `axon` collection confirmed dense vector `on_disk=true`, payload `on_disk_payload=true`, HNSW `on_disk=true`, and quantization `always_ram=false` after patching.
- `grey` collection status meant optimizations were pending but not triggered; a no-op `PATCH /collections/axon` with `{"optimizers_config":{}}` resumed active optimization.
- Host RAM was not exhausted during checks: examples included `57Gi` total, `28Gi` to `33Gi` available, and swap usage ranging from `0B` to `805Mi`.
- Qdrant remained the largest resident process, dropping from roughly `10.4GiB` to `8.6-9.0GiB` during later checks, while CPU activity showed optimizer work.

## Technical Decisions

- Use `always_ram=false` for scalar quantization so Linux page cache and NVMe-backed mmap can keep hot data warm without pinning quantized vectors permanently in Qdrant RAM.
- Leave `vectors.on_disk=true`, `on_disk_payload=true`, and `hnsw_index.on_disk=true` unchanged because they already match the desired memory model.
- Patch existing live collections because changing the code only affects newly created collections.
- Trigger optimizers with a no-op collection update rather than restarting Qdrant, because Qdrant documents this as the recovery path for grey collections.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `src/vector/ops/tei/qdrant_store.rs` |  | New collection creation now sends `always_ram=false`. | `git show --stat --name-only HEAD` includes this file in `a1d44643`. |
| modified | `src/vector/ops/tei/qdrant_store_tests.rs` |  | Regression tests now assert `always_ram=false`. | Targeted tests passed. |
| modified | `docs/contracts/qdrant-payload-schema.md` |  | Current contract documents scalar int8 quantization with `always_ram=false`. | `rg` found no active-code/current-contract `always_ram=true` hits after update. |
| created | `docs/sessions/2026-05-26-qdrant-quantization-memory.md` |  | Session documentation artifact. | This file. |

## Beads Activity

No bead activity observed. `bd list --all --sort updated --reverse --limit 20 --json` returned historical closed issues; no session-specific bead was created, edited, claimed, or closed.

## Repository Maintenance

### Plans

Checked `docs/plans/` and `docs/plans/complete/`. No plan file was clearly completed by this Qdrant memory follow-up, so no plan moves were made.

### Beads

Read recent beads with `bd list`. No directly relevant active bead was found or changed.

### Worktrees and branches

Inspected `git worktree list --porcelain`, local branches, and remote branches. Existing worktrees `feat/axon-android-app` and `work/palette-streamdown-streaming` were active and not removed. No branch cleanup was safe or relevant.

### Stale docs

The current Qdrant payload contract doc had already been updated in HEAD to `always_ram=false`. Historical session and plan notes still mention the old value, but those were left untouched as historical records.

## Tools and Skills Used

- **Skills.** Used `axon` for project/runtime context, `glances` for system monitoring context, `superpowers:test-driven-development` for the behavior change, `superpowers:verification-before-completion` before reporting completion, and `save-to-md` for this artifact.
- **Shell commands.** Used `rg`, `sed`, `curl`, `jq`, `docker`, `ps`, `free`, `vmstat`, `git`, and `bd`.
- **Web search.** Consulted Qdrant documentation for collection status and grey-status recovery behavior.
- **File tools.** Used `apply_patch` to modify code/tests/docs and to create this session note.

## Commands Executed

| command | result |
| --- | --- |
| `curl http://127.0.0.1:53333/collections/axon` | Confirmed live collection status/config, including `always_ram=false`. |
| `curl -X PATCH http://127.0.0.1:53333/collections/axon --data-raw '{"quantization_config":{"scalar":{"type":"int8","quantile":0.99,"always_ram":false}}}'` | Patched the live `axon` collection quantization config. |
| `curl -X PATCH http://127.0.0.1:53333/collections/axon --data-raw '{"optimizers_config":{}}'` | Triggered pending optimizers and moved collection from `grey` to `yellow`. |
| `docker stats --no-stream axon-qdrant` | Showed Qdrant memory/CPU during optimization. |
| `free -h` | Showed host memory availability and swap usage. |
| `cargo test -p axon -- ensure_collection_sends_quantization_config_on_create ensure_collection_sends_full_create_body_with_hnsw_and_quantization` | Targeted tests passed after code change. |
| `cargo fmt --check` | Formatting check passed. |

## Errors Encountered

- Initial regression test run failed as intended after changing tests to expect `always_ram=false`; production code still emitted `true`.
- One `rg` command had a quoting error (`zsh: unmatched "`); it was rerun with corrected quoting.
- Qdrant entered `grey` status after patching; docs indicated this means pending optimizations are paused. A no-op optimizer update resumed optimization and changed status back to `yellow`.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| New Axon-created Qdrant collections | Scalar quantized vectors were pinned with `always_ram=true`. | Scalar quantized vectors are configured with `always_ram=false`. |
| Existing live Qdrant collections | Many collections still reported `always_ram=true`. | Verification scan reported `0` collections with `always_ram=true`. |
| Main `axon` collection optimizer state | Moved from `yellow` to paused `grey`. | No-op optimizer trigger moved it back to active `yellow`. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test -p axon -- ensure_collection_sends_quantization_config_on_create ensure_collection_sends_full_create_body_with_hnsw_and_quantization` | Both quantization create-body tests pass. | `2 passed; 0 failed`. | PASS |
| `cargo fmt --check` | No formatting diffs. | Exited successfully with no output. | PASS |
| Live scan for `quantization_config.scalar.always_ram == true` | No collections remain pinned. | `0`. | PASS |
| `curl /collections/axon` after optimizer trigger | Optimizer resumes from grey. | `status=yellow optimizer=ok`. | PASS |

## Risks and Rollback

- Search latency may increase for cold data because quantized vectors are no longer pinned in RAM. Rollback is to patch affected collections back to `always_ram=true` and restore the code/tests/docs, but that reintroduces higher resident memory pressure.
- The main `axon` collection was still optimizing at the latest check. Completion should be monitored until `status=green`.

## Decisions Not Taken

- Did not restart Qdrant to resolve `grey`; Qdrant documentation recommends triggering optimizers via the UI or update operation.
- Did not rewrite historical session notes that mention `always_ram=true`; they record old behavior and were not current contracts.

## References

- Qdrant collections documentation: https://qdrant.tech/documentation/concepts/collections/
- Qdrant fundamentals FAQ for grey collection status: https://qdrant.tech/documentation/faq/qdrant-fundamentals/

## Open Questions

- Whether the latency impact of `always_ram=false` is acceptable under real Axon query load after the optimizer finishes.
- Whether a small helper script or Axon ops command should be added to report Qdrant grey/yellow/green state and trigger optimizers intentionally.

## Next Steps

- Continue polling `GET /collections/axon` until `status=green`.
- If `axon` returns to `grey`, run `PATCH /collections/axon` with `{"optimizers_config":{}}` again.
- After optimization completes, run representative `axon query` or `axon ask` checks to compare latency before deciding whether further Qdrant tuning is needed.
