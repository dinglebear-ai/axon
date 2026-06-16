# Phase 3: Testing & Documentation Review

Target: `apps/palette-tauri` — UI/UX lens.

---

## Test Coverage Findings

**Verdict: inverted pyramid for a UI app.** The lib/hook layer is genuinely well-tested (9 suites, ~1,075 lines — `axonClient`, `crawlJob`, `paletteView`, `configModel`, `historyRun`, `format`, `actionHelp`, `useActionRunner` reducer, `useWindowChrome`). But the interactive component layer is thin (~242 lines across 3 real render suites), **14 of 17 palette components have zero render coverage**, and there are **zero accessibility assertions anywhere**. Two devDeps block most fixes: `@testing-library/user-event` and `jest-axe`. No `setupFiles` in `vite.config.ts` (jsdom opted-in per-file; `matchMedia`/`scrollIntoView` hand-stubbed per suite). No coverage gate.

### Critical
- **T-C1 — No a11y coverage on a keyboard-first command palette, AND the ARIA isn't implemented.** No `jest-axe`, no role/aria assertions. Worse, inspection shows the combobox/listbox pattern is missing: command `<input>` (`PaletteCommandBar.tsx:156`) has only `aria-label` — no `role="combobox"`, `aria-expanded`, `aria-controls`, `aria-activedescendant`. `ActionList.tsx:54` rows are `<div>` wrapping `<button>` — no `role="listbox"`/`role="option"`/`aria-selected`. Screen-reader users hear nothing change while arrowing. **Fix:** add `jest-axe` smoke pass + explicit combobox-pattern suite (will fail today → drives the implementation fix).

### High
- **T-H1 — Keyboard nav barely exercised, not with real key semantics.** `App.onInputKeyDown` (`App.tsx:278-303`) implements Arrow/Enter/Tab branching (enter-mode-vs-submit decision tree, selection clamping, Tab-runs-immediately); only the Enter→local-help path is tested (`App.test.tsx:62-78`). Global Escape (`:101`) untested. All tests use `fireEvent.keyDown` (no focus/preventDefault semantics). **Fix:** add `@testing-library/user-event`, drive real keyboard for Arrow/Tab/Escape + the Enter decision tree.
- **T-H2 — Streaming UI has no render-level start/delta/done/cancel test.** Reducer is unit-tested but the rendered experience (`OutputPanel.tsx:159-264` streaming branch, `AskConversation` → `Streamdown`) is never rendered. No "Stop" affordance exists for ask/chat streams (only crawl `cancelJob`) — test or document the gap. Couples to perf P-M1: `firstUrl(run.text)`/`readingHeaderSummary` O(n²) over the buffer has no guard. **Fix:** drive synthetic stream events through mocked `appWindow.listen`, assert transitions + render-count.
- **T-H3 — `SettingsPanel`/`AskConversation` "tests" are not behavioral.** `SettingsPanel.test.tsx` asserts `typeof === "function"`, `.name`, and builds props it never passes to React (`:44-49`) — never renders. `AskConversation.test.ts` only greps `styles.css`. These pass while the panel is fully broken. **Fix:** real render + interaction (type server URL → assert `onChange`, click Save → assert `onSave`).

### Medium
- **T-M1 — Error/empty states untested; `ErrorResultView` has no coverage and no `role="alert"`.** Failed operation announced to no one. Untested: 401/500 render, empty query/sources/domains, `run.kind === "error"` branch. **Fix:** render with error payload, assert message + add `role="alert"`.
- **T-M2 — Structured result renderers untested.** `OperationResultView.test.tsx` tests routing well but never renders `CrawlJobView`/`EvaluateView`/`StatsView`/`StatusView`/`HelpResultView`/`OperationResultViewShared` (incl. the `imagePreviewSrc`/link code from S-M1/S-L2/S-M2). **Fix:** use `OperationResultFixture.tsx` as shared fixture source, render each with representative payload.
- **T-M3 — No streamdown sanitization regression test.** Nothing pins the XSS containment (S-M1/S-L1). A future `rehypePlugins` override (replaces the array) could silently re-enable scripts/tracking images. **Fix:** render a `<script>`/`javascript:`/`onerror` payload through `Streamdown`, assert stripped.
- **T-M4 — `PaletteCommandBar` action-switcher menu untested.** Real `role="menu"` popup (`:107-152`); open/close, `aria-expanded`, `onSwitchAction`, click-outside dismissal untested; in-menu keyboard (arrow/Escape) appears unimplemented. **Fix:** render in `modeAction` state, assert menu semantics.

### Low
- **T-L1 — Timing behaviors on real timers** (30ms focus, 1200ms `copied`, 1Hz crawl poll) — flaky/slow when tested. Use `vi.useFakeTimers()`.
- **T-L2 — Harness duplication, no shared setup.** `matchMedia`/`scrollIntoView` hand-stubbed per suite; no `setupFiles`, no `renderApp()` factory. **Fix:** add `test.setupFiles` (jest-dom + polyfills + shared invoke/listen mock + `renderApp()`).
- **T-L3 — `fireEvent` used where `userEvent` is correct** — lower fidelity; migrate interaction suites.

---

## Documentation Findings

**Verdict: existing docs are high quality; the deficit is the absence of an architecture/convention doc.** The README is strong (accurate, current, explains the browser-vs-Tauri runtime model, IPC networking, CSP rationale, frozen lockfile). Inline comments on hard logic are model-quality (`invoke.ts` seam, `useWindowChrome.ts` sizing rationale, streaming guards) — hold these up as the bar. **No CLAUDE.md exists anywhere in or above the app.**

### Critical
None — nothing documented is dangerously wrong; the security/runtime model that is documented (CSP, IPC, frozen lockfile) is accurate.

### High — *all three collapse into one deliverable: a new `apps/palette-tauri/CLAUDE.md`*
- **D-H1 — No CLAUDE.md/contributor guide.** `find` returns nothing; repo-root CLAUDE.md mentions the palette only in release tables. A new UI contributor has no durable doc for the data-flow model, the deliberate `App.tsx` orchestrator pattern, the co-located test convention, design-system layering, action-registry process, or the fixture harness. **Fix:** add `apps/palette-tauri/CLAUDE.md` (+ `AGENTS.md`/`GEMINI.md` symlinks per repo source-of-truth rule).
- **D-H2 — No documented canonical design-system layer.** (= A-H2.) Two button forms coexist; no comment/doc states which layer is canonical. Contributor can't tell whether to use an Aurora primitive or a new `.foo` class. Note positively: token discipline (`var(--aurora-*)` 600+×) is already centralized — only the component-abstraction story is undocumented. **Fix:** declare canonical layer + one-line decision rule.
- **D-H3 — "How to add an action" undocumented despite 7+ files.** (= A-H1.) Most error-prone task, entirely tribal; no `assertNever` so a forgotten site silently degrades to raw `<pre>`. **Fix:** document the ordered edit-site checklist; replace with "add one `ActionBehavior` entry" after the A-H1 registry refactor.

### Medium
- **D-M1 — `LIST_CAP`↔`.action-scroll` invariant documented JS-side only.** `useWindowChrome.ts:92` comment is exemplary (praise it); `styles.css:2171-2172` `max-height: min(338px,…)` has no back-reference. **Fix:** add reciprocal comment at `styles.css:2171`, document invariant in CLAUDE.md.
- **D-M2 — Fixture harness (`fixture:operation-results`) undocumented.** Primary tool for iterating result-view UI with no backend (`main.tsx:9-13` → `OperationResultFixture`); named nowhere in README, no header comment. **Fix:** document in README dev workflow + add file header explaining how to add a case.
- **D-M3 — OpenAPI codegen noted but contract gap unstated.** README Note is commendably honest (tracks #177); doesn't state the generated `axon-api.d.ts` is imported by nothing and responses are key-probed. **Fix:** one sentence converting the silent footgun to documented known-state.
- **D-M4 — Repo test-convention doc is Rust-only.** Root CLAUDE.md mandates `_tests.rs` sidecar; the TS frontend uses co-located `*.test.ts(x)`. **Fix:** state the frontend convention in app CLAUDE.md.

### Low
- **D-L1** — `pnpm dev` failure mode / no-backend alternative not described; cross-reference fixture mode.
- **D-L2** — `pnpm vite:dev` (browser dev entry, the reason the invoke seam exists) absent from README Commands.
- **D-L3** — `@aurora` registry install path (`components.json` → `aurora.tootie.tv/r/`) undocumented.

### Strengths (keep doing)
- `src/lib/invoke.ts` header comment — exemplary explanation of the dev-vs-Tauri seam and why it exists.
- `useWindowChrome.ts` — model-quality sizing rationale with failure-mode-per-decision.
- README runtime/security sections — accurate networking model, CSP rationale w/ migration path, frozen-lockfile explanation.
- Accuracy — every version/script/path checked is correct; palette 5.9.1 consistent across `package.json`↔`tauri.conf.json` and correctly independent from CLI 5.15.0.
