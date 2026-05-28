---
date: 2026-05-27 16:02:38 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 9c539c2c
agent: Claude (Opus 4.7 1M)
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
pr: none
---

# Android pager shell + FAB mode selector + Document view

## User Request

Restructure the Android app: remove the bottom navigation bar; add a FAB on the
Operations page that switches between Axon operations (Ask, Summarize, Research,
Query, Scrape, Crawl, Ingest, Search, Map); add three more swipe pages (Jobs,
Knowledge, System). Settings cog must live on the top app bar of every page.
Rename existing "Search" to "Query" (it always called `/v1/query`) and reserve
"Search" for real web search. Tapping a Query hit must open the full document
in-app via `/v1/retrieve` instead of opening the source URL in a browser.

## Session Overview

Shipped the skeleton: bottom bar removed; four-page `HorizontalPager` shell;
top app bar with page-dot indicator + settings gear on every page; FAB-driven
mode selector (`AuroraSheet` + grid of `AuroraCard` tiles); active mode persists
in a `ViewModel`; existing screens reused as mode bodies. Renamed
`ui/search/` → `ui/query/` and rebuilt the Query result card so taps open a new
in-app `DocumentScreen` powered by `/v1/retrieve`. Compiles, unit tests pass,
debug APK built and uploaded to gdrive.

## Sequence of Events

- Confirmed every operation already has a server endpoint by reading
  `src/web/server/routing.rs`, `src/web/CLAUDE.md`, and the handler modules — no
  new server routes required. Streaming exists only for `/v1/ask/stream`;
  research/summarize have no streaming endpoint.
- Audited Aurora Android components (`~/workspace/aurora-design-system/android/aurora/src/main/kotlin/tv/tootie/aurora/components/`); confirmed no FAB primitive — Material3 `ExtendedFloatingActionButton` is correct.
- Built the Operations shell: `OperationMode` enum (9 modes with distinct
  Material icons), `OperationsViewModel` (active mode state), `ModePickerSheet`
  (Aurora-based grid), `OperationsScreen` (FAB + active-mode renderer),
  `StubModeForm` for Summarize/Ingest/Search.
- Renamed `ui/search/SearchScreen.kt` → `ui/query/QueryScreen.kt`, swapped class
  names, "Vector Search" → "Vector Query".
- Wrote `JobsScreen`/`KnowledgeScreen`/`SystemScreen` as Aurora-styled stubs.
- Rewrote `AxonNavGraph.kt`: `Scaffold` + `CenterAlignedTopAppBar` + page dots +
  settings gear → `SettingsRoute`; `HorizontalPager` hosting the four pages.
- Compiled, fixed `ExperimentalMaterial3Api` opt-in for `AuroraSheet`, and
  re-ran tests — green.
- Added `/v1/retrieve` wire shapes (`RetrieveRequest`/`RetrieveResponse`) +
  `AxonClient.retrieve()` + `AxonRepository.retrieve()` → `RetrieveResultUi`.
- Added `LocalAxonNavController` `CompositionLocal` to avoid prop-drilling
  through `OperationsScreen` → mode bodies.
- Added `DocumentRoute(url)` + `DocumentShell` (with back arrow) +
  `DocumentScreen`/`DocumentViewModel` (loads on first composition, renders
  matched URL, chunk count, truncated/warning callouts, the assembled body,
  and an outlined Aurora button to open the source URL in the browser).
- Swapped `QueryHitCard`'s click from `LocalUriHandler.openUri(hit.url)` to
  `navController.navigate(DocumentRoute(hit.url))`.
- Fixed `AuroraCalloutVariant.Warning` → `Warn` (Aurora's enum is `Warn`).
- `./gradlew :app:compileDebugKotlin` and `:app:testDebugUnitTest` both green.
- `./gradlew :app:assembleDebug` produced `apps/android/app/build/outputs/apk/debug/app-debug.apk` (20.4 MiB).
- Uploaded APK to `gdrive:axon-apks/axon-android-v4.8.2-20260527-1602.apk`.

## Key Findings

- All operations the user listed are already served by routes registered in
  `src/web/server/routing.rs:39-90` — including `/v1/ingest/*`, `/v1/summarize`,
  `/v1/search`, `/v1/suggest`, `/v1/domains`, `/v1/stats`, `/v1/status`,
  `/v1/doctor`. No new server endpoints required.
- Only `/v1/ask/stream` exists for streaming
  (`src/web/server/handlers/ask_stream.rs:54`); research and summarize have no
  SSE endpoint server-side.
- The CLI follow-up flow (`src/cli/commands/ask.rs:11` + `ask/followup.rs`) is
  filesystem-local — there is no `session`/`follow_up` field on `/v1/ask`. Any
  Android follow-up needs to be implemented client-side by inlining prior turns
  into the next query.
- `RetrieveResult` wire shape is fully defined in `src/services/types/service.rs:441-466`; assembled doc is the `content: String` field plus a chunk count and `matched_url`/truncation/warnings metadata.
- Aurora Android library has no FAB primitive; the closest navigation surface
  primitives are `AuroraTabs`, `AuroraSheet`, `AuroraToolbar`, and
  `AuroraCommandPalette`. The mode picker uses `AuroraSheet` + `AuroraCard`
  grid because the visual-tile UX doesn't match a search-driven command list.
- `EmptyContent` in `ui/common/StateContent.kt:76-93` already wraps
  `AuroraEmptyState`, so reusing it preserves Aurora styling without import
  churn.

## Technical Decisions

- **`HorizontalPager` + `TopAppBar` over a `BottomNavigation`/`NavigationRail`** — the user explicitly asked for swipe pages with no bottom bar; the top bar carries the settings gear instead.
- **Active mode held in `OperationsViewModel`, not `rememberSaveable`** — it survives configuration changes and recompositions but resets when the activity is destroyed (no UX requirement for "last mode" persistence yet).
- **Reused existing tab composables (`ScrapeTab`, `CrawlTab`, `MapTab`, `ResearchTab`) instead of cloning them.** Each takes a `ToolsViewModel`; `OperationsScreen` provides one shared instance so per-tab form state survives mode switches.
- **`LocalAxonNavController` `CompositionLocal` instead of prop-drilling** — `QueryScreen` is two levels deep under the nav graph (via `OperationsScreen`); threading a lambda would have been noisier than a typed local. The local throws when unprovided so misuse fails loudly.
- **`DocumentScreen` ViewModel uses an explicit `load(url)` method, not a constructor argument** — avoids writing a `SavedStateHandle` factory; calling `load` from a `LaunchedEffect(url)` keyed on the URL is enough.
- **Reused `httpLong` client (300s read timeout) for `/v1/retrieve`** — assembled documents can be large enough that the 60s default risks tripping the read timeout on slow links.
- **Kept the source-URL escape hatch on `DocumentScreen`** (outlined Aurora button) so users can still jump to the live page when they want to.
- **Search/Summarize/Ingest get `StubModeForm` placeholders** rather than half-wired forms — guessing at request bodies would be sloppy when the shapes can be read from `client_contract.rs`. The full wiring is queued in `axon_rust-ivjr`.

## Files Modified

- **Created** `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationMode.kt` — 9-mode enum with icons.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsViewModel.kt` — active mode state.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/operations/ModePickerSheet.kt` — Aurora bottom sheet with mode tiles.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/operations/OperationsScreen.kt` — FAB host that renders the active mode's screen.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/operations/StubModeForm.kt` — placeholder body for not-yet-wired modes.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/jobs/JobsScreen.kt` — page 1 stub.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/knowledge/KnowledgeScreen.kt` — page 2 stub.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/system/SystemScreen.kt` — page 3 stub.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/nav/AxonNav.kt` — `LocalAxonNavController` CompositionLocal.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/document/DocumentScreen.kt` — in-app document view.
- **Created** `apps/android/app/src/main/java/com/axon/app/ui/document/DocumentViewModel.kt` — loads `/v1/retrieve` once per URL.
- **Renamed** `ui/search/SearchScreen.kt` → `ui/query/QueryScreen.kt`; class/state renamed; title text updated.
- **Renamed** `ui/search/SearchViewModel.kt` → `ui/query/QueryViewModel.kt`; class/state renamed.
- **Rewrote** `ui/nav/AxonNavGraph.kt` — pager + top bar shell + `SettingsShell` + `DocumentShell`; `CompositionLocalProvider` for nav controller.
- **Modified** `data/remote/AxonModels.kt` — added `RetrieveRequest` + `RetrieveResponse`.
- **Modified** `data/remote/AxonClient.kt` — added `retrieve(...)` using the long-timeout client.
- **Modified** `data/repository/AxonRepository.kt` — added `RetrieveResultUi` + `retrieve(...)`.
- **Modified** `ui/query/QueryScreen.kt` — card click navigates to `DocumentRoute` instead of opening the URL.

## Commands Executed

- `./gradlew :app:compileDebugKotlin --no-daemon` — green after Aurora opt-in fix and `AuroraCalloutVariant.Warn` rename.
- `./gradlew :app:testDebugUnitTest --no-daemon` — green.
- `./gradlew :app:assembleDebug --no-daemon` — produced `app-debug.apk` (20.4 MiB).
- `rclone copyto apps/android/app/build/outputs/apk/debug/app-debug.apk gdrive:axon-apks/axon-android-v4.8.2-20260527-1602.apk -P` — uploaded.
- `bd create … --priority=2` and `bd update --claim` — tracked work as `axon_rust-ivjr`; spinoff `axon_rust-ywis` (closed).

## Errors Encountered

- **`AuroraSheet` experimental API** — `rememberModalBottomSheetState()` requires `@OptIn(ExperimentalMaterial3Api::class)` on the caller. Fixed by re-adding the opt-in marker to `ModePickerSheet`.
- **`AuroraCalloutVariant.Warning` does not exist** — Aurora's enum is `Info, Success, Warn, Error, Neutral`. Fixed by switching all callsites to `Warn`.

## Behavior Changes (Before/After)

| Surface | Before | After |
|---|---|---|
| Navigation chrome | Bottom `NavigationBar` with 5 tabs | Top `CenterAlignedTopAppBar` with page-dot indicator + settings gear; swipeable `HorizontalPager` |
| Operations entry | One screen per nav tab | One Operations page; FAB switches between 9 modes; active mode persists |
| Search vs query | "Search" tab called `/v1/query` and was labelled "Vector Search" | Renamed to "Query" with "Vector Query" label; "Search" mode reserved for real web `/v1/search` (stubbed) |
| Query result tap | Opened `hit.url` in external browser | Pushes `DocumentRoute(hit.url)` → in-app `DocumentScreen` rendering the full assembled document from `/v1/retrieve` |
| Settings access | Bottom-bar tab | Gear icon on every page's top bar |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `./gradlew :app:compileDebugKotlin` | BUILD SUCCESSFUL | BUILD SUCCESSFUL in 11s | ✅ |
| `./gradlew :app:testDebugUnitTest` | BUILD SUCCESSFUL | BUILD SUCCESSFUL in 10s | ✅ |
| `./gradlew :app:assembleDebug` | apk emitted | `app-debug.apk` 20.4 MiB | ✅ |
| `rclone copyto … gdrive:axon-apks/…` | 100% transferred | `Transferred: 1/1, 100%` | ✅ |
| `grep -rn 'SearchViewModel\|SearchUiState\|ui.search' apps/android/app/src` | no matches | no matches | ✅ |

## Risks and Rollback

- Visual restructure of the entry-point nav graph touches every user flow. Rollback is a single `git revert` of the upcoming commit — no schema, no migrations, no shared state changed.
- `/v1/retrieve` returns large bodies; the existing 300s `httpLong` timeout was reused, but slow tethered links could still trip it. The screen surfaces server errors clearly via `ErrorContent`.
- `LocalAxonNavController` throws when read outside a provider — any new top-level Composable that hosts deep query-card-style links must be wrapped under `AxonNavGraph`'s provider.

## Decisions Not Taken

- **Persist active mode via DataStore** — rejected for the skeleton pass; resetting to Ask on cold start is acceptable and the UX requirement is not in scope.
- **Top-bar `AuroraTabs` instead of dot indicator** — rejected; the swipe pages are equally weighted, and tabs would invite duplicate selection affordances (tap-to-switch + swipe).
- **Per-mode ViewModel proliferation** — rejected; reused the existing `ToolsViewModel` for the four crawl/scrape/map/research modes so tab state survives.
- **Wire Summarize/Ingest/real-Search now** — deferred; their request bodies are clearly typed in `src/services/client_contract.rs` and will be wired in the next pass without guessing.

## References

- `src/web/server/routing.rs:25-100` — full route tree.
- `src/web/CLAUDE.md` — auth scope rules + services-first contract.
- `src/services/types/service.rs:441-466` — `RetrieveResult` wire shape.
- `src/services/client_contract.rs:117-148` — `RestRetrieveRequest`/`RestMapRequest`/`RestSuggestRequest`.
- `~/workspace/aurora-design-system/android/aurora/src/main/kotlin/tv/tootie/aurora/components/` — Aurora Android component inventory.
- Beads: `axon_rust-ivjr` (shell skeleton), `axon_rust-ywis` (document view, closed).

## Open Questions

- Should the mode-options screen (per-mode flag form, reachable via cog left of Send) write to per-mode DataStore preferences or pass values via the existing `Config` request fields only? — likely the latter, but confirm before implementing.
- Should Ask mode's in-app follow-up state persist across activity recreation (Room) or stay in-VM only? — defaulting to in-VM for now unless the user asks otherwise.

## Next Steps

**Started but not completed (in this session's diff):**
- None — every file in the diff is in a compiling, tested state.

**Follow-on tasks not yet started (queued under `axon_rust-ivjr`):**
1. Cog button left of Send → mode-options screen with per-mode flag forms; Crawl gets the full flag list the user specified; defaults loaded from server `Config` defaults.
2. Ask mode: track turns in `AskViewModel`, inline prior Q/A into the next query (mirrors CLI's `followup::follow_up_query`).
3. Kotlin client + repo + UI wiring for: `summarize`, `ingest`, real web `search`, `suggest`, `domains`, `stats`, `status`, `doctor`, `panel/stack`, `config`.
4. Populate `JobsScreen` (via `/v1/status` + `/v1/{crawl,embed,extract,ingest}/list`), `KnowledgeScreen` (Suggest/Sources/Domains/Stats), and `SystemScreen` (Debug/Doctor/Smoke/Stack/Config) bodies once #3 lands.
5. Server-side ask: consider adding `/v1/research/stream` and `/v1/summarize/stream` SSE endpoints so the Android client can stream those too (currently non-streaming).
