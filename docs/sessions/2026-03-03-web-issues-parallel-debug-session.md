# Session Log — Web Issues Parallel Debug
Date: 2026-03-03
Start context: `apps/web` startup/runtime warnings and failures in Docker

## 1. Session overview
- Investigated six user-reported `axon-web` startup issues in parallel using six subagents with non-overlapping scopes.
- Implemented targeted fixes for `middleware` deprecation migration and install/runtime race hardening.
- Validated build success and container restart behavior after changes.
- Collected unresolved/partially resolved items as explicit risks and open questions.

## 2. Timeline of major activities
- Created team tasks for six issues, dispatched dedicated agents, collected per-issue findings.
- Migrated `apps/web/middleware.ts` to `apps/web/proxy.ts` and updated references.
- Updated startup scripts to stop `pnpm-dev` before `pnpm install` and restart afterward.
- Added startup dependency self-heal guard for missing `next` binary.
- Rebuilt app, restarted `axon-web`, rechecked logs and install behavior.

## 3. Key findings (with references)
- Next.js 16 deprecates middleware file convention; app used `middleware.ts` (`apps/web/proxy.ts:117` now exports `proxy`).
- Runtime panic root cause: install/dev race in watcher flow; install happened while dev server could still be live (`docker/web/s6-rc.d/pnpm-watcher/run:14`, `:28`).
- Startup robustness gap: missing `next` bin was not part of install trigger; guard added (`docker/web/cont-init.d/20-pnpm-install:17`, `:20`).
- `turbopack.root` was already configured (`apps/web/next.config.ts:20`), so `/app/app` resolution errors were observed as runtime instability symptom, not absent config.
- Build-script warnings for `esbuild`/`msw` are still emitted during install despite policy entries (`apps/web/pnpm-workspace.yaml:1`, `apps/web/package.json:104`).

## 4. Technical decisions and rationale
- Renamed middleware file and export to `proxy` to align with Next.js 16 expectations and remove deprecation path.
- Changed watcher sequencing to stop `pnpm-dev` before install to avoid mutating `node_modules` while Turbopack is active.
- Added `NEXT_BIN` existence check in cont-init install step to recover from stale/corrupt `node_modules` volumes.
- Kept strict build-script policy (`onlyBuiltDependencies: node-pty`) and documented ignored scripts instead of broad approvals.

## 5. Files modified/created and purpose
- `apps/web/proxy.ts` (created): replacement for deprecated middleware entry; exports `proxy`.
- `apps/web/middleware.ts` (deleted): removed deprecated convention file.
- `docker/web/s6-rc.d/pnpm-watcher/run`: reordered stop/install/start flow to prevent race.
- `docker/web/cont-init.d/20-pnpm-install`: added `NEXT_BIN` health gate to force reinstall when needed.
- `apps/web/next.config.ts`, `apps/web/README.md`, `apps/web/CLAUDE.md`, `apps/web/app/api/logs/route.ts`, `apps/web/pnpm-workspace.yaml`, `apps/web/package.json`: reference/policy updates.

## 6. Critical commands executed and outcomes
- `pnpm --dir /home/jmagar/workspace/axon_rust/apps/web build` -> success, optimized build completed.
- `timeout 25s pnpm --dir /home/jmagar/workspace/axon_rust/apps/web dev` -> failed locally due to existing dev lock (`.next/dev/lock`).
- `docker compose -f /home/jmagar/workspace/axon_rust/docker-compose.yaml restart axon-web` -> container restarted successfully.
- `docker compose ... logs --tail=80 axon-web` -> observed fresh startup and historical error lines in same tail window.
- `docker compose ... exec -T axon-web sh -lc 'cd /app && CI=true pnpm install --frozen-lockfile'` -> install succeeded; warning about ignored build scripts remained.

## 7. Behavior changes (before/after)
- Before: deprecation warning for `middleware` convention during startup; after: runtime file moved to `proxy.ts` and exported as `proxy`.
- Before: watcher installed while dev process lifecycle overlap was possible; after: watcher stops `pnpm-dev` first, then installs, then restarts.
- Before: install trigger depended only on sentinel/lockfile timestamp; after: also triggers when `/app/node_modules/.bin/next` is missing.
- Before: repeated panic reports linked to package resolution race in logs; after: sequencing hardened, but follow-up verification still required under repeated lockfile churn.

## 8. Verification evidence
- `pnpm --dir ... build` | expected: successful production build | actual: completed with route table output | status: pass
- `timeout 25s pnpm --dir ... dev` | expected: dev server start | actual: failed due to existing lock (`.next/dev/lock`) | status: blocked-by-existing-process
- `docker compose ... restart axon-web` | expected: service restarts cleanly | actual: restart completed and service started | status: pass
- `docker compose ... logs --tail=80 axon-web` | expected: verify post-fix startup behavior | actual: saw ready startup plus prior tail errors/warnings in same window | status: mixed-evidence
- `docker compose ... exec ... pnpm install --frozen-lockfile` | expected: sync install | actual: install succeeded; ignored build-script warning persisted | status: partial

## 9. Source IDs + collections touched
- Embed command: `./scripts/axon embed "/home/jmagar/workspace/axon_rust/docs/sessions/2026-03-03-web-issues-parallel-debug-session.md" --json`.
- Initial embed response: `{"job_id":"d7d9b000-2db3-4b99-a9cd-df3df433f209","source":"rust","status":"pending"}`.
- Status command (`embed status`) reported terminal `completed` with `result_json.collection = "cortex"` and input path `/home/jmagar/workspace/axon_rust/docs/sessions/2026-03-03-web-issues-parallel-debug-session.md`.
- Retrieve verification used source identifier `/home/jmagar/workspace/axon_rust/docs/sessions/2026-03-03-web-issues-parallel-debug-session.md` with collection `cortex`; output reported `Chunks: 1`.
- Note: status JSON did not expose `data.url`; source identifier used for retrieve was the embedded path from status payload.

## 10. Risks and rollback
- Risk: mixed historical + fresh logs can obscure whether a warning is still active.
- Risk: `pnpm` ignored build-script warning persists and may confuse operations.
- Risk: broad pre-existing repo changes may complicate isolating this session’s diff in future reviews.
- Rollback: restore `docker/web/s6-rc.d/pnpm-watcher/run` and `docker/web/cont-init.d/20-pnpm-install` from git if watcher behavior regresses.
- Rollback: restore `apps/web/middleware.ts` and reverse `proxy.ts` migration only if Next.js compatibility requires legacy behavior.

## 11. Decisions not taken
- Did not approve all PNPM build scripts globally.
- Did not remove watcher service or disable lockfile polling.
- Did not run destructive Docker volume cleanup in this session.
- Did not claim final elimination of all startup warnings from a single log tail sample.

## 12. Open questions
- Why does PNPM continue warning for ignored scripts despite `ignoredBuiltDependencies` entries in workspace/package config?
- Does `axon-web` still emit any proxy-related startup warnings after a clean container lifecycle with empty tail history?
- Is additional lockfile/debounce logic needed to prevent frequent watcher restarts under host-side lockfile updates?

## 13. Next steps
- Run a clean-slate `axon-web` startup verification with isolated fresh logs and no prior tail noise.
- Decide whether to explicitly approve needed scripts or keep warnings as accepted policy noise.
- If warnings persist undesirably, test PNPM config placement/version semantics for suppression in this Docker runtime.
