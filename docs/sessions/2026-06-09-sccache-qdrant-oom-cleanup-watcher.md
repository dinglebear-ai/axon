---
date: 2026-06-09 13:14:38 EST
repo: git@github.com:jmagar/axon.git
branch: fix/mcp-informative-errors
head: df3ca011
session id: a66acb5c-15f0-4458-aaef-393db7b4e8d9
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/a66acb5c-15f0-4458-aaef-393db7b4e8d9.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: 194 — fix(mcp): include error cause in query-family MCP responses — https://github.com/jmagar/axon/pull/194
beads: none
---

# sccache throttle, Qdrant OOM crashloop, collection cleanup, restart watcher, 5.5.4 release

## User Request
Started as venting about sccache repeatedly printing "server looks like it shut down unexpectedly, compiling locally." Expanded into: diagnose/fix that, answer whether ZFS→XFS is feasible, fix the underlying memory pressure, delete junk Qdrant collections, build a container-restart→gotify watcher, then `/vibin:quick-push` the resulting changes and save a session doc.

## Session Overview
Diagnosed and fixed two distinct memory-pressure failures on dookie that were *not* what they appeared to be: an sccache "crash" that was actually a cgroup soft-throttle stall, and a Qdrant 11.9G RSS "memory hog" that was actually a 206-restart OOM crashloop against a too-small container cap. Raised both limits live without data loss, garbage-collected 38 junk Qdrant collections (75→37), built a persistent docker-restart→gotify watcher, and shipped the in-repo infra/doc changes as axon 5.5.4.

## Sequence of Events
1. Investigated sccache: server up 6 days, but `memory.events` showed `high=102841` throttle hits against `MemoryHigh=12G`; root-caused as cgroup soft-throttle stalls (not crashes) and raised the soft cap to 15G live; captured the drop-in into chezmoi.
2. User asked if ZFS→XFS was easy; checked ARC (capped at 4G, innocent) and found Qdrant at 11.9G RSS — pivoted to Qdrant.
3. Inspected the `axon` collection (already on-disk + int8 quant) and found `axon-qdrant` had `RestartCount=206`, OOM-killed by its own 12G container cap (`dmesg CONSTRAINT_MEMCG`, anon-rss ~12.5G).
4. Raised the Qdrant container limit 12G→16G in `docker-compose.prod.yaml`, recreated the container (RestartCount→0, collection intact green).
5. Inventoried 75 collections; deleted 21 test/CI artifacts (5,689 pts) then 17 dead ≤2-point stubs → 37 remaining, prod `axon` untouched.
6. Built and enabled `docker-restart-watch.service` (polls RestartCount, pings gotify from `~/.lab/.env` creds, 10-min cooldown, linger on); verified gotify with a test ping.
7. Ran `/vibin:quick-push`: bumped 5.5.3→5.5.4, updated CHANGELOG, committed all 10 dirty in-repo files as `df3ca011`, pushed (hooks: xtask/clippy/test all green).

## Key Findings
- **sccache never crashed.** `MemoryHigh=12G` is a *soft* throttle; the server's working set peaks ~12.4G, so it crossed it constantly (`high=102841`, `oom_kill=0`). Under throttle the kernel stalled its threads; waiting `cargo` clients timed out on the UDS connect (`SCCACHE_SERVER_UDS=/tmp/sccache-jmagar.sock`) and fell back to local compile. Server-side counterpart in `~/.local/state/sccache/error.log`: "Failed to bind socket: Broken pipe".
- **Qdrant was crashlooping, not hogging.** `docker inspect axon-qdrant`: `RestartCount=206`, `MemLimit=12G`; `dmesg` showed repeated `Memory cgroup out of memory: Killed process (qdrant)` `constraint=CONSTRAINT_MEMCG`, anon-rss ~12.5G, ~every 9–10 min.
- **The 12.5G is genuinely pinned.** `axon` collection (4,493,621 pts, 1024-dim) has `vectors.on_disk=true` + `on_disk_payload=true` (mmap, harmless) but `quantization.scalar.always_ram=true` (~4.6G int8) + `hnsw.on_disk=false` (index in RAM) — ~0.5G over the 12G cap.
- **ZFS was innocent:** ARC capped at `zfs_arc_max=4G`, sitting at 4G. Real RAM consumers: qdrant ~11.9G, next-server 1.9G, cortex 1.4G, TEI 0.6G, plus concurrent `claude` sessions.
- **38 of 75 Qdrant collections were ephemeral** test/CI/ingest-verify debris totalling ~22k points (<0.5% of `axon`).

## Technical Decisions
- **Raise limits rather than shrink working sets.** For sccache, raised `MemoryHigh` above the real peak (15G, under the proven-sufficient 16G `MemoryMax`). For Qdrant, raised the container cap 12G→16G (the working set is stable at 12.5G and `MemoryMax`/`oom_kill` was never hit, so the cap bump is near-zero real cost and zero retrieval-quality change). Deferred Option B (`always_ram:false` + `hnsw.on_disk:true`) — it cuts ~5–6G but adds query latency, and its cost is highest exactly under the host pressure it's meant to relieve.
- **Applied sccache fix live** (`daemon-reload` + `set-property --runtime`) to preserve the 6-day-warm 52GiB cache; persisted via a chezmoi-tracked drop-in.
- **Recreated Qdrant** (already crashlooping, so a clean recreate is strictly better); bind-mounted storage preserved all 4.49M points.
- **Watcher reads gotify creds at runtime** from `~/.lab/.env` — no secrets baked into the script; placed outside the axon repo per request.
- **Conservative deletion:** only deleted collections re-verified ≤2 points (dead stubs) or self-evidently test artifacts; left all 36 real session-knowledge collections alone.

## Files Changed
In-repo changes were committed in `df3ca011`. Out-of-repo changes (systemd units, scripts, Qdrant state, agent memory) are listed for completeness.

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | docker-compose.prod.yaml | — | Qdrant container `limits.memory` 12G→16G + rationale comment | df3ca011 |
| modified | .env.example | — | Pre-existing config/env doc sync (research full-content, LLM concurrency, GOOGLE_API_KEY, etc.) | df3ca011 |
| modified | config.example.toml | — | ask.chunk-limit + authoritative-domain default doc sync | df3ca011 |
| modified | docs/guides/configuration.md | — | env var reference sync (synthesis/chat split, watch, NO_COLOR, endpoints) | df3ca011 |
| modified | plugins/axon/skills/using-axon/SKILL.md | — | added Configuration section pointing at authoritative refs | df3ca011 |
| modified | Cargo.toml / Cargo.lock | — | version 5.5.3→5.5.4 | df3ca011 |
| modified | plugins/axon/.claude-plugin/plugin.json | — | version 5.5.3→5.5.4 | df3ca011 |
| modified | README.md | — | Version: 5.5.3→5.5.4 | df3ca011 |
| modified | CHANGELOG.md | — | new 5.5.4 section | df3ca011 |
| created | ~/.config/systemd/user/sccache.service.d/memory.conf | — | sccache MemoryHigh 12G→15G drop-in (also chezmoi-tracked, dotfiles 383adbb) | systemctl show |
| created | ~/.local/bin/docker-restart-watch.sh | — | restart→gotify watcher script | service active |
| created | ~/.config/systemd/user/docker-restart-watch.service | — | persistent watcher unit (enabled + linger) | is-active=active |
| created | ~/.claude/.../memory/qdrant-oom-crashloop.md | — | agent memory of the crashloop fix + levers | file written |

## Beads Activity
No bead activity observed. No beads were created, claimed, closed, or commented during this session. Remaining follow-ups are captured in Next Steps rather than beads (operational/host-infra items, mostly out of the axon repo's tracker scope).

## Repository Maintenance
- **Plans:** Checked `docs/plans/`. No plan was completed this session; the injected "Active plan" points at a different repo (`axon_rust`). No moves made.
- **Beads:** Read-only consideration; no session work mapped to a bead. Stated explicitly above.
- **Worktrees/branches:** `git worktree list` shows a single worktree on `fix/mcp-informative-errors` (active PR #194). No stale worktrees/branches; nothing removed.
- **Stale docs:** The in-repo config/env docs were themselves the sync target and are now current. `CLAUDE.md` cites the compose limit only generically (no hardcoded 12G), so no drift introduced by the 16G bump.
- **Out-of-scope note:** Two files (`plugins/axon/README.md`, `plugins/axon/skills/using-axon/SKILL.md`) became dirty *after* the push, provenance unclear (likely a plugin-sync/hook). Left untouched; the session-doc commit is path-scoped and excludes them. Flagged in Open Questions.

## Tools and Skills Used
- **Shell (Bash):** the bulk of the work — `systemctl --user`, `docker inspect`/`compose`/`ps`, `dmesg`, `curl` against the Qdrant REST API, `chezmoi add`, `sed`/`python3` for the version bump and changelog, `git`. Issues: an early per-collection count loop hammered Qdrant mid-restart and returned empty rows (retried with gentler pacing); one `docker inspect --format` used an undefined `div` template func (cosmetic, re-run with awk).
- **File tools (Write/Edit/Read):** drop-in conf, watcher script, systemd unit, agent memory, session doc; compose edit.
- **Skills:** `vibin:quick-push` (release + push), `vibin:save-to-md` (this doc).
- **External services:** Qdrant REST API (inventory/delete), Gotify HTTP API (test ping, `http=200`), chezmoi (dotfiles capture + auto-push).
- No MCP tools, subagents, or browser tools were used.

## Commands Executed
| command | result |
|---|---|
| `cat .../sccache.service/memory.events` | `high 102841`, `oom_kill 0` — soft-throttle root cause |
| `systemctl --user set-property sccache.service MemoryHigh=15G ... --runtime` | live cap raised, server PID unchanged (cache preserved) |
| `docker inspect axon-qdrant` | `RestartCount=206`, `MemLimit=12G` |
| `dmesg -T \| grep -i oom` | repeated `CONSTRAINT_MEMCG` kills of qdrant ~12.5G |
| `docker compose ... up -d axon-qdrant` | recreated; RestartCount→0, readyz 200, axon green 4,493,621 |
| Qdrant `DELETE /collections/*` ×38 | 75→37 collections, prod axon intact |
| `systemctl --user enable --now docker-restart-watch.service` | active, linger enabled |
| `git commit ... && git push` (df3ca011) | hooks green (xtask, clippy 30s, test 75s); pushed |

## Errors Encountered
- **Qdrant REST calls intermittently failed (`http=000`) mid-investigation.** Root cause: the container OOM-restarted (the very bug being diagnosed) and binds its port before `readyz` passes. Resolved by waiting for `readyz=200` after the cap bump.
- **Empty/garbled collection count output** on the first loop. Root cause: 75 rapid sequential curls during a warming restart. Resolved with `--max-time` + `sleep 0.12` pacing and a single-call name fetch.

## Behavior Changes (Before/After)
| area | before | after |
|---|---|---|
| sccache | client falls back to local compile under throttle stalls | soft cap 15G > 12.4G peak; throttle stalls eliminated |
| axon-qdrant | OOM-killed ~every 9–10 min (206 restarts) | 16G cap; RestartCount=0, stable |
| Qdrant collections | 75 (38 junk) | 37 (prod + real session data only) |
| container restarts | silent | gotify push on any RestartCount increase |
| axon version | 5.5.3 | 5.5.4 |

## Verification Evidence
| command | expected | actual | status |
|---|---|---|---|
| `cat memory.high` (sccache cgroup) | 15G live | 16106127360 (15 GiB) | pass |
| `docker inspect axon-qdrant MemLimit` | 16G | 17179869184 | pass |
| `docker inspect axon-qdrant RestartCount` | 0 and holding | 0 | pass |
| `curl /collections/axon` | 4,493,621 green | 4,493,621 green | pass |
| `curl /collections \| length` | 37 | 37 | pass |
| gotify test POST | http 200 | http=200 | pass |
| `systemctl --user is-active docker-restart-watch` | active | active | pass |
| `git push` | df3ca011 to origin | 4e7ae86f..df3ca011 | pass |

## Risks and Rollback
- **Higher Qdrant cap on a pressured host.** 16G cap on a 48G box already ~33G used. Real risk low: working set is stable at ~12.5G and `MemoryMax`/host `oom_kill` was never hit; the cap just stops the soft kill. Rollback: revert `docker-compose.prod.yaml` to `memory: 12G` and recreate.
- **sccache 15G soft cap** likewise only raises a throttle threshold under the 16G hard cap; rollback is removing the drop-in.
- **Collection deletions are irreversible** but were re-verified ≤2 points / test-only before deletion; no production data affected.

## Decisions Not Taken
- **ZFS→XFS migration:** rejected — no in-place conversion exists, ARC was already capped at 4G, and it would not address the real RAM consumers. Huge blast radius (snapshots, replication) for ~4G.
- **Qdrant Option B (`always_ram:false` + `hnsw.on_disk:true`):** deferred — frees ~5–6G but adds query latency precisely when the host is pressured; unnecessary once the cap was raised and junk cleared.
- **Adding swap / raising swap:** rejected — swap was 100% full because of over-commit, not under-provisioning; hiding pressure rather than fixing it.

## References
- Dependabot alert (1 moderate, default branch, not from this push): https://github.com/jmagar/axon/security/dependabot/92
- PR #194: https://github.com/jmagar/axon/pull/194

## Open Questions
- Why did `plugins/axon/README.md` and `plugins/axon/skills/using-axon/SKILL.md` become dirty *after* the push? Likely a plugin-sync/hook regenerated them; left untouched and excluded from the session-doc commit. Worth confirming before the next push so they aren't silently swept in.
- Host-wide memory over-commit remains (swap was 100% full). The cap bumps stop the two acute failures but the box is still tight under concurrent load.

## Next Steps
- **Unfinished from this session:** none — all requested work shipped.
- **Recommended immediate:** confirm the watcher end-to-end (`docker restart <harmless-container>` → expect a gotify push); decide whether to commit/ignore the two post-push dirty plugin files.
- **Follow-on (not started):** triage the Dependabot moderate vuln (#92); decide whether to merge/PR `fix/mcp-informative-errors` (PR #194) which now also carries the 5.5.4 infra/doc bump; optionally apply Qdrant Option B only if host pressure persists.
- **Out of repo:** the sccache drop-in is in dotfiles (383adbb); the watcher script/unit and agent memory live under `~/` and are not version-controlled in this repo.
