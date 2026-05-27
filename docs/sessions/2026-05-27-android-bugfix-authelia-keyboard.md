---
date: 2026-05-27 14:51:08 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: a5ae8350c5dac5395d5a9c173f8a43c520e2d0fc
session id: 211b64ba-65d8-4ffd-8c1b-4755fa3f8674
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon-rust/211b64ba-65d8-4ffd-8c1b-4755fa3f8674.jsonl
working directory: /home/jmagar/workspace/axon_rust
---

# Android app bugfix session — CrawlStatus envelope, Authelia bypass, keyboard IME

## User Request

Functional test the Axon Android app against the live emulator, ensure every tool works as intended, then fix whatever breaks including the crawl status polling bug carried over from the previous session.

## Session Overview

Continued from a compacted prior session. Fixed three bugs found during live emulator testing of the Axon Android app: (1) `CrawlStatusResponse` deserialized a flat shape but the server wraps the job in `{"job":{...}}`, causing status polling to always show "unknown"; (2) Authelia intercepted `/v1/ask/stream` POST requests from OkHttp (no session cookie) and issued a 302 to the login page, which the SSE parser interpreted as an empty stream; (3) `enableEdgeToEdge()` disabled `adjustResize` so the keyboard covered input fields. All three bugs fixed, tests updated, APK rebuilt and uploaded to Google Drive twice.

## Sequence of Events

1. **Resumed from compaction.** Picked up mid-session fix task for `CrawlStatusResponse` API shape mismatch.
2. **Read source files.** Inspected `AxonModels.kt`, `AxonClient.kt`, `AxonRepositoryTest.kt`, and `AxonRepository.kt` to understand current state and confirm the mismatch.
3. **Verified actual API shape.** `curl GET /v1/crawl/{job_id}` confirmed server returns `{"job":{...}}` wrapper with `id`, `status`, `error_text`, and nested `result_json.pages_crawled`.
4. **Fixed CrawlStatusResponse.** Added `CrawlStatusWrapper` envelope type, `CrawlResultJson` sub-object, corrected field names (`id`, `error_text`, `result_json`), added `jobId`/`pagesCrawled` computed properties for backward-compatible access.
5. **Updated AxonClient.** Changed `crawlStatus()` to decode `CrawlStatusWrapper` and call `.map { it.job }`.
6. **Updated tests.** Rewrote two crawlStatus tests with real `{"job":{...}}` JSON shape; added two new tests for `pagesCrawled` extraction and `jobId` fallback.
7. **Built and installed APK on emulator.** All 36 unit tests passed; emulator confirmed status polling now shows "completed" and "Pages crawled: 1".
8. **Rcloned APK to Google Drive** (first time).
9. **User reported Ask broken on real device** — "no response received from server".
10. **Investigated.** Server healthy, SSE stream worked from dookie via curl. Checked nginx access logs on squirts; found `174.225.198.53` (user's device) getting HTTP 302 then 200/1125 bytes (Authelia login page HTML).
11. **Root-caused Authelia intercept.** OkHttp sends Bearer token but no Authelia session cookie. Authelia redirected to login page. OkHttp followed the redirect, SSE parser found no `data:` lines, stream appeared empty.
12. **Fixed axon.subdomain.conf on squirts.** Added `location ^~ /v1/` block bypassing Authelia with `proxy_buffering off`; axon enforces its own Bearer token auth. Tested nginx config, reloaded.
13. **User confirmed Ask works** after SWAG reload.
14. **Fixed keyboard covering inputs.** `enableEdgeToEdge()` in `MainActivity` disables `adjustResize`. Added `consumeWindowInsets(innerPadding).imePadding()` to `NavHost` modifier in `AxonNavGraph.kt`.
15. **Built, installed, committed, pushed, rcloned** updated APK.
16. **Repository maintenance.** `feat/axon-android-app` confirmed merged into main; removed worktree `.worktrees/axon-android-app` and deleted local + remote branch.

## Key Findings

- `AxonModels.kt:197-203` — `CrawlStatusResponse` assumed flat shape; actual API returns `{"job":{...}}` wrapper with `id` (not `job_id`), `error_text` (not `error`), and `pages_crawled` nested under `result_json`.
- `AxonRepository.kt:132` — `r.status.ifBlank { "unknown" }` was the visible symptom; all fields defaulted to `""` because `ignoreUnknownKeys = true` silently dropped the `"job"` wrapper.
- `apps/android/app/src/test/java/.../AxonRepositoryTest.kt:206-227` — existing crawlStatus tests used flat mock JSON that never matched production, so they passed while masking the bug.
- `nginx/access.log` on squirts — `174.225.198.53` (user device) received `302 → 200/1125` for every `/v1/ask/stream` POST; `76.213.118.20` (dookie) received `200` directly. Authelia was the differentiator.
- `AxonNavGraph.kt:98-103` — `Modifier.padding(innerPadding)` alone does not handle keyboard insets when `enableEdgeToEdge()` is active; `consumeWindowInsets` + `imePadding()` required.

## Technical Decisions

- **`CrawlStatusWrapper` + computed properties** rather than changing `AxonRepository.kt` — kept the repository/UI layer untouched; only the deserialization boundary changed.
- **`location ^~ /v1/`** (exact-prefix match) in nginx rather than a regex — faster matching, unambiguous priority over `location /`, and covers all current and future API routes under `/v1/` in one rule.
- **`proxy_buffering off` on `/v1/`** included alongside the Authelia bypass — belt-and-suspenders for SSE correctness even though the buffering issue was not the root cause of the user's failure.
- **`consumeWindowInsets(innerPadding)` before `imePadding()`** — without consuming the nav bar insets first, `imePadding()` would double-count the bottom nav bar height and push content too far up.
- **Single NavHost modifier change** rather than per-screen `imePadding()` — fixes all five screens in one edit with no risk of forgetting a screen.

## Files Changed

| Status | Path | Purpose |
|---|---|---|
| modified | `apps/android/app/src/main/java/com/axon/app/data/remote/AxonModels.kt` | Replace flat `CrawlStatusResponse` with `CrawlStatusWrapper` envelope, `CrawlResultJson` sub-object, corrected field names |
| modified | `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt` | `crawlStatus()` decodes `CrawlStatusWrapper` and unwraps `.job` |
| modified | `apps/android/app/src/test/java/com/axon/app/data/repository/AxonRepositoryTest.kt` | Update two crawlStatus tests to real JSON shape; add two new tests |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNavGraph.kt` | Add `consumeWindowInsets(innerPadding).imePadding()` to NavHost modifier |
| modified | `/mnt/appdata/swag/nginx/proxy-confs/axon.subdomain.conf` (on squirts) | Add `location ^~ /v1/` bypassing Authelia; include `proxy_buffering off` |

## Beads Activity

No bead activity observed this session. The work was a direct continuation of the prior session's identified bug; no new beads were created and no existing beads were in scope.

## Repository Maintenance

**Plans:** All plans in `docs/plans/` reviewed. None of the active plans (`2026-03-05-watch-top-level-scheduler.md`, `2026-05-21-port-webclaw-diff-brand.md`, etc.) were completed by this session. No moves performed.

**Worktrees/branches:** `feat/axon-android-app` confirmed fully merged into `main` via `git merge-base --is-ancestor`. Worktree `.worktrees/axon-android-app` removed, local branch deleted, remote branch `origin/feat/axon-android-app` deleted.

**Stale docs:** No documentation was found to be contradicted by session changes. Android CLAUDE.md not present; no docs update needed.

**No-ops / skipped:** No bead updates, no plan moves. `docs/palette-demo/` untracked directory left untouched (not related to this session).

## Tools and Skills Used

- **Shell / ADB** — emulator lifecycle, `uiautomator dump`, tap/input, screenshot, logcat, APK install
- **Bash file tools** — read Kotlin/nginx source files, write session doc
- **curl** — verify live API shapes (`/v1/crawl/{id}`, `/v1/ask/stream` headers, `/healthz`)
- **Gradle** — `assembleDebug`, `testDebugUnitTest`
- **rclone** — upload APK to `gdrive:` remote (run twice)
- **SSH to squirts** — read/write SWAG nginx config, `docker exec swag nginx -t`, `nginx -s reload`
- **save-to-md skill** — this session documentation

## Commands Executed

| Command | Result |
|---|---|
| `./gradlew :app:testDebugUnitTest` | 36 tests, 0 failures |
| `./gradlew :app:assembleDebug` (×2) | BUILD SUCCESSFUL, 20.4 MiB APK |
| `adb install -r -d app-debug.apk` (×2) | Success |
| `curl GET /v1/crawl/0972f143-...` | Confirmed `{"job":{"status":"completed",...}}` wrapper shape |
| `curl -N POST /v1/ask/stream` | Full SSE stream received; `event: meta → delta × N → done` |
| `curl -sv /healthz` | `server: cloudflare`, HTTP 200 |
| `ssh squirts grep 'ask' nginx/access.log` | Revealed `174.225.198.53` getting 302→200/1125 (Authelia redirect) |
| `ssh squirts docker exec swag nginx -t` | `syntax ok` |
| `ssh squirts docker exec swag nginx -s reload` | Reloaded (2 design.* warnings, unrelated) |
| `rclone copy app-debug.apk gdrive:` (×2) | 20.362 MiB transferred each time |
| `git merge-base --is-ancestor origin/feat/axon-android-app origin/main` | Exit 0 — confirmed merged |
| `git worktree remove .worktrees/axon-android-app` | Removed |
| `git branch -d feat/axon-android-app && git push origin --delete feat/axon-android-app` | Deleted local and remote |

## Errors Encountered

- **`CrawlStatusResponse` silent deserialization failure** — `ignoreUnknownKeys = true` silently dropped the `"job"` wrapper; all fields defaulted to `""`; `ifBlank { "unknown" }` fired. Fixed by adding `CrawlStatusWrapper` and updating `crawlStatus()`.
- **Ask "no response received from server" on real device** — Authelia issued HTTP 302 to login page; OkHttp followed redirect and received 1125 bytes of HTML with no `data:` SSE lines. Fixed by adding `location ^~ /v1/` bypass in nginx config.
- **Keyboard covers input fields** — `enableEdgeToEdge()` disables `adjustResize` window resize behavior; Compose needs explicit `imePadding()`. Fixed in `AxonNavGraph.kt`.

## Behavior Changes (Before / After)

| Area | Before | After |
|---|---|---|
| Crawl status polling | Always showed "unknown" regardless of actual job state | Shows correct server status ("completed", "running", etc.) and pages crawled count |
| Ask from real device (non-dookie) | HTTP 302 → Authelia login HTML → "No response received from server" | Authelia bypassed for `/v1/`; SSE stream reaches app correctly |
| Text input fields with keyboard open | Keyboard covered input fields on all screens | Content scrolls above keyboard via `consumeWindowInsets` + `imePadding()` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `./gradlew :app:testDebugUnitTest` | 36 pass, 0 fail | 36 pass, 0 fail | pass |
| Emulator: tap Check Status after crawl | "completed", pages crawled: 1 | "completed", Pages crawled: 1 | pass |
| Emulator: Ask "what is axon" | Full SSE answer received | Full SSE answer in UI | pass |
| `nginx -t` on squirts | `syntax ok` | `syntax ok` | pass |
| User device: Ask after nginx reload | Answer streams correctly | Confirmed by user | pass |

## Risks and Rollback

- **Authelia bypass on `/v1/`** — all API routes under `/v1/` now rely solely on axon's Bearer token auth. A misconfigured or missing token would allow unauthenticated access; axon already enforces this via `AXON_MCP_HTTP_TOKEN`. Rollback: restore `axon.subdomain.conf.bak.20260527*` on squirts and `nginx -s reload`.
- **`proxy_buffering off` on `/v1/`** — disables nginx response buffering for all API routes, not just SSE. For non-streaming endpoints this is safe (slightly higher memory usage per request, no correctness impact). No rollback needed.

## Next Steps

- **Install updated APK on device** and confirm keyboard IME fix is working on real hardware.
- **Remaining active plans** — `2026-05-21-port-webclaw-diff-brand.md` is the active plan per injected context; next work session should resume webclaw port epic `jej7` (6 open children: `25cu`, `di8j`, `jj43`, `urk2`, `kxot`, `upnq`).
- **GitHub dependabot alert** — push output showed 1 moderate vulnerability on default branch. Review at `https://github.com/jmagar/axon/security/dependabot/92`.
- **`docs/palette-demo/` untracked directory** — not staged or committed. Determine whether it belongs in the repo or should be gitignored.
