---
date: 2026-06-17 22:59:49 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/android-share-target
head: 32b1ed8f
working directory: /home/jmagar/workspace/axon/.worktrees/codex/android-share-target
worktree: /home/jmagar/workspace/axon/.worktrees/codex/android-share-target 32b1ed8f [codex/android-share-target]
pr: "#235 Add Android OAuth sign-in and share-target mobile workflow https://github.com/jmagar/axon/pull/235"
---

# Android OAuth, share target, and review fixes

## User Request

Implement the Android-side OAuth plan for Axon, keep the work isolated in a worktree, show/test the Android app changes, publish a PR, and run the full `vibin:work-it` review-and-fix workflow.

## Session Overview

The branch added the Android share target and mobile workflow, Android OAuth sign-in, mobile session persistence, jobs detail views, and settings cleanup. After PR creation, the independent review wave found auth-boundary and persistence issues; this follow-up fixed those findings and re-ran the Android and Rust verification gates.

## Sequence of Events

1. Created and worked from `/home/jmagar/workspace/axon/.worktrees/codex/android-share-target` on `codex/android-share-target`.
2. Dispatched an implementation agent to execute the Android OAuth plan and then committed/pushed `32b1ed8f feat(android): add oauth sign-in and mobile sessions`.
3. Created PR #235 and ran the Lavra review wave across architecture, security, performance, simplicity, and data integrity.
4. Fixed review findings around panel auth, mobile session ownership, stale writes, OAuth cancellation/sign-out, operation-card restore, config saves, progress math, and large crawl page lists.
5. Ran the PR-review-toolkit sweep and fixed the remaining issues around Android panel auth, OAuth server binding, visible mobile session sync failures, legacy session migration, route/OpenAPI inventory, and full-router session tests.
6. Re-ran targeted and full Android/Rust verification before preparing the final review-fix commit.

## Key Findings

- `/api/panel/*` had accidentally accepted the general MCP bearer token; panel routes now require `x-axon-panel-token` again.
- `/v1/mobile/sessions` initially stored all sessions in one global keyspace; handlers now derive an owner from `AuthContext` and scope list/get/upsert/delete by that owner.
- Mobile session JSON writes were read-modify-write without serialization; writes now use a process-wide async mutex and reject stale `updated_at` updates.
- Android OAuth pending state was process-local; the pending authorization state is now persisted in encrypted storage so callbacks can survive app recreation.
- Restored operation cards were being converted into normal assistant messages; persisted activity/action/injection items now restore as typed chat items.
- Android OAuth credentials could be reused after changing the configured Axon server URL; OAuth credentials are now bound to the server URL used for sign-in.
- Android mobile session list/load/save failures were silent; the Ask and Sessions screens now expose errors instead of mutating local state as if remote sync succeeded.
- `/v1/mobile/sessions*` routes were missing from route inventory and OpenAPI; the REST contract now advertises and tests those routes.

## Technical Decisions

- Kept panel config/env routes panel-scoped instead of allowing OAuth or static API bearer tokens to read/write raw server config.
- Used an internal composite store key for mobile session ownership so Android payload schemas stay unchanged.
- Added stale-update rejection server-side and debounce/coalescing client-side to reduce lost updates and needless full-session uploads.
- Chose a 200-row cap for job detail page crawled URLs to avoid eager rendering freezes on very large crawl manifests.
- Refreshed raw `.env` / `config.toml` immediately before mobile settings saves so dirty-key patches do not revert unrelated server-side edits.
- Kept bearer tokens usable for panel config only through the explicit `x-axon-panel-token` header, while OAuth tokens remain unavailable for raw panel config endpoints.
- Migrated legacy unscoped mobile sessions into the authenticated owner namespace on first access to avoid making existing Android sessions disappear after owner scoping.

## Files Changed

| status | path | purpose |
|---|---|---|
| modified | `apps/android/app/src/main/java/com/axon/app/data/auth/OAuthRepository.kt` | Persist pending OAuth state, add cancel, serialize sign-out with refresh. |
| modified | `apps/android/app/src/main/java/com/axon/app/data/auth/OAuthStateStore.kt` | Store and clear pending OAuth callback state in encrypted prefs. |
| modified | `apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt` | Send panel token headers for panel routes and bind OAuth tokens to the signed-in server URL. |
| modified | `apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt` | Build OAuth auth config with the normalized server URL. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/ask/AskSessionPersistence.kt` | Restore typed operation items and persist operation body fields. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/ask/AskViewModel.kt` | Debounce mobile session saves, cancel pending saves on session switches, and surface session load/save failures. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobDetailScreen.kt` | Cap crawled-page rows on job detail. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsFormatters.kt` | Treat `queued` as remaining work when computing progress. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionsDrawerContent.kt` | Show mobile session sync errors in the sessions drawer. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionsViewModel.kt` | Avoid silent remote sync fallback for pin/unpin/delete and expose sync errors. |
| modified | `apps/android/app/src/main/java/com/axon/app/ui/settings/SettingsViewModel.kt` | Cancel OAuth repository state, refresh raw config before saves, and save the sign-in server URL on callback. |
| modified | `apps/android/app/src/test/java/com/axon/app/data/auth/OAuthRepositoryTest.kt` | Cover persisted pending state and suspend sign-out behavior. |
| modified | `apps/android/app/src/test/java/com/axon/app/data/remote/AxonClientTest.kt` | Cover panel-token headers and OAuth server URL binding. |
| modified | `docs/reference/api-parity.md` | Document advertised mobile session REST routes. |
| modified | `src/services/mobile_sessions.rs` | Add owner scoping, write lock, stale update rejection, payload validation, legacy migration, and regression tests. |
| modified | `src/services/types/route_inventory.rs` | Add mobile session routes to the REST inventory. |
| modified | `src/web/server/handlers/config.rs` | Add timeout to Qdrant collections metadata request. |
| modified | `src/web/server/handlers/mobile_sessions.rs` | Pass authenticated owner into mobile session services and expose OpenAPI path metadata. |
| modified | `src/web/server/openapi.rs` | Register mobile session routes and schemas in OpenAPI. |
| modified | `src/web/server/routing.rs` | Block unauthenticated loopback mobile session PUT/DELETE. |
| modified | `src/web/server/utils.rs` | Restore panel-only authorization boundary. |
| modified | `src/web/server_test_support_tests.rs` | Prove panel artifact rejects bearer and accepts panel token. |
| modified | `src/web/server_tests.rs` | Cover mobile session auth inventory, loopback reads, round-trip, stale update, delete, and OpenAPI listing. |

## Beads Activity

No bead activity observed in the coordinator transcript for this follow-up.

## Repository Maintenance

- Plans: no plan files were moved during this follow-up; the active plan work was already represented by PR #235.
- Beads: no bead updates were made; no direct bead ID was provided for this Android PR workflow.
- Worktrees and branches: the active worktree is `/home/jmagar/workspace/axon/.worktrees/codex/android-share-target`; it is the PR branch worktree and was left in place.
- Stale docs: no broad docs update was attempted beyond this session note; the user-facing behavior was still in-flight on PR #235.
- `vibin:save-to-md` note: its skill body requires a separate session-only commit, while `vibin:work-it` requires the note before the final `git add .`; this note was created manually so it can be included with the final review-fix commit.

## Tools and Skills Used

- Shell commands: git status/diff/log, Gradle, Cargo, gh, date, and file inspection commands.
- File tools: `apply_patch` for scoped source and test edits.
- Skills/plugins: `vibin:work-it`, `vibin:save-to-md` instructions, `lavra-review` review wave, Android/Rust repo guidance.
- Subagents: implementation agent plus Lavra review agents; all review agents were closed after findings were collected.

## Commands Executed

| command | result |
|---|---|
| `./gradlew -PaxonAuroraAndroidPath=/home/jmagar/workspace/aurora-design-system/android :app:testDebugUnitTest --tests com.axon.app.data.auth.OAuthRepositoryTest --tests com.axon.app.ui.settings.SettingsViewModelTest --tests com.axon.app.data.remote.AxonClientTest --tests com.axon.app.ui.jobs.JobsFormattersTest` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib services::mobile_sessions::tests::sessions_are_owner_scoped_and_reject_stale_updates` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::tests::loopback_dev_blocks_destructive_rest_routes_without_auth` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::test_support::panel_artifact_requires_panel_token_and_serves_png` | passed |
| `./gradlew -PaxonAuroraAndroidPath=/home/jmagar/workspace/aurora-design-system/android :app:testDebugUnitTest :app:assembleDebug :app:assembleRelease` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test --workspace` | passed: 3155 lib tests passed, 6 ignored, plus integration/doc tests passed |
| `cargo fmt --check && git diff --check` | passed |
| `./gradlew -PaxonAuroraAndroidPath=/home/jmagar/workspace/aurora-design-system/android :app:testDebugUnitTest --tests com.axon.app.data.remote.AxonClientTest` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib mobile_sessions` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::tests::all_v1_rest_routes_reject_missing_auth_when_auth_is_configured` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::tests::mobile_session_routes_round_trip_and_reject_stale_updates` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::tests::openapi_docs_are_public_and_list_rest_routes` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --lib web::server::tests::loopback_dev_can_read_empty_mobile_session_list_without_auth_extension` | passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test -p axon --test http_api_parity_inventory` | passed after documenting mobile session routes |

## Errors Encountered

- Cargo was initially invoked with two test filters, which Cargo rejects. The tests were rerun with one filter per command.
- A first service regression test attempted to mutate `AXON_DATA_DIR` and hit repo-wide `unsafe_code` denial. The test was replaced with a pure store-mutation helper test.
- Full Rust verification caught one panel artifact test still expecting bearer auth to unlock a panel route. The test was updated to assert bearer rejection and panel-token success.
- Full Rust verification later caught `docs/reference/api-parity.md` missing the newly advertised mobile session REST routes. The parity doc was updated and both the parity test and full workspace test passed.

## Behavior Changes

| area | before | after |
|---|---|---|
| Panel routes | API bearer token could unlock panel internals. | Only the panel token is accepted for panel internals. |
| Mobile sessions | Sessions were global and write races could lose updates. | Sessions are owner-scoped, serialized on write, and stale updates are rejected. |
| OAuth | Browser-auth callback state was memory-only; cancel left sign-in stuck. | Pending state is persisted, cancel resets repository state, and sign-out is mutex-protected. |
| OAuth server binding | OAuth tokens could be reused after changing the server URL. | OAuth auth config carries the signed-in server URL and refuses mismatched requests. |
| Chat restore | Operation cards could become normal assistant turns. | Operation items restore as typed cards and stay out of Q/A follow-up turns. |
| Session sync failures | List/load/save/pin/delete failures were mostly silent. | Failures are logged and surfaced in Ask/Sessions UI state. |
| Mobile REST contract | Mobile session routes existed without inventory/OpenAPI coverage. | Routes are in inventory, OpenAPI, parity docs, and router tests. |
| Jobs UI | Large crawl page lists rendered eagerly. | Detail page shows a bounded crawled-page list. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| Android full gate | tests and debug/release APK assembly pass | build successful | pass |
| Rust full gate | workspace tests pass | workspace tests passed after parity doc update | pass |
| Diff hygiene | no whitespace/format issues | clean | pass |
| Targeted panel auth test | bearer rejected, panel token accepted | passed | pass |
| Targeted mobile session tests | owner partition, stale rejection, legacy migration, route round-trip, and OpenAPI listing hold | passed | pass |

## Risks and Rollback

- Restoring panel-only auth means the Android config page still cannot use OAuth app tokens to read/write raw panel config. Roll back only by adding a deliberately scoped admin API, not by reusing the general bearer token for `/api/panel/*`.
- The mobile session store remains a JSON file rather than SQLite. The current lock/stale guard, payload validation, and legacy migration fix the reported correctness risks, but SQLite would be the stronger long-term store.

## Decisions Not Taken

- Did not implement verified HTTPS App Links for OAuth redirect in this follow-up; the current custom scheme remains the Android app redirect path.
- Did not replace the JSON session store with SQLite in this review-fix batch; the smaller lock plus owner/stale checks addressed the reported correctness risks.

## References

- PR #235: https://github.com/jmagar/axon/pull/235
- `vibin:work-it` skill: `/home/jmagar/.codex/plugins/cache/dendrite/vibin/local/skills/work-it/SKILL.md`
- `vibin:save-to-md` skill: `/home/jmagar/.codex/plugins/cache/dendrite/vibin/local/skills/save-to-md/SKILL.md`

## Open Questions

- Whether to add a proper OAuth-admin or panel-session flow for the Android settings page, instead of raw panel config over the general app token.
- Whether to migrate mobile sessions from JSON to SQLite before merging if multi-device history becomes a heavy-use feature.

## Next Steps

1. Commit and push this review-fix batch to PR #235.
2. Fetch and resolve any new PR comments after the pushed commit is visible remotely.
