---
date: 2026-06-30
repo: git@github.com:jmagar/axon.git
branch: codex/android-ui-qa-report
head: 61cb7aa8
working directory: /home/jmagar/workspace/axon/.worktrees/codex-android-ui-qa-report
worktree: /home/jmagar/workspace/axon/.worktrees/codex-android-ui-qa-report
pr: "#296 [codex] Polish Android mobile UX https://github.com/jmagar/axon/pull/296"
---

# Android PR review closeout

## User Request

Run Lavra review and PR review toolkit agents against PR #296, address all issues surfaced, fix CI and tests, push, and merge once CI is green.

## Session Overview

Reviewed PR #296 with Lavra and PR-review-toolkit agents, then fixed all actionable introduced findings plus the failing pre-existing Android unit-test path. The Android APKs were rebuilt locally and the Aurora primitive inventory CI failure was reproduced and fixed.

## Sequence of Events

1. Gathered PR metadata, diff scope, changed files, and current CI checks.
2. Dispatched Lavra design, architecture, security, bug reproduction, simplicity, and PR toolkit review agents.
3. Fixed the failing `aurora-primitive-inventory` guard for `ShellBackBehavior.kt`.
4. Addressed review findings around OAuth migration, stale navigation state, stale activity detail, hidden partial refresh errors, diagnostics copy safety, accessibility target disclosure, local-only activity rows, and test-connection copy.
5. Fixed a pre-existing `AxonClientErrorPathTest` transport-failure hang and a Robolectric app-startup OAuth repository initialization failure.
6. Rebuilt/tested Android and refreshed debug/release APK artifacts.

## Key Findings

- PR CI was red because `ShellBackBehavior.kt` introduced `Sidebar` references that were not covered by `docs/reference/aurora-primitive-inventory.json`.
- Missing persisted `auth_mode` plus an existing bearer token would have upgraded users into OAuth mode and made them appear signed out.
- `onOpenJobs` could leave `askReturnPage` stale, causing an unnecessary Back press after Ask-to-Jobs navigation.
- Activity detail held a stale `JobUi` snapshot instead of re-resolving from live polling state.
- OAuth-first startup constructed `OAuthRepository` eagerly under Robolectric, causing a background Conscrypt exception before unrelated tests started.

## Technical Decisions

- Kept OAuth as the first-run default but used bearer mode when a missing `auth_mode` is paired with an existing bearer token.
- Added a lazy `OAuthTokenSource` wrapper so app startup does not instantiate AppAuth/OkHttp until OAuth is actually used.
- Used existing Jobs detail `JobRef` style for Activity detail to keep selected status live.
- Changed copied health diagnostics to derive an origin-only URL before composing a curl command.
- Left test connection as a health/reachability check and changed copy to say so explicitly.

## Files Changed

| status | path | purpose |
|---|---|---|
| modified | apps/android/app/src/main/java/com/axon/app/data/auth/AuthMode.kt | Preserve upgraded bearer installs with missing auth mode. |
| modified | apps/android/app/src/main/java/com/axon/app/data/repository/SettingsRepository.kt | Resolve missing auth mode with bearer-token awareness. |
| modified | apps/android/app/src/main/java/com/axon/app/di/AppContainer.kt | Defer OAuth repository construction through a lazy token source. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/jobs/ActivityHistoryScreen.kt | Show refresh warnings, avoid false empty state, and keep detail live. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/jobs/ActivityModels.kt | Mark missing live jobs as local-only, not submitted. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsFormatters.kt | Centralize failed job status classification. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsScreen.kt | Show partial refresh warnings and shorten accessibility target text. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/nav/RailScaffold.kt | Clear stale ask return state on direct page navigation. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/settings/SettingsConnectionTab.kt | Label connection test as reachability-only. |
| modified | apps/android/app/src/main/java/com/axon/app/ui/status/TopChromeStatus.kt | Use origin-only health-check command text. |
| modified | docs/reference/aurora-primitive-inventory.json | Inventory `ShellBackBehavior.kt` sidebar references. |
| modified | Android unit tests | Added/updated coverage for auth migration, local-only rows, diagnostics origin handling, and transport failure. |

## Beads Activity

No bead activity was created or modified during this closeout. Existing in-progress beads listed by `bd list --status in_progress` were unrelated to PR #296.

## Repository Maintenance

- Plans: no completed plan movement was performed; the active Android command-console plan remains part of the PR history.
- Worktrees and branches: work remained in `/home/jmagar/workspace/axon/.worktrees/codex-android-ui-qa-report` on `codex/android-ui-qa-report`.
- Stale docs: updated only the Aurora primitive inventory that CI proved stale.
- Generated output: Gradle/build/cache directories remain untracked local artifacts and were not staged.

## Tools and Skills Used

- Skills: `lavra:lavra-review`, `vibin:quick-push`, `vibin:gh-fix-ci`, and `save-to-md` workflow guidance.
- Agents: Lavra design, architecture, security, bug reproduction, simplicity, and PR review toolkit agents.
- Shell/GitHub CLI: PR metadata, CI checks, run logs, Gradle tests, lint, APK copy, and git state.
- Lumen semantic search: attempted first for code discovery; Android/OAuth queries returned no useful results or embedding backend errors, so exact known-file reads were used.

## Commands Executed

| command | result |
|---|---|
| `gh pr view 296 --json ...` | Loaded PR metadata and changed files. |
| `gh pr checks 296 --json ...` | Found failing `aurora-primitive-inventory` and `ci-gate`. |
| `gh run view 28465770570 --job 84365049642 --log` | Found missing inventory entries for `ShellBackBehavior.kt`. |
| `python3 scripts/check_aurora_primitive_inventory.py` | Failed before inventory fix, passed after. |
| `apps/android/gradlew -p apps/android :app:testDebugUnitTest ...` | Targeted tests passed after fixes. |
| `apps/android/gradlew -p apps/android :app:testDebugUnitTest --no-daemon --stacktrace` | Full Android unit tests passed after OAuth lazy-init fix. |
| `apps/android/gradlew -p apps/android :app:lintDebug :app:copyDebugApkToRepoBin :app:copyReleaseApkToRepoBin --no-daemon --stacktrace` | Lint and APK refresh passed. |

## Errors Encountered

- Lumen semantic search returned no results and once failed because the embedding backend reset the connection.
- The first targeted status diagnostics test failed because malformed URL fallback kept path text; fixed by extracting `scheme://authority`.
- Full Android tests failed with a suppressed Conscrypt exception from eager OAuth startup; fixed by lazy token-source delegation.

## Behavior Changes

| area | before | after |
|---|---|---|
| Auth migration | Existing bearer installs with no `auth_mode` defaulted to OAuth. | Existing bearer token preserves bearer mode; new installs remain OAuth-first. |
| Activity | Missing live jobs looked like submitted jobs. | Missing live jobs are marked local-only with unavailable server status. |
| Jobs and Activity | Partial refresh errors could be hidden when some data existed. | Warning/error cards render even with partial data. |
| Diagnostics | Copied curl command could include user-controlled URL path text. | Copied command uses origin-only health URL. |
| Startup tests | OAuth default could eagerly construct OAuthRepository. | OAuth token source constructs repository lazily. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `python3 scripts/check_aurora_primitive_inventory.py` | Inventory guard passes. | Passed. | pass |
| Targeted `:app:testDebugUnitTest --tests ...` | Review-fix tests pass. | Passed. | pass |
| Full `:app:testDebugUnitTest` | All unit tests pass. | Passed. | pass |
| `:app:lintDebug :app:copyDebugApkToRepoBin :app:copyReleaseApkToRepoBin` | Lint clean and APKs refreshed. | Passed. | pass |

## Risks and Rollback

Risk is low and isolated to Android UI/auth state. Roll back by reverting the forthcoming review-fix commit if CI or runtime smoke reveals an unexpected Android regression.

## Next Steps

1. Commit and push the review fixes.
2. Wait for PR #296 checks to rerun.
3. If CI is green and mergeable, merge PR #296 into `main`.
