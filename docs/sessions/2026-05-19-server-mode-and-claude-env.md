---
date: 2026-05-19 22:31:08 EDT
repo: git@github.com:jmagar/axon.git
branch: main
head: ad5f714c
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust ad5f714c [main]
---

# Axon Server Mode, Dev Wrapper, Env Loader, and Claude Env File Session

## User Request

The session started around Axon bead triage and server-mode architecture, then narrowed into concrete fixes and documentation: server-routed vs local behavior, REST parity, extract output, dev-wrapper rebuild behavior, dotenv loading, and finally setting `CLAUDE_ENV_FILE` in project-local Claude settings.

## Session Overview

- Audited current server-routed vs local CLI/MCP behavior and captured the desired future shape in specs, a routing contract, and a Superpowers implementation plan.
- Fixed the dotenv-loading bug that created a repo-root `=12.2` file when `.env` contained shell metacharacters such as `NVIDIA_REQUIRE_CUDA=cuda>=12.2`.
- Changed the dev wrapper direction from rebuilding the container for normal Rust edits toward installing the debug binary into `~/.axon/dev/axon` and using the dev compose entrypoint.
- Tested `axon scrape https://code.claude.com/docs/en/env-vars` and used the scraped Claude docs to validate the `CLAUDE_ENV_FILE` behavior.
- Added `.claude/settings.json` env wiring so Claude should set `CLAUDE_ENV_FILE` for this repo.

## Sequence of Events

1. Reviewed open/in-progress Beads at epic level and discussed whether already-addressed work should be closed.
2. Investigated extract behavior, including deterministic parser precedence vs LLM fallback, server-routed `--wait true --json` output, and provenance visibility.
3. Mapped Axon routing gaps: CLI server mode still used `/v1/actions` broadly, MCP stdio ran the service layer in-process, and direct REST routes did not yet expose all service knobs.
4. Drafted and moved server-mode docs into `docs/specs` and `docs/contracts`, then created an execution plan under `docs/superpowers/plans`.
5. Fixed and verified safe dotenv loading through `scripts/lib/axon-env.sh`, removing the root cause of the `=12.2` artifact.
6. Scraped Anthropic Claude Code env-var docs with Axon and then configured `.claude/settings.json` to set `CLAUDE_ENV_FILE`.

## Key Findings

- `/v1/actions` is an HTTP dispatcher using the MCP-shaped `AxonRequest` action/subaction envelope; it is not where business logic should live long-term.
- Direct REST routes were thinner than the action envelope in places, so the contract now requires canonical service request types and REST parity before removing `/v1/actions`.
- Stdio MCP currently can run a full local Axon runtime because it constructs `ServiceContext` and calls service handlers in-process; future behavior should become server-first when `AXON_SERVER_URL` is set, with safe local fallback.
- The `=12.2` file was caused by shell-sourcing an env file containing `>` redirection syntax, not by Axon itself writing that filename.
- Claude Code documents `CLAUDE_ENV_FILE` as a path to a shell script run before Bash commands; Claude settings support an `env` block for startup environment variables.

## Technical Decisions

- Keep extract default mode as `auto`: deterministic extraction first, LLM fallback only when deterministic extraction yields nothing. `both` should be explicit because it adds cost and merge/conflict semantics.
- Treat `/v1/actions` as a hard-cutover removal target, not a compatibility surface, because there are no external users requiring backwards compatibility.
- Define server mode around capability tiers, route metadata, stable artifact handles, and explicit fallback policy rather than only checking whether `AXON_SERVER_URL` exists.
- Use the shared safe dotenv loader for Claude’s env hook instead of sourcing `.env` directly, so shell metacharacters remain literal env values.
- For dev container freshness, prefer installing/copying the debug binary into the dev runtime over rebuilding the full Docker image for normal Rust source edits.

## Files Modified

- `.claude/settings.json` - added an `env` block setting `CLAUDE_ENV_FILE` to `/home/jmagar/workspace/axon_rust/.claude/claude-env.sh`.
- `.claude/claude-env.sh` - added a repo-local script that sources `scripts/lib/axon-env.sh` and calls `load_axon_env_file`.
- `scripts/lib/axon-env.sh` - safe dotenv parser that avoids evaluating shell metacharacters.
- `scripts/axon` - dev wrapper changes for debug binary installation/container sync behavior.
- `docker-compose.dev.yaml` - dev compose entrypoint points at the installed debug binary path.
- `docs/specs/server-mode-capability-tiers.md` - server-mode capability tiers and feature/command expectations.
- `docs/contracts/server-mode-routing-contract.md` - canonical routing, request, artifact, fallback, auth, JSON envelope, and cutover contract.
- `docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md` - detailed Superpowers execution plan.
- `docs/sessions/2026-05-19-server-mode-and-claude-env.md` - this session note.

## Commands Executed

- `axon scrape https://code.claude.com/docs/en/env-vars --wait true --render-mode http --skip-embed` succeeded and returned the Claude Code env-var documentation content.
- `axon scrape ... --embed false` failed because the installed CLI supports `--skip-embed`, not `--embed false`, on that path.
- `axon --help` and `axon scrape --help` were used to confirm the supported global and scrape flags.
- `bash -n scripts/lib/axon-env.sh` and `bash -n scripts/axon` verified shell syntax for wrapper/env changes.
- `docker compose -f docker-compose.yaml -f docker-compose.dev.yaml config --services` verified compose config after dev-wrapper changes.
- `jq -e '.env.CLAUDE_ENV_FILE == "/home/jmagar/workspace/axon_rust/.claude/claude-env.sh"' .claude/settings.json` verified the Claude setting.
- `bash -n .claude/claude-env.sh` verified the Claude env file script syntax.
- `bash -lc 'rm -f ./=12.2; source .claude/claude-env.sh ...; test ! -e ./=12.2'` verified the Claude env hook did not recreate the bad file.

## Errors Encountered

- A search pattern containing shell backticks accidentally started `scripts/axon`; the stray cargo build was stopped.
- `axon scrape ... --embed false` failed because this installed CLI uses `--skip-embed`; rerunning with `--skip-embed` worked.
- `axon ask --follow-up "can we set CLAUDE_ENV_FILE in .claude/settings.json?"` returned an answer, but citation validation failed, so it was treated as non-authoritative until the docs were scraped directly.
- The `=12.2` artifact was reproduced and traced to unsafe shell evaluation of `NVIDIA_REQUIRE_CUDA=cuda>=12.2`; the safe parser fixed it.

## Behavior Changes Before/After

| Area | Before | After |
| --- | --- | --- |
| Env loading | `.env` could be shell-evaluated, allowing `>` to create `=12.2`. | Env values are parsed literally and exported without shell evaluation. |
| Claude Bash env | Project settings did not set `CLAUDE_ENV_FILE`. | `.claude/settings.json` sets it to a repo-local script that loads Axon env safely. |
| Dev container freshness | Normal Rust edits could trigger full image rebuild behavior. | Dev direction now installs the debug binary and uses a dev compose entrypoint for faster code updates. |
| Server-mode docs | Routing, fallback, and capability tiers were scattered through discussion. | Specs, contract, and implementation plan now capture the intended architecture. |

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `bash -n scripts/lib/axon-env.sh` | Shell syntax valid | Passed | Pass |
| `bash -n scripts/axon` | Shell syntax valid | Passed | Pass |
| temp dotenv load with `NVIDIA_REQUIRE_CUDA=cuda>=12.2` | No `=12.2` file created | Passed | Pass |
| `test ! -e '=12.2'` | Bad artifact absent | Passed | Pass |
| `docker compose -f docker-compose.yaml -f docker-compose.dev.yaml config --services` | Compose config valid | Passed | Pass |
| `axon scrape https://code.claude.com/docs/en/env-vars --wait true --render-mode http --skip-embed` | Scrape real page content | Succeeded | Pass |
| `jq -e '.env.CLAUDE_ENV_FILE == "/home/jmagar/workspace/axon_rust/.claude/claude-env.sh"' .claude/settings.json` | Setting present | `true` | Pass |
| `bash -n .claude/claude-env.sh` | Shell syntax valid | Passed | Pass |
| source `.claude/claude-env.sh` with `=12.2` removed first | No stdout/stderr and no `=12.2` recreation | Passed | Pass |

## Risks and Rollback

- `.claude/settings.json` now depends on an absolute repo path; moving the checkout would require updating that value.
- The Claude docs imply settings `env` can set environment variables and document `CLAUDE_ENV_FILE`, but a live Claude Code startup test was not completed in this session.
- Rollback for the Claude env change is to remove the `env.CLAUDE_ENV_FILE` entry from `.claude/settings.json` and delete `.claude/claude-env.sh`.
- Rollback for server-mode planning docs is to remove the spec/contract/plan files; no runtime behavior depends on them yet.

## Decisions Not Taken

- Did not make `extract --extract-mode both` the default; it should remain explicit because it costs more and needs result-merge semantics.
- Did not preserve `/v1/actions` as a long-term compatibility endpoint; the planned path is hard cutover to direct REST parity.
- Did not move all CLI/MCP operations to remote-only behavior; local service-layer execution remains useful for degraded and offline operation.
- Did not collapse FastEmbed and Qdrant into a single-container implementation in this session; it was acknowledged as out of scope until server-mode routing is cleaned up.

## References

- `docs/specs/server-mode-capability-tiers.md`
- `docs/contracts/server-mode-routing-contract.md`
- `docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md`
- `scripts/lib/axon-env.sh`
- `.claude/settings.json`
- `.claude/claude-env.sh`
- `https://code.claude.com/docs/en/env-vars`

## Open Questions

- Does Claude Code read `CLAUDE_ENV_FILE` from project `.claude/settings.json` early enough for the Bash tool in a fresh Claude session? The docs support the shape, but a fresh-process test was not completed.
- Which currently dirty app/web files are user work vs generated output? They were not part of this session note except as ambient dirty-tree context.
- Should the dev-wrapper debug-binary sync be extended with a stronger container existence/restart health check?

## Next Steps

- Started but not completed: execute the Superpowers plan in `docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md`.
- Started but not completed: run a fresh Claude Code process test to prove `.claude/settings.json` can set `CLAUDE_ENV_FILE` for Bash commands.
- Not yet started: implement canonical service request types, route metadata, stable artifact handles, local reconciliation, direct REST parity, stdio MCP thin-client mode, and `/v1/actions` removal.
- Not yet started: upgrade `doctor` into a capability/remedy-oriented report, with a possible `doctor diagnose` LLM-assisted follow-up.
