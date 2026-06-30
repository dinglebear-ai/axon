# Android Command Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Android app feel like an Axon command console instead of a plain Compose utility.

**Architecture:** Add small reusable visual primitives for branded surfaces, then apply them to the shell, Ask console, and high-traffic list screens. Keep existing navigation, data flow, and ViewModels intact.

**Tech Stack:** Kotlin, Jetpack Compose, Material 3, existing Axon theme/elevation utilities.

## Global Constraints

- Work in `/home/jmagar/workspace/axon/.worktrees/codex-android-ui-qa-report`.
- Preserve the existing navigation and accessibility fixes.
- Do not introduce a new design dependency.
- Verify with Android unit tests, lint, APK build, and live emulator screenshots.

---

### Task 1: Reusable Console Primitives

**Files:**
- Create: `apps/android/app/src/main/java/com/axon/app/ui/common/CommandConsoleChrome.kt`

**Interfaces:**
- Produces: `CommandConsoleBackground`, `CommandConsoleHeader`, `MetricPill`, `SignalRail`.

- [ ] Create the composables above using existing `AxonTheme`, `tint`, and `axonElevation`.
- [ ] Ensure every interactive element has semantics supplied by callers.
- [ ] Compile with `apps/android/gradlew -p apps/android :app:compileDebugKotlin --no-daemon`.

### Task 2: Shell Identity

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/RailScaffold.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/nav/ShellSidebar.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/status/TopChromeStatus.kt`

**Interfaces:**
- Consumes: `SignalRail`, `MetricPill`.

- [ ] Give top chrome a layered command-console frame.
- [ ] Add a compact status ribbon feel without changing status behavior.
- [ ] Make sidebar rows feel selected/active through accent rails, glow, and stronger depth.
- [ ] Preserve existing TalkBack labels.

### Task 3: Ask Agent Console

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/ask/AskScreen.kt`

**Interfaces:**
- Consumes: `CommandConsoleBackground`, `CommandConsoleHeader`, `MetricPill`.

- [ ] Add an agent-console header with live identity chips.
- [ ] Give the composer/message area stronger layered depth.
- [ ] Preserve prompt submission, attachments, mode picker, and session behavior.

### Task 4: Operational Screens

**Files:**
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/jobs/ActivityHistoryScreen.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsScreen.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/sessions/SessionsDrawerContent.kt`
- Modify: `apps/android/app/src/main/java/com/axon/app/ui/settings/SettingsScreen.kt`

**Interfaces:**
- Consumes: `CommandConsoleHeader`, `MetricPill`.

- [ ] Add richer headers and metrics to Activity, Jobs, Sessions, and Settings.
- [ ] Keep dense operational scanning; no landing-page treatment.
- [ ] Preserve existing nested-back and accessibility behavior.

### Task 5: Verify and Close

**Files:**
- Update QA evidence under `reports/android-nav-qa/`.

- [ ] Run `apps/android/gradlew -p apps/android :app:testDebugUnitTest :app:lintDebug :app:copyDebugApkToRepoBin :app:copyReleaseApkToRepoBin --no-daemon --stacktrace`.
- [ ] Install the debug APK on the Android emulator and capture screenshots for Ask, sidebar, Jobs, Activity, Sessions, Settings, and launcher.
- [ ] Commit and push the branch.
