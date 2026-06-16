# Phase 1: Code Quality & Architecture Review

Target: `apps/palette-tauri` (axon-palette-tauri v5.9.1) — React 19 + Tauri 2 + Tailwind v4 + Aurora command palette. UI/UX focus.

**Overall verdict:** Competently engineered, well-tested frontend, well above average for a Tauri side-app. Strengths: real unidirectional data flow, clean Tauri/browser invoke seam, strong Aurora token discipline, good result-view composition, model-quality inline comments, solid logic-layer test coverage. Weaknesses are accreted duplication, a scattered subcommand dispatch, an overloaded `App.tsx`, and a "split-brain" design-system layer.

---

## Code Quality Findings

### Critical
- **C1 — Dead, divergent `RunState` export.** `src/lib/paletteView.ts:12-15` exports a `RunState` union (`idle|running|queued|success|error`) that is structurally incompatible with the canonical one in `src/lib/runState.ts:5-44` (`idle|running|streaming|job|success|error` — no `queued`, has `streaming`/`job`). Every consumer imports from `runState.ts`; the `paletteView` copy and its `runTone`/`outputTitle`/`outputSubtitle` helpers (lines 185-200) are dead and typed against a phantom `queued` kind. A future `import { RunState } from "@/lib/paletteView"` compiles but is wrong. **Fix:** delete the local union + helpers from `paletteView.ts`, re-export canonical type.

### High
- **H1 — `hostLabel` defined 4× with two behaviors.** `lib/url.ts:1` & `paletteView.ts:173` use `.host` (keeps `:port`); `OperationResultViewShared.tsx:219` & `OutputPanel.tsx:244` use `.hostname` (strips port). User-visible inconsistency (`example.com:8080` vs `example.com`). `lib/url.ts` is **entirely dead** (zero imports). **Fix:** delete `lib/url.ts`, standardize on `.hostname`, export once, import everywhere.
- **H2 — Copy-pasted helpers, some divergent.** `firstUrl` (paletteView:181, OutputPanel:252), `firstArray` (OperationResultViewShared:207 exported, re-copied OutputPanel:231), `shortId` (StatusView:3 `>12`/`…`/`"—"` vs OperationResultViewShared:312 `>14`/`...`/`undefined` → same ID truncates differently in two panels), `titleCase` (CrawlJobView:120 all-words vs OperationResultViewShared:317 first-char-only), `isRecord` (5 copies). **Fix:** consolidate into `lib/payload.ts`; standardize `shortId` threshold/glyph.
- **H3 — Global keydown listener re-binds every keystroke.** `App.tsx:98-134` effect deps include `query` and `run`, which change on every keystroke/stream delta, tearing down + re-adding the `window` keydown listener constantly. Stale-closure risk as branches grow. **Fix:** read volatile values through a ref; bind once with `[]`.

### Medium
- **M1 — `App.tsx` view-flag boolean soup.** `App.tsx:150-178` computes ~10 interdependent booleans (`hasQuery`, `jobMinimized`, `jobExpanded`, `showOutput`, `enteringArgument`, `showContent`, `compact`, `showResultsLayout`, `showActionPanel`) as ad-hoc boolean algebra; 11 setters drilled into hooks/panels. **Fix:** extract pure `derivePaletteView(state)` into `paletteView.ts` returning a `layout` discriminant + orthogonal booleans.
- **M2 — `outputTitle`/`outputSubtitle` re-implemented.** `OutputPanel.tsx:193-201` duplicates `paletteView.ts:192-200` verbatim. **Fix:** keep the real-typed `OutputPanel` versions, move into `runState.ts`/`runView.ts`, delete `paletteView` copies (ties to C1).
- **M3 — Magic numbers / inline widths.** `App.tsx:368` & `CrawlJobView.tsx:80` repeat `Math.max(2, pct)%` (min-bar floor `2`). `useWindowChrome.ts:35-53` is a wall of magic sizes; `LIST_CAP=338` (line 92) must stay in sync with CSS `.action-scroll` max-height with no enforcement. **Fix:** name constants; surface `LIST_CAP` as a CSS custom property.
- **M4 — Array-index keys where stable IDs exist.** `StatusView.tsx:58,70` use `key={i}` for error/job rows; job rows have `job_id` in hand. **Fix:** key on `job_id` / string content.
- **M5 — Dead & split CSS in `styles.css`.** 22 selectors defined >1×. Dead families (zero TSX refs): `.ask-tool-row` + children (1836-1878), `.ask-activity` (1847), `.ask-code-mini` (1880), `.command-action-empty` (~40+ dead lines). Split-definition selectors (`.output-panel` 1376 & 1393, `.panel-heading`, `.settings-field`, etc.) fragment the cascade. **Fix:** delete dead families, consolidate split blocks.
- **M6 — Hardcoded `#06131c` ×4.** `styles.css:385,393,1077,1946` hardcode the on-accent text color (one behind `!important`) while the file uses `var(--aurora-*)` 637× elsewhere. **Fix:** introduce `--aurora-on-accent` token.
- **M7 — `!important` to win specificity.** `styles.css:956`, `1077-1080` (4× on selected action-row accent), `3445` (streamdown override). The 1077-1080 cluster forces bg/border/shadow/color because base `.action-row` is over-specified. **Fix:** raise modifier selector specificity instead. (Note: `aurora.css:503-506` `!important` inside `prefers-reduced-motion` is legitimate.)

### Low
- **L1** — `useActionRunner.submit` is ~230 lines (`useActionRunner.ts:116-346`); error-run object `{ ok:false, status:0, ... }` constructed 4-5×. Extract `makeErrorRun` factory + split crawl/stream branches.
- **L2** — `App.tsx:471` comma-operator-in-ternary side effect; adjacent stale-set read. Use `if/else`.
- **L3** — `paletteView.ts:17-23` magic 30ms `setTimeout` + stringly-typed `.command-input` DOM query (also `ActionList.tsx:27`, `useWindowChrome.ts:85`). Prefer refs; name constants.
- **L4** — `OperationResultView.tsx:46-75` allowlist array + `:80-128` switch must be hand-synced. Derive one map.
- **L5** — `SettingsPanel.tsx` (486 lines) bundles 4 reusable form primitives (`TextInput`/`SecretInput`/`SelectInput`/`MiniToggle`) that belong in `ui/aurora/`.

---

## Architecture Findings

### Critical
None — contract boundary is sound, errors are surfaced, shippable.

### High
- **A-H1 — Subcommand dispatch scattered across 7+ sites, no exhaustiveness guarantee.** Adding one action requires editing: `ACTIONS` (actions.ts), `PaletteSubcommand` union, `ACTION_META` (actionMeta.ts), `bodyFor` (axonClient.ts:121), `actionRouteTemplate`, `outputKindFor` + `formatPayload` (format.ts:6,32), `hasStructuredOperationView` (OperationResultView.tsx:46 hardcoded array), the OperationResultView switch (:80), and two icon maps (OutputPanel.tsx:295, ActionIcon.tsx:36). No `default: assertNever` arm anywhere, so the union's exhaustiveness safety is unused; a forgotten action silently falls back to raw `<pre>` JSON. **Fix:** consolidate per-action behavior into a `Record<PaletteSubcommand, ActionBehavior>` registry (buildRequest/outputKind/formatText/ResultView) forcing type-level exhaustiveness. Minimum: add `assertNever` arms.
- **A-H2 — Design-system split-brain; Aurora primitives bypassed.** Only 4 of 16 palette components import `ui/aurora/*`; only 4 distinct primitives used app-wide (`button`/`scroll-area`/`spinner`/`status-indicator`); `badge`/`input`/`kbd`/`separator` essentially unused. The real design system is the 4,005-line `styles.css` of hand-rolled semantic classes. Token discipline is good (664 `var(--aurora-*)` vs ~13 hex confined to a test fixture SVG) so *theming* is centralized — but the *component abstraction* is not. Two button forms coexist: `<Button variant="aurora">` and `<button className="command-submit">`. **Fix:** pick one canonical layer — either trim the under-used `ui/aurora` primitives, or promote recurring result atoms (`ResultRows`/`ChipSection`/`DetailLine`/`Swatch`/`StatusDot`, already in OperationResultViewShared) into `ui/aurora`. Document the canonical layer in a CLAUDE.md.
- **A-H3 — Duplicate `RunState`** (= C1 above, cross-confirmed by architecture agent).

### Medium
- **A-M1 — `App.tsx` overloaded orchestrator.** 15 `useState`, ~9 derived view booleans (`:167-178`), an implicit view state machine (browse/argument-entry/results/settings/history/job-min/job-expanded) encoded across booleans rather than a discriminated `view` union; Escape handler (`:98-134`) is a 6-branch hand-coded back-stack. **Fix:** model top-level view as a discriminated union or `useReducer`; `run` orthogonal.
- **A-M2 — Setter drilling.** `useActionRunner` takes 5 setters + 4 reads; `useCrawlJob` takes 6 setters and reaches into App state (e.g. `minimizeJob` clears settings/history/browse/query/modeAction). Hooks aren't self-contained. **Fix:** pass one `dispatch` or intent callbacks (`goToBrowse()`, `minimizeJob()`); dissolves with A-M1.
- **A-M3 — OpenAPI types generated but unused.** `axon-api.d.ts` imported by nothing; `bodyFor` (axonClient.ts:107) hand-builds untyped `Record<string,unknown>` bodies; responses untyped, probed defensively (e.g. OperationResultView.tsx:188-195 tries `markdown ?? content ?? output ?? text ?? body`). Team aware (NOTE comment tracks GitHub #177). **Fix:** complete #177 — type request/response against generated `paths`/`responses`, eliminate key-probing.
- **A-M4 — Streaming fabricates `PaletteResult`.** `useActionRunner.ts:68-107` synthesizes fake `{ ok:true, status:200, ... }` to fit the one-shot `success` shape; views read by luck of fabricated values. **Fix:** give streaming terminal states their own shape or make `result` optional.
- **A-M5 — Silent `submit()` early returns.** `useActionRunner.ts:117,140,141,144` silently `return` on in-flight/local/missing-client/failed-validation. Pressing Enter with a missing token does nothing, no feedback (only the `endpointTone` dot signals). **Fix:** surface a transient `error` RunState / inline validation.

### Low
- **A-L1** — `outputTitle`/`outputSubtitle` defined twice (= M2).
- **A-L2** — Window-chrome magic numbers couple JS to CSS (`LIST_CAP` mirrors `.action-scroll` max-height) with no shared constant (= M3).
- **A-L3** — `ingestBody` source-type detection heuristic (substring match) hidden in HTTP-body builder (axonClient.ts:304-313); will accrete as sources grow.
- **A-L4** — `looksLikeUrl` (paletteView.ts:168-171) accepts bare `word.word`; combined with Enter-runs-URL-immediately (App.tsx:291) a two-word input like `cat.jpg` could trigger an unintended run.

---

## Critical Issues for Phase 2 Context

The following Phase 1 findings carry directly into the **performance** review (Phase 2B):
- **H3 (keydown re-bind every keystroke)** — render/effect performance hot path.
- **M1 / A-M1 (boolean-soup view derivation recomputed every render)** — re-render cost.
- **A-M4 / streaming reducer** (`useActionRunner.ts:62-108`) — stream-delta handling on every event; check for unbounded growth / per-delta re-render.
- **`styles.css` is 4,005 lines** — bundle-size / CSS-parse cost for a desktop palette.
- **shiki + streamdown** markdown/code rendering — known heavy; check lazy-loading & memoization of result rendering.

For **security** review (Phase 2A) — UI-relevant only:
- Untyped/defensive payload handling (A-M3) — any `dangerouslySetInnerHTML` or unsanitized markdown/HTML render paths via streamdown/shiki.
- Tauri capability surface (`capabilities/default.json`) and the invoke seam (`invoke.ts`) — what the WebView is allowed to call.
- Secret handling in `SettingsPanel` (token/secret inputs) — masking, clipboard, persistence exposure in the UI.
