---
date: 2026-05-28 15:09:21 EST
repo: git@github.com:jmagar/axon.git
branch: feat/android-rail-redesign
head: f657bc99
plan: docs/superpowers/plans/2026-05-28-android-phase3-completion.md
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust f657bc99 [feat/android-rail-redesign]
pr: none
beads: axon_rust-21u8 (closed, referenced for Phase 2 completion context)
---

# Android Phase 3 planning — FAB ring bugs, stub-filling, Jobs drawer gap analysis

## User Request

Continue testing the Axon Android app after edge-to-edge fixes, identify all remaining bugs and spec gaps, write a Phase 3 completion plan, and answer whether Phase 3 would fully finish the redesign.

## Session Overview

The session resumed from compaction mid-test-pass. ADB-based testing confirmed FAB ring functionality while uncovering two bugs (no `BackHandler`, ring items clip at screen edge due to FAB-corner origin). Inspection of `SessionsDrawerContent.kt` and `JobsDrawerContent.kt` revealed the Sessions drawer is fully spec-compliant but the Jobs drawer has a structural gap against the spec (flat list vs. two-level type-aggregate + drill-down). The `writing-plans` skill produced `docs/superpowers/plans/2026-05-28-android-phase3-completion.md` covering 8 tasks. The session ended by confirming the Phase 3 plan covers all remaining work *except* the Jobs drawer two-level navigation — flagged as the one open design choice for the user.

## Sequence of Events

1. **Session resumed.** Context recovered from compaction; prior work had reached the FAB ring test step with Back key accidentally exiting the app to ConnectBot.
2. **App relaunched via ADB.** Emulator `emulator-5554` (Pixel 6 API 35, 1080×2400 @ 420dpi) relaunched with `am start`.
3. **Recent query card behavior clarified.** Tapping a card fills the input field (quick-fill), not session history reload — confirmed intentional by reading the UI dump.
4. **FAB ring opened successfully.** Tapped FAB at (983, 2135) after dismissing keyboard; ring expanded. Only items 6–9 (Retrieve/Summarize/Crawl/Ingest at 126°–234°) were visible; items 0–5 clip off the right/bottom edges.
5. **FAB ring geometry root-caused.** `FabRing.kt` uses `angleDeg = -90.0 + i * 36.0` (clockwise from 12 o'clock) with origin at `fabCenter` (FAB corner, ~983, ~2135). With `radiusDp = 96dp = 252px`, items in the right/upper semicircle land off-screen.
6. **Back key exits app from FAB ring.** No `BackHandler` exists in `FabLauncher.kt`; pressing Back when ring is open closes the entire app rather than collapsing the ring.
7. **FAB dismiss button tapped at wrong coordinates.** `(978, 2338)` hit the Retrieve action (ring is still open, button shifted), not the `×` dismiss. Workaround: tap-outside-to-cancel on the Retrieve dialog closes both dialog and ring.
8. **`writing-plans` skill invoked.** User: "aight /writing-plans for all the fixes and whatevs needs to be done to complete the redesign."
9. **Sessions drawer reviewed.** `SessionsDrawerContent.kt` confirmed spec-complete: title, `firstMessagePreview`, `turnCount`, long-press `DropdownMenu` with Pin/Unpin/Delete — all present.
10. **Jobs drawer gap identified.** `JobsDrawerContent.kt` is a flat `LazyColumn` of `JobsOverviewItem` entries. The spec defines two-level navigation: Level 1 shows 5 category rows (Crawls/Embeddings/Ingestions/Extractions/Watches) with aggregate badges and progress bars; Level 2 drills into per-type job lists. The current implementation satisfies neither level.
11. **Phase 3 plan written and saved.** `docs/superpowers/plans/2026-05-28-android-phase3-completion.md` — 8 tasks: BackHandler fix, screen-center ring, ManagementViewModel, ManagementDrawerContent, SetupViewModel, SetupDrawerContent, DrawerSectionContent wiring, ADB smoke tests. Dedupe/Sync deferred (no server endpoints).
12. **Redesign completeness answer delivered.** Phase 3 covers FAB ring + MGMT + SETUP + wiring. Jobs drawer two-level nav is the one remaining gap not in the plan; user must decide flat (ship now) vs. spec-compliant (add Task 9).

## Key Findings

- **`FabLauncher.kt` has no `BackHandler`** — pressing Back while ring is open exits the app. Fix: add `BackHandler(enabled = state is FabState.Ring) { state = FabState.Idle }`.
- **`FabRing.kt` uses FAB corner as ring origin** — `fabCenter` ≈ (983, 2135); with radius 96dp items at -90°–+90° clip off right edge. Fix: compute `screenCenter` via `BoxWithConstraints` in `FabLauncher`, pass it to `FabRing` when ring is open.
- **Sessions drawer is fully spec-compliant** — `SessionsDrawerContent.kt` implements all required fields and interactions.
- **Jobs drawer is a flat list** — `JobsDrawerContent.kt` shows one `LazyColumn` of active jobs with refresh. The spec's two-level type-aggregate nav (5 categories + drill-down) is not implemented.
- **Management and Setup drawers are empty stubs** — `ManagementDrawerContent.kt` and `SetupDrawerContent.kt` contain only a comment. Phase 3 Tasks 3–6 fill these.
- **Phase 2 plan already moved to `docs/plans/complete/`** — confirmed at `docs/plans/complete/2026-05-27-android-phase2-stubbed-modes.md`; epic bead `axon_rust-21u8` is closed.

## Technical Decisions

- **Screen-center approach for FAB ring** — wrapping `FabLauncher` in `BoxWithConstraints` gives `maxWidth`/`maxHeight` and lets us compute `screenCenter` without a second `onGloballyPositioned`. The `FabRing` already accepts a `centerOffset` parameter; passing `screenCenter` instead of `fabCenter` when the ring is open makes all 10 ops visible without changing the ring's geometry formula.
- **Dedupe/Sync marked "coming soon"** in ManagementDrawerContent — there is no server endpoint for either operation from the Android client; shipping placeholder chips avoids misleading UX while keeping the UI layout consistent with the spec.
- **Jobs drawer gap not added to Phase 3 plan** — the flat list is functional and was not originally contested. Adding two-level nav is non-trivial (requires new ViewModel with per-type drill-down, back-stack management, two composables). Left as an explicit decision for the user rather than silently expanding scope.

## Files Changed

| Status | Path | Purpose |
|--------|------|---------|
| created | `docs/superpowers/plans/2026-05-28-android-phase3-completion.md` | Phase 3 implementation plan: 8 tasks covering BackHandler, screen-center ring, MGMT/SETUP drawers, wiring, smoke tests |

No source code files were modified this session — all work was analysis, testing, and planning.

## Beads Activity

- **`axon_rust-21u8`** — Phase 2 epic, already closed before this session (close reason: PR #142 merged, children 21u8.1–21u8.9 all closed). Referenced to confirm Phase 2 is complete. No state change.
- **`axon_rust-21u8.10`** — Optional follow-up: stream research + summarize once server adds SSE. Remains open/deferred; not started this session.
- **`axon_rust-3lt7`** — Android: decouple `AxonClient.JobKind` from UI layer. Open P3. Not worked this session.
- No new beads created; no bead state changes made during this session.

## Repository Maintenance

**Plans.** `docs/plans/complete/2026-05-27-android-phase2-stubbed-modes.md` was already present — moved during a prior session. No additional moves warranted. `docs/superpowers/plans/2026-05-28-android-phase3-completion.md` is active and not moved. No other plans are evidently complete based on current session scope.

**Beads.** `axon_rust-21u8` is closed (confirmed via `bd show axon_rust-21u8`). Open Android follow-up beads `21u8.10` and `3lt7` were reviewed but not changed — no work was completed to justify closing them. No new beads created this session (planning sessions warrant a bead for Phase 3 execution, but left to user discretion for which execution approach they choose).

**Worktrees.** Three registered worktrees:
- `/home/jmagar/workspace/axon_rust` — `feat/android-rail-redesign` HEAD `f657bc99`, clean
- `.worktrees/android-phase3` — `feat/android-phase3-completion` HEAD `f2e2628f`, active Phase 3 execution branch
- `.worktrees/palette-crystalline` — `feat/palette-crystalline` HEAD `774098fb`, unrelated feature branch

None removed — all have unique branches with unmerged work.

**Stale docs.** `docs/plans/2026-05-27-android-phase2-stubbed-modes.md` was already moved; the active plan reference in the session header reflects the Phase 3 plan which is the current focus. No other docs required updates this session.

## Tools and Skills Used

- **ADB shell** — `input tap`, `input text`, `input keyevent KEYCODE_BACK`, `screencap`, `pull`, `am start` for emulator interaction.
- **ffmpeg** — `ffmpeg -i input.png -vf scale=540:1200 output.png` to resize 1080×2400 screenshots to fit the model's 2000px image limit.
- **File tools (Read)** — reading `FabLauncher.kt`, `FabRing.kt`, `FabOp.kt`, `SessionsDrawerContent.kt`, `JobsDrawerContent.kt`, `ManagementDrawerContent.kt`, `SetupDrawerContent.kt` to verify spec compliance.
- **Bash** — `bd show`, `bd list` for bead inspection; `ls` for plan/session file listing.
- **Skill: writing-plans** — invoked to produce the Phase 3 implementation plan document.
- **Skill: save-to-md** — this file.

## Commands Executed

| Command | Result |
|---------|--------|
| `adb shell am start -n com.axon.app/.MainActivity` | App relaunched on emulator |
| `adb shell screencap /sdcard/screen.png && adb pull /sdcard/screen.png` | Screenshot captured for UI verification |
| `ffmpeg -i screen.png -vf scale=540:1200 screen_small.png` | Screenshot resized to within model image limit |
| `adb shell input keyevent KEYCODE_BACK` | Keyboard dismissed before tapping FAB |
| `adb shell input tap 983 2135` | FAB tapped; ring opened |
| `bd show axon_rust-21u8` | Confirmed Phase 2 epic is closed |
| `bd list --status=open` | Confirmed no Android Phase 3 epic bead exists yet |
| `ls docs/sessions/ \| grep 2026-05-28` | Confirmed existing today session docs for filename uniqueness |

## Errors Encountered

- **Tapping (978, 2338) to dismiss FAB ring hit Retrieve action instead.** Root cause: the `×` dismiss button in `FabRing.kt` draws at `fabCenterOffset - 21dp` ≈ (962, 2114) — above the ring, partially off-screen when ring is at FAB corner. The displayed `×` is clipped; coordinates assumed to be the dismiss were actually on the Retrieve button. Workaround: tap-outside-cancel on the resulting dialog dismissed both dialog and ring.

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Session planning scope | Phase 2 complete; no plan for remaining bugs | `docs/superpowers/plans/2026-05-28-android-phase3-completion.md` created |

No code changes this session — only analysis and planning.

## Risks and Rollback

No code was modified. The plan document is the only artifact. Rollback: `git rm docs/superpowers/plans/2026-05-28-android-phase3-completion.md` if the Phase 3 direction is abandoned.

## Open Questions

- **Jobs drawer: flat list vs. two-level type-aggregate nav?** The spec defines Level 1 (5 category rows with badges + progress bars) and Level 2 (per-type drill-down). Current implementation is a flat list. Does the flat list satisfy the requirement, or should a Task 9 be added to Phase 3?
- **Phase 3 execution approach?** The plan was written but execution approach (subagent-driven vs. inline) was not selected before the session ended.
- **Phase 3 bead.** No epic bead was created for Phase 3. Should `bd create` be run before execution begins?

## Next Steps

1. **Decide on Jobs drawer approach** — flat list (acceptable, ship as-is) or add Task 9 to Phase 3 for two-level type-aggregate nav. This decision gates whether Phase 3 fully closes the redesign.
2. **Execute Phase 3 plan** in `.worktrees/android-phase3` on branch `feat/android-phase3-completion` (already exists at `f2e2628f`). Skill options: `superpowers:subagent-driven-development` (recommended) or inline execution.
3. **Create Phase 3 bead** before starting execution: `bd create --title="Android Phase 3: FAB ring fixes + MGMT/SETUP drawers" --type=epic --priority=2`.
4. **After Phase 3 complete** — open PR from `feat/android-phase3-completion` → `feat/android-rail-redesign`, merge rail-redesign to main, then APK rebuild + upload to Google Drive.
