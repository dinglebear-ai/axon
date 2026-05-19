# Session Log — 2026-02-26 UI Density, Permissions, Context Pass

## 1. Session overview
- Completed UI/UX refinement for Pulse + omnibox with higher information density and mobile-first behavior.
- Removed Pulse from mode-picker interaction path and kept Pulse as implicit chat workspace flow.
- Implemented condensed model selector and Claude-style permission modes (`plan`, `accept-edits`, `bypass-permissions`).
- Added context utilization bar + quick actions in omnibox after first turn only.
- Added robust chat UX upgrades: inline error actions, jump-to-latest, thread-scoped removable source chips.

## 2. Timeline of major activities
- Audited current diffs and validated active app instance at `http://dookie:3000` using Chrome DevTools.
- Implemented workspace/layout/toolbar/omnibox/chat refinements across `apps/web/components/*` and `apps/web/lib/pulse/*`.
- Extended API contract to include `threadSources` input and response `metadata` for context budgeting.
- Updated tests for permission/model/schema/prompt-intent changes and ran full `apps/web` test suite.
- Revalidated desktop + mobile rendering and console state in DevTools after code changes.

## 3. Key findings with `path:line` references
- Permission and model schema now explicit in shared types: `apps/web/lib/pulse/types.ts:28`, `apps/web/lib/pulse/types.ts:31`.
- Chat request now carries `threadSources` and response includes `metadata`: `apps/web/lib/pulse/types.ts:39`, `apps/web/lib/pulse/types.ts:79`.
- API applies model-specific context budgets and emits metadata: `apps/web/app/api/pulse/chat/route.ts:24`, `apps/web/app/api/pulse/chat/route.ts:292`, `apps/web/app/api/pulse/chat/route.ts:301`.
- Omnibox context bar uses measured totals/budget and exposes quick actions: `apps/web/components/omnibox.tsx:790`, `apps/web/components/omnibox.tsx:805`, `apps/web/components/omnibox.tsx:810`.
- Chat pane includes retry/copy error controls, source chip removal, and jump-to-latest: `apps/web/components/pulse/pulse-chat-pane.tsx:182`, `apps/web/components/pulse/pulse-chat-pane.tsx:273`, `apps/web/components/pulse/pulse-chat-pane.tsx:346`.

## 4. Technical decisions and rationale
- Used API-supplied context metadata instead of purely client heuristics to reduce misleading context utilization values.
- Kept permission model constrained to three explicit states to mirror requested Claude-style flow without `ask` mode UI complexity.
- Chose thread-scoped source removal (soft remove) rather than global unindex to avoid destructive data side effects.
- Added per-breakpoint split persistence (`desktop` + `mobile`) to avoid cross-device layout mismatch.
- Added compact segmented model selector for speed/space instead of a full-width dropdown.

## 5. Files modified/created and purpose
- `apps/web/components/omnibox.tsx`: context bar gating, context metrics display, quick actions, keyboard focus/send shortcuts.
- `apps/web/components/pulse/pulse-toolbar.tsx`: condensed model selector, compact permissions group, latency/model feedback, permission description.
- `apps/web/components/pulse/pulse-workspace.tsx`: thread source state, metadata wiring, chat error shape, split persistence by breakpoint.
- `apps/web/components/pulse/pulse-chat-pane.tsx` (rewritten): removable source chips, retry/copy error UX, jump-to-latest behavior.
- `apps/web/app/api/pulse/chat/route.ts`, `apps/web/lib/pulse/types.ts`, `apps/web/lib/pulse/rag.ts`, `apps/web/lib/pulse/prompt-intent.ts`: request/response contract, context budget metadata, thread source prompt context, `+source` detection.

## 6. Critical commands executed and outcomes
- `pnpm --dir /home/jmagar/workspace/axon_rust/apps/web exec tsc --noEmit` -> success (no type errors).
- `pnpm --dir /home/jmagar/workspace/axon_rust/apps/web exec vitest run` -> success, 15 files passed, 78 tests passed.
- `pnpm --dir /home/jmagar/workspace/axon_rust/apps/web exec vitest run __tests__/pulse-types.test.ts __tests__/pulse-permissions.test.ts __tests__/omnibox.test.ts` -> success, 3 files passed, 21 tests passed.
- Chrome DevTools checks on `http://dookie:3000` (`reload`, `snapshot`, `console`) -> UI present on desktop/mobile and no active runtime console errors beyond expected dev logs.
- Git and grep/read commands were used to confirm modified scope and line-level references.

## 7. Behavior changes (before/after)
- Before: context bar could appear before active chat and used approximate weighting.
- After: context bar appears only after first turn and displays measured `contextCharsTotal/contextBudgetChars` + last latency.
- Before: model control was a compact select and permissions less explicit.
- After: model and permissions are segmented compact controls with inline active-mode explanation.
- Before: chat errors rendered as plain text message.
- After: errors render with explicit Retry + Copy actions and prompt replay.
- Before: mobile had no explicit jump-to-latest affordance and no mobile split drag.
- After: mobile has jump-to-latest when scrolled up and a vertical split drag handle.

## 8. Verification evidence (`command | expected | actual | status`)
- `pnpm --dir ... tsc --noEmit | no TS errors | no output | PASS`
- `pnpm --dir ... vitest run | all tests pass | 15 files, 78 tests passed | PASS`
- `pnpm --dir ... vitest run pulse/omnibox subset | targeted suites pass | 3 files, 21 tests passed | PASS`
- `Chrome DevTools reload + snapshot | Pulse workspace visible and controls rendered | snapshot showed model/perm controls + chat/editor panes | PASS`
- `Chrome DevTools console | no recoverable/runtime errors | only React DevTools hint + HMR connected logs | PASS`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Axon embed queued/completed for this file with `job_id=cf280788-b260-453a-b7ab-c86e4c7cb3f4`.
- Embed status (`./scripts/axon embed status ... --json`) reported `status=completed`, `result_json.collection=cortex`, `result_json.source=rust`.
- Status payload did not expose `data.url`; verified retrieval with concrete saved-path source ID instead.
- Retrieve verification command: `./scripts/axon retrieve "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-ui-density-permissions-context-pass.md" --collection "cortex"` -> returned `Chunks: 1` and matching path.

## 10. Risks and rollback
- Risk: larger context payloads may increase API latency; metadata now exposes this but does not enforce truncation.
- Risk: retry button may replay failing prompts repeatedly if upstream failures persist.
- Risk: mobile split drag could still conflict with touch scroll on some devices.
- Rollback: revert web UI/API pulse changes by resetting touched files in `apps/web/components/pulse/*`, `apps/web/components/omnibox.tsx`, and `apps/web/lib/pulse/*` to prior commit.
- Rollback safety: schema updates are additive in API response metadata and optional fields.

## 11. Decisions not taken
- Did not add `ask mode` because requested UX explicitly excluded it.
- Did not implement destructive source deletion from index; used thread-scoped source removal only.
- Did not add cloud/SaaS telemetry; kept local-only behavior.
- Did not force model hard-lock per permission mode; selector remains user-controlled.
- Did not introduce extra backend persistence for thread source state beyond session flow.

## 12. Open questions
- Should retry include capped exponential backoff to avoid rapid failure loops?
- Should context budget enforcement truncate context server-side when over budget instead of only displaying metrics?
- Should quick-action chips be user-configurable and persisted?
- Should model/permission selections persist globally or per workspace/document?
- Should jump-to-latest auto-hide timeout be configurable?

## 13. Next steps
- Add integration test to assert `threadSources` is sent from workspace state into `/api/pulse/chat` request payload.
- Add explicit `measuring...` UI state until first metadata-bearing assistant response arrives.
- Add retry throttle/backoff and disable Retry while request is in-flight.
- Add overflow indicator for hidden source chips (`+N`) when active thread sources exceed visible chips.
- Tighten mobile drag gesture gating (long-press/drag-handle threshold) to reduce scroll conflicts.
