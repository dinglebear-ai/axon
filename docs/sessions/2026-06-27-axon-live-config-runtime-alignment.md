---
date: 2026-06-27 02:17:21 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 2f4d56bb
working_directory: /home/jmagar/workspace/axon
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/34fb82ca-bbd6-4c0c-9a6a-2a467ee97e15.jsonl
---

# Axon Live Config and Runtime Alignment

## Summary

Reviewed and repaired Axon's live configuration/runtime path so the installed `axon`
command, Docker services, and `~/.axon` configuration all point at the current main
checkout instead of stale worktree artifacts. The live `.env` and `config.toml` were
backed up first, then updated with the intended split: URLs, secrets, and bootstrap
runtime in `.env`; feature and performance knobs in `config.toml`.

The session also confirmed that the Linux Containers scrape failure was not a
redirect-following bug. `https://linuxcontainers.org/distrobuilder/docs` redirects
to `/distrobuilder/docs/`, and that target returns HTTP 403. The canonical docs URL,
`https://linuxcontainers.org/distrobuilder/docs/latest/`, scrapes successfully.

## Starting State

- Repo: `/home/jmagar/workspace/axon`
- Branch: `main`
- Starting HEAD: `2f4d56bb` (`Merge pull request #282 from jmagar/codex/axon-config-toml-main`)
- Git status before saving this log: clean, `## main...origin/main`
- Active PR: none
- Worktree for main: `/home/jmagar/workspace/axon 2f4d56bb [main]`

## Work Completed

- Backed up the live configuration files before editing:
  - `/home/jmagar/.axon/.env.bak-20260626-235031`
  - `/home/jmagar/.axon/config.toml.bak-20260626-235031`
- Removed the bad live override:
  - `AXON_DEV_TARGET_DIR=/home/jmagar/workspace/axon/.worktrees/axon-freshness-schedules/target/release-fast`
- Rebuilt and realigned the installed command path so `axon` uses the normal main checkout binary:
  - `/home/jmagar/workspace/axon/target/debug/axon`
  - `axon --version` reported `axon 6.1.0`
- Recreated the Docker `axon` service so Compose uses the normal `./target/debug` bind mount again.
- Recreated `axon-tei` after discovering it was still running with stale TEI batch sizing.
  - Verified the recreated command includes `--max-client-batch-size 256`.
  - `axon doctor` reported TEI `max_client_batch_size=256`.
- Started/verified the runtime service set:
  - `axon`
  - `axon-tei`
  - `axon-chrome`
- Confirmed current feature support through `axon doctor`, including:
  - SQLite jobs DB at `/home/jmagar/.axon/jobs.db`
  - Qdrant at `http://100.120.242.29:53333`
  - TEI at `http://127.0.0.1:52000`
  - Chrome management endpoint at `http://127.0.0.1:6000/`
  - Gemini headless command validation
  - crawl, extract, embed, and ingest pipeline probes
- Confirmed the Linux Containers docs scrape behavior:
  - `/distrobuilder/docs` redirects to `/distrobuilder/docs/`
  - `/distrobuilder/docs/` returns HTTP 403
  - `/distrobuilder/docs/latest/` returns HTTP 200 and scrapes successfully

## Files and Runtime State Changed

- `/home/jmagar/.axon/.env`
  - Live runtime/bootstrap configuration updated.
  - Bad worktree target override removed.
- `/home/jmagar/.axon/config.toml`
  - Live feature/performance configuration aligned with the implemented config surface.
- `/home/jmagar/.local/bin/axon`
  - Realigned to the current main checkout binary path.
- `/home/jmagar/workspace/axon/target/debug/axon`
  - Rebuilt from main.
- Docker runtime state
  - `axon`, `axon-tei`, and `axon-chrome` recreated or restarted as needed.
- `/home/jmagar/workspace/axon/docs/sessions/2026-06-27-axon-live-config-runtime-alignment.md`
  - This session artifact.

## Verification Evidence

| Check | Result |
| --- | --- |
| `cargo build --bin axon` | Passed |
| `axon --version` | `axon 6.1.0` |
| `axon config list` | Parsed current config successfully |
| `docker compose ... up -d axon` | Axon service recreated from the normal main checkout path |
| `docker inspect axon-tei` | Command includes `--max-client-batch-size 256` |
| `axon doctor` | Overall pass; SQLite, Qdrant, TEI, Chrome, Gemini, and pipeline probes OK |
| `curl -I -L https://linuxcontainers.org/distrobuilder/docs` | 301 to `/distrobuilder/docs/`, then 403 |
| `curl -I -L https://linuxcontainers.org/distrobuilder/docs/latest/` | 200 |
| `axon scrape https://linuxcontainers.org/distrobuilder/docs/latest/` | Succeeded with indexing enabled |

## Errors Encountered

- `zsh: exec format error: axon`
  - Root cause was a bad/stale installed binary artifact. Rebuilt and repointed to the current main checkout.
- Config parse failures around newer TOML sections such as `[chunking]`
  - Caused by stale binary/config surface skew. Fixed by rebuilding and aligning the runtime binary.
- SQLite migration skew around freshness migrations
  - Caused by mixed binaries/worktree state. Fixed by using the merged main branch state.
- TEI connection/transport failures
  - Resolved by ensuring the `axon-tei` container was running and recreated with current settings.
- `axon` container unhealthy after config work
  - Caused by a mounted binary/config mismatch. Fixed by rebuilding main and recreating the service.
- Linux Containers scrape returned HTTP 403
  - Not a redirect-following bug; the redirected URL itself returns 403. The canonical `/latest/` URL works.

## Repository Maintenance

- Worktree state was inspected.
- `codex/axon-freshness-schedules` is merged into `main`; it remains as a cleanup candidate.
- `codex/env-config-drift-alignment` is not an ancestor of `main`, but its reworked replacement landed through PR #282.
- `codex/pull-agent-skills` has dirty local work and was left untouched.
- `_no_mcp_worktrees/axon` on `marketplace-no-mcp` is a protected long-lived variant and was left untouched.
- `.full-review` deletions that were previously discussed are not present in current `main`; git status is clean.
- Recent beads were reviewed; no relevant bead action was taken for this session.
- No broad plan-file moves were made during this save-only turn.

## Follow-Ups

- Persist the live dotfile changes into chezmoi when ready:
  - `chezmoi re-add ~/.axon/.env ~/.axon/config.toml`
- Cleanup candidate:
  - remove the merged `.worktrees/axon-freshness-schedules` worktree after confirming no local artifacts are still needed.
- Continue to keep `.env` limited to URLs, secrets, and runtime/bootstrap values; keep all feature and performance knobs in `config.toml`.
