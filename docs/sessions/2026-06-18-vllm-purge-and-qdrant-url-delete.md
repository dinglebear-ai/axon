---
date: 2026-06-18 19:35:50 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 39e36c95
session id: 69e9d346-4528-4a72-86f1-4dfb93a61d6c
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/69e9d346-4528-4a72-86f1-4dfb93a61d6c.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
---

# vLLM purge and Qdrant URL delete session

## User Request

The user asked to remove the vLLM China crawl/indexed content and make it easy to delete indexed Qdrant content by URL. The user also asked for current memory usage after killing the Axon container due to a 13 GB memory balloon.

## Session Overview

Implemented a new `axon purge <url>` CLI path in the working tree for deleting Qdrant points by URL or seed URL, with `--prefix`, `--dry-run`, `--yes`, JSON output, and aliases `delete-url` / `delete`. Used it live against `https://docs.vllm.com.cn/`, verified Qdrant already had zero matching points, removed local crawl artifacts and vLLM experiment files, deleted the vLLM China crawl rows from SQLite, and confirmed Axon remained stopped while TEI and Chrome stayed healthy.

## Sequence of Events

1. Reviewed the existing Qdrant delete path and found only exact `payload.url` deletion was available.
2. Added CLI/config plumbing for `purge`, a Qdrant scan-and-delete primitive, and unit tests for exact/prefix URL matching.
3. Ran focused compile/test checks, then attempted a release-fast build; the build was killed without compiler diagnostics while the Axon container was under memory pressure.
4. Switched to `cargo run` for the live command, ran `purge https://docs.vllm.com.cn/ --prefix --dry-run --json`, then ran the real purge with `--yes`.
5. Removed the local `docs.vllm.com.cn` output tree, deleted six matching crawl job rows, removed the vLLM sandbox under `~/.axon/vllm-test`, and deleted `docker-compose.vllm.yaml`.
6. Checked host memory after the container kill: RAM was not pressured, but swap remained heavily used.

## Key Findings

- `src/vector/ops/qdrant/client/delete.rs:76` had an exact URL helper, but no operator-facing whole-origin delete path.
- The new delete primitive scrolls only point IDs plus `url` and `seed_url`, filters matches in Rust, and deletes by exact point ID at `src/vector/ops/qdrant/client/delete.rs:97`.
- `axon purge` is exposed in clap with aliases at `src/core/config/cli.rs:80`.
- Live purge checks for `https://docs.vllm.com.cn/` returned `matched_points: 0` and `matched_url_count: 0`.
- The Axon container was stopped as `Exited (143)`, while `axon-tei` and `axon-chrome` stayed healthy.
- Current memory after cleanup was `20Gi used / 48Gi total`, `27Gi available`, and swap at `6.9Gi used / 8.0Gi`.

## Technical Decisions

- Used a point-ID delete instead of a Qdrant prefix text filter so deletion is deterministic and dry-run can report the same match set as the real delete.
- Treated `seed_url` as a first-class match field so a crawl origin can be removed even when stored page URLs differ from the seed.
- Added boundary-aware prefix matching so `https://docs.example.com/guide` does not match `https://docs.example.com/guide-old`.
- Kept old vLLM benchmark JSON files because they are historical benchmark evidence, not running deployments or indexed Qdrant content.
- Did not restart Axon after the memory balloon; the user had killed it and the cleanup could be completed host-side.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `CLAUDE.md` | | Documented `axon purge <url>` in the command table. | `git diff --stat` shows one line added. |
| deleted | `docker-compose.vllm.yaml` | | Removed the standalone vLLM compose deployment file. | `rg vllm` found the file; no `axon-vllm` container existed. |
| modified | `src/cli/commands.rs` | | Registered the new purge command module and runner export. | `git diff --name-status`. |
| created | `src/cli/commands/purge.rs` | | Added CLI wrapper, dry-run reporting, confirmation, and JSON output. | `src/cli/commands/purge.rs:9`. |
| modified | `src/core/config/cli.rs` | | Added `PurgeArgs`, `--prefix`, `--dry-run`, and aliases. | `src/core/config/cli.rs:80`. |
| modified | `src/core/config/parse/build_config/command_dispatch.rs` | | Wired clap args into `CommandKind::Purge` and config flags. | `git diff --name-status`. |
| modified | `src/core/config/parse/build_config/config_literal.rs` | | Copied purge flags into runtime `Config`. | `git diff --name-status`. |
| modified | `src/core/config/types/config.rs` | | Added `purge_prefix` and `purge_dry_run` fields. | `git diff --name-status`. |
| modified | `src/core/config/types/config_debug.rs` | | Included purge fields in debug output. | `git diff --name-status`. |
| modified | `src/core/config/types/config_impls.rs` | | Added default values for purge flags. | `git diff --name-status`. |
| modified | `src/core/config/types/enums.rs` | | Added `CommandKind::Purge`. | `git diff --name-status`. |
| modified | `src/lib.rs` | | Dispatched `CommandKind::Purge` to `run_purge`. | `git diff --name-status`. |
| modified | `src/vector/ops/qdrant.rs` | | Re-exported purge Qdrant functions to CLI layer. | `git diff --name-status`. |
| modified | `src/vector/ops/qdrant/client.rs` | | Re-exported new low-level delete result/function. | `git diff --name-status`. |
| modified | `src/vector/ops/qdrant/client/delete.rs` | | Added scan/dry-run/delete-by-URL implementation. | `src/vector/ops/qdrant/client/delete.rs:97`. |
| modified | `src/vector/ops/qdrant/client/delete_tests.rs` | | Added exact and prefix URL matching tests. | `src/vector/ops/qdrant/client/delete_tests.rs:205`. |
| created | `docs/sessions/2026-06-18-vllm-purge-and-qdrant-url-delete.md` | | Session artifact generated by `vibin:save-to-md`. | This file. |

## Beads Activity

No bead changes were made during this save run. The maintenance read of `.beads/interactions.jsonl` showed recent prior activity, including `axon_rust-o29l` and children closed on 2026-06-18, but no bead was created, edited, claimed, or closed for the purge implementation in this Codex turn.

## Repository Maintenance

### Plans

`find docs/plans -maxdepth 2 -type f` showed active-looking plans under `docs/plans/` and many already-completed plans under `docs/plans/complete/`. No plan files were moved because none were proven completed by this session.

### Beads

`bd list --all --sort updated --reverse --limit 100 --json` and `tail -200 .beads/interactions.jsonl` were read. No tracker mutation was performed because the save contract requires committing only the generated session artifact, and the directly relevant purge work is still uncommitted source changes.

### Worktrees and branches

`git worktree list --porcelain` showed the main worktree, `marketplace-no-mcp`, and three codex worktrees. No cleanup was performed because those worktrees/branches have unclear ownership or active branch state.

### Stale docs

`CLAUDE.md` was updated to mention the new `purge` command. Broader stale-doc cleanup was not attempted.

### Transparency

The functional purge changes remain dirty in the working tree. This save artifact is the only file staged/committed by the save workflow.

## Tools and Skills Used

- **Skill.** `vibin:save-to-md` was used to generate, commit, and push this session artifact.
- **Shell commands.** Used for git state, build/test verification, SQLite cleanup, Docker state, Qdrant purge execution, memory inspection, and filesystem cleanup.
- **File edits.** Used `apply_patch` to add/modify Rust source, remove the vLLM compose file, update `CLAUDE.md`, and write this session artifact.
- **External CLIs.** Used `cargo`, `git`, `sqlite3`, `docker`, `find`, `rg`, `free`, `swapon`, and `ps`.
- **Qdrant/TEI services.** Qdrant was accessed through Axon's configured client path; TEI was observed as a running container but not modified.
- **Beads.** Read-only maintenance inspection only.

## Commands Executed

| command | result |
|---|---|
| `cargo test -q url_target_match --locked` | Passed: 2 tests passed, 3187 filtered out. |
| `cargo check -q --bin axon --locked` | Passed after fixing CLI import/format issues. |
| `cargo build --profile release-fast --bin axon --locked` | Ended with no compiler diagnostics while memory pressure was present; stale binary did not contain `purge`. |
| `cargo run --quiet --bin axon -- purge https://docs.vllm.com.cn/ --prefix --dry-run --json` | Returned zero matching Qdrant points and URLs. |
| `cargo run --quiet --bin axon -- purge https://docs.vllm.com.cn/ --prefix --yes --json` | Completed and deleted zero points because no Qdrant matches remained. |
| `sqlite3 ~/.axon/jobs.db "delete from axon_crawl_jobs where url like '%docs.vllm.com.cn%'; select changes();"` | Deleted 6 crawl rows. |
| `rm -rf ~/.axon/output/domains/docs.vllm.com.cn ~/.axon/vllm-test` | Removed vLLM China crawl output and the vLLM experiment sandbox. |
| `docker ps -a --filter name=axon --format ...` | Showed `axon` stopped, `axon-tei` and `axon-chrome` healthy. |
| `free -h` | Showed `20Gi` used, `23Gi` free, `27Gi` available, `6.9Gi` swap used after cleanup. |

## Errors Encountered

- `cargo check` initially failed because `src/cli/commands/purge.rs` imported through the private `qdrant::client` module and one output string lacked a placeholder. Fixed by re-exporting through `qdrant.rs` and correcting the format string.
- The release-fast build exited without compiler diagnostics while the Axon container was under memory pressure. The binary was stale, proven by `./target/release-fast/axon purge --help` returning `unrecognized subcommand 'purge'`.
- A SQLite query used the wrong column names (`error`, then `error_message`). `.schema axon_crawl_jobs` showed the correct column is `error_text`.
- A broad `rg` over `~/.axon` produced very large output and found benchmark artifacts; those were not deleted because they are not indexed content.
- `memory-capture.sh` hook output reported a missing `lavra` plugin directory during transcript inspection; it was non-blocking.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Qdrant deletion | Operators had an exact URL helper but no easy CLI to purge by URL/seed URL/prefix. | `axon purge <url>` can preview or delete matching points; aliases `delete-url` and `delete` work through clap. |
| vLLM deployment | Repo had `docker-compose.vllm.yaml`; host had a `~/.axon/vllm-test` sandbox. | Compose file is deleted in the working tree; sandbox files are removed from disk. |
| vLLM China crawl residue | SQLite had six `docs.vllm.com.cn` crawl rows and local output directories. | SQLite has zero matching crawl rows and local `docs.vllm.com.cn` output is removed. |
| Axon runtime | Axon container ballooned and was killed by the user. | Axon remains stopped; TEI and Chrome remain healthy. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -q url_target_match --locked` | URL match tests pass. | 2 passed, 0 failed. | pass |
| `cargo check -q --bin axon --locked` | Binary compiles. | Passed after fixes. | pass |
| `cargo run --quiet --bin axon -- purge https://docs.vllm.com.cn/ --prefix --dry-run --json` | No Qdrant residue remains. | `matched_points: 0`, `matched_url_count: 0`. | pass |
| `sqlite3 ~/.axon/jobs.db "select count(*) from axon_crawl_jobs where url like '%docs.vllm.com.cn%';"` | No vLLM China crawl rows remain. | `0`. | pass |
| `test ! -e ~/.axon/output/domains/docs.vllm.com.cn` | Output artifacts removed. | `docs_vllm_cn_output_removed`. | pass |
| `test ! -e ~/.axon/vllm-test` | vLLM sandbox removed. | `vllm_test_removed`. | pass |
| `docker ps -a --filter name=axon-vllm` | No vLLM container exists. | No rows returned. | pass |
| `docker ps -a --filter name=axon --format ...` | Axon app remains stopped after user kill. | `axon` exited 143; TEI/Chrome healthy. | pass |

## Risks and Rollback

The purge implementation scans point IDs and payload metadata; very large collections may make `--dry-run` and delete operations take time because it avoids unsafe prefix filters. Rollback for source changes is to revert the uncommitted working-tree diff. Runtime cleanup rollback would require re-crawling/re-indexing `https://docs.vllm.com.cn/`; deleted local output and SQLite rows were intentionally removed.

## Decisions Not Taken

- Did not delete old vLLM benchmark JSON under `~/.axon/bench`; those are historical benchmark records, not indexed content or a running vLLM deployment.
- Did not restart Axon after the memory balloon; cleanup and verification were host-side and the user had intentionally killed the container.
- Did not remove active or ambiguous worktrees/branches during the save maintenance pass.

## References

- Transcript: `/home/jmagar/.claude/projects/-home-jmagar-workspace-axon/69e9d346-4528-4a72-86f1-4dfb93a61d6c.jsonl`
- Skill: `/home/jmagar/.codex/plugins/cache/dendrite/vibin/local/skills/save-to-md/SKILL.md`

## Open Questions

- The purge source changes are not committed or pushed yet; only this session artifact is committed by the save workflow.
- Axon remains stopped after the memory balloon. Restarting it should be a separate deliberate step after deciding whether to keep crawling paused.
- Swap remains materially used after cleanup; if the machine feels slow, swap clearing or reboot planning may be needed.

## Next Steps

1. Review, commit, and push the functional purge changes separately from this session artifact.
2. Decide whether to restart Axon and resume crawling after memory safeguards are satisfactory.
3. Consider adding a bounded/paged implementation for `axon purge` if delete-by-prefix becomes common on multi-million point collections.
