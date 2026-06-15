# Comprehensive Code Review Report — Axon Palette (Tauri Desktop) UI/UX

## Review Target

`apps/palette-tauri` — **axon-palette-tauri v5.9.1**, a Raycast/Spotlight-style command-palette desktop GUI for the Axon RAG engine. Stack: React 19.2, Tauri 2.10, Tailwind CSS v4, Aurora design system, shiki + streamdown, TypeScript 5.9, Vitest. Review scoped specifically to **UI/UX**: layout, interaction design, accessibility, visual consistency, keyboard/focus handling, theming, and frontend code quality.

## Executive Summary

This is a **well-engineered, well-tested frontend, clearly above average for a Tauri side-app** — real unidirectional data flow, a clean Tauri/browser invoke seam, strong Aurora *token* discipline, excellent inline comments on hard logic, a genuinely hardened IPC bridge, and a strict TypeScript setup. It is shippable today and has **no Critical security or stability defects**.

The work needed is concentrated in three themes: **(1) accessibility** — for a keyboard-first command palette, the WAI-ARIA combobox/listbox pattern is entirely unimplemented and streamed results are never announced (the single highest-impact UX gap); **(2) consolidation** — a dead/divergent `RunState` type, ~10 copy-pasted helpers, scattered 7-file action dispatch, and a "split-brain" design system where 4,005 lines of hand-written CSS coexist with bypassed Aurora primitives; **(3) guardrails** — no linter, CI never builds the actual Tauri binary, and the interactive UI layer + a11y have effectively no test coverage, which is *why* several of these gaps shipped.

---

## Findings by Priority

### Critical Issues (P0 — Must Fix Immediately)

- **[A11Y-C1] Command palette has none of the combobox/listbox ARIA pattern** *(Phase 4)* — input lacks `role="combobox"`/`aria-expanded`/`aria-activedescendant`; list rows lack `role="listbox"`/`role="option"`/`aria-selected`. Selection is visual-only; screen-reader users hear nothing. The defining accessibility gap for this app class. `PaletteCommandBar.tsx:156`, `ActionList.tsx:31-114`.
- **[A11Y-C2] Streamed/async results are never announced** *(Phase 4)* — no `aria-live` on output/streaming bodies; non-sighted users get silence on every result. Fails WCAG 4.1.3. `OutputPanel.tsx:79`, `AskConversation`.
- **[C1] Dead, divergent `RunState` type export** *(Phase 1, confirmed by 2 agents)* — `paletteView.ts:12-15` exports an incompatible union (phantom `queued`, missing `streaming`/`job`) vs canonical `runState.ts:5-44`. A type-safety landmine: `import { RunState } from "@/lib/paletteView"` compiles but is wrong. Mechanical fix (delete + re-export).

> Note: A11Y-C1/C2 are "Critical" on the accessibility axis (the review's explicit focus), not stability. C1 is a latent correctness/type-safety landmine. None block shipping today, but all three are high-impact and low-cost.

### High Priority (P1 — Fix Before Next Release)

**Architecture / consolidation**
- **[A-H1] Subcommand dispatch scattered across 7+ files with no exhaustiveness check** *(Phase 1)* — adding one action touches ~9 sites; a forgotten one silently degrades to raw `<pre>`. No `assertNever`. Consolidate into a `Record<PaletteSubcommand, ActionBehavior>` registry. Top extensibility risk.
- **[A-H2 / TW-H1 / D-H2] Design-system split-brain** *(Phases 1, 3, 4)* — only 4/16 palette components use `ui/aurora/*`; the real design system is the 4,005-line `styles.css`; Tailwind v4 is installed but bypassed (zero `@theme`/`@apply`). Tokens are disciplined; the *component abstraction* and *canonical layer* are not. Decide + document one layer; add a `@theme` bridge.
- **[H1/H2] ~10 duplicated helpers, some user-visibly divergent** *(Phase 1)* — `hostLabel` (×4, `.host` vs `.hostname`), `shortId` (two thresholds/glyphs → same ID truncates differently in two panels), `titleCase`, `firstUrl`, `firstArray`, `isRecord` (×5). `lib/url.ts` is entirely dead. Consolidate into `lib/payload.ts`/`lib/url.ts`.

**Performance**
- **[P-H1 / BUILD-H1] shiki/streamdown eagerly bundled, blocking cold-start** *(Phases 2, 4)* — highlighter instantiated at module-eval on the startup path; no `React.lazy`/`manualChunks`. Largest perceived-perf win for a launcher. `limitedStreamdownCode.ts:32-36`, `vite.config.ts`.
- **[H3 / P-H2 / R-M1] Global keydown listener re-binds every keystroke + stream delta** *(Phases 1, 2, 4)* — `App.tsx:98-134` deps include `query`/`run`. Ref-for-latest-value; bind once.

**Accessibility**
- **[A11Y-H1] Action-switcher `role="menu"` is keyboard-inoperable** *(Phase 4)* — advertises menu semantics with no `onKeyDown`. Implement APG menu-button or demote to a disclosure.
- **[A11Y-H2] No focus management for overlays** *(Phase 4)* — settings/history/result panels don't move/restore focus or trap; settings "tabs" lack tablist semantics.

**React idiom**
- **[R-H1] Manual ~230-line async state machine where `useActionState` fits** *(Phase 4)* — one-shot actions hand-roll pending/error + silent early-returns (= A-M5). Migrate one-shot paths; keep streaming/job imperative.

**Testing / CI**
- **[T-C1] No a11y test coverage** *(Phase 3)* — no jest-axe/role assertions; would also surface A11Y-C1. **[T-H1/H2/H3]** keyboard nav barely tested, streaming UI untested, `SettingsPanel`/`AskConversation` "tests" are non-behavioral (`typeof`/CSS-grep).
- **[CI-H1] CI never builds the Tauri binary** *(Phase 4)* — first real build is at release, after the tag is cut. **[CI-H2]** no JS/TS linter (why P-H2 + a11y gaps shipped). **[CI-H3]** palette Rust bridge skips clippy/fmt.

**Documentation**
- **[D-H1/H2/H3] No CLAUDE.md/contributor guide** *(Phase 3)* — no durable doc for data-flow, canonical design layer, or the 7-file "add an action" process. These collapse into one deliverable: `apps/palette-tauri/CLAUDE.md`.

### Medium Priority (P2 — Plan for Next Sprint)

- **[A-M1/M1] `App.tsx` view-flag boolean soup** — ~10 interdependent booleans + implicit view state machine; model as a discriminated `view` union / `useReducer`. **[A-M2]** dissolves the 11-setter drilling.
- **[A-M3 / TS-M1] OpenAPI types generated but unused** — untyped `Record<string,unknown>` bodies + defensive `markdown ?? content ?? ...` probing. Complete GitHub #177. **[CI-M1]** add a drift check.
- **[S-M1] Untrusted external images render with no prefix allowlist** *(Phase 2)* — image-beacon tracking, currently contained only by CSP. **[S-M2]** crawled-content links clickable with no domain allowlist (phishing). Both fixed by a shared hardened `rehypePlugins` set (`allowedImagePrefixes`/`allowedLinkPrefixes`).
- **[P-M1+P-M2] No `React.memo`/`useCallback`** — `firstUrl(run.text)` is O(n²) over the streaming buffer (`OutputPanel.tsx:72`); memoize derived values + result views (ship as a pair). **[T-H2]** add a render-count guard.
- **[M5/M6/M7] Dead & duplicated CSS, hardcoded `#06131c`×4, `!important` specificity fights.**
- **[A11Y-M1/M2/M3]** focus-visible audit across hand-written controls; color-only status signals; unverified token contrast (run axe/Lighthouse).
- **[T-M1/M2/M3]** error/empty states untested (`ErrorResultView` has no `role="alert"`); structured result renderers unrendered in tests; no streamdown-sanitization regression test.
- **[D-M1/M2/M3/M4]** `LIST_CAP`↔CSS invariant documented one-side-only; fixture harness undocumented; OpenAPI not-wired-in state unstated; TS test convention undocumented.
- **[R-M2/M3] `forwardRef` legacy in React 19**; reset-selection-in-effect anti-pattern. **[CI-M2/M3]** code-splitting + coverage floor.

### Low Priority (P3 — Track in Backlog)

- **[L1]** `useActionRunner.submit` ~230 lines; extract error-run factory. **[L2]** comma-operator-in-ternary (`App.tsx:471`). **[L3]** magic 30ms `setTimeout` + stringly-typed `.command-input` DOM query. **[L4]** allowlist-array vs switch hand-sync. **[L5]** `SettingsPanel` bundles 4 reusable form primitives.
- **[S-L1/L2/L3/I1]** Streamdown `harden` permissive default; dead `file://` branch in `imagePreviewSrc`; secret-file umask window (`OpenOptions::mode`); token input `autoComplete="off"`.
- **[P-M3]** `copied` flash re-renders whole tree; font preload for Manrope; verify Noto Sans usage.
- **[TW-L1]** `input.tsx` runtime-interpolated arbitrary class never compiles (inert). **[DEP-3]** stray `"use client"` in `input.tsx`.
- **[T-L1/L2/L3]** fake-timer the timing behaviors; shared setup file + `renderApp()` factory; migrate `fireEvent`→`userEvent`.
- **[D-L1/L2/L3]** document `pnpm dev` failure mode, `pnpm vite:dev`, `@aurora` registry install path. **[CI-L1/L2/L3]** CI call `verify`; pinning; CSP dev/prod note.

---

## Findings by Category

- **Code Quality**: 13 findings (1 Critical, 3 High, 7 Medium, 5 Low)
- **Architecture**: 12 findings (0 Critical, 3 High, 5 Medium, 4 Low)
- **Security**: 6 findings (0 Critical, 0 High, 2 Medium, 4 Low/Info) — *well-sandboxed*
- **Performance**: 5 findings (0 Critical, 2 High, 3 Medium)
- **Testing**: 10 findings (1 Critical, 3 High, 4 Medium, 3 Low) — *biggest gap: a11y + interactive layer*
- **Documentation**: 10 findings (0 Critical, 3 High, 4 Medium, 3 Low) — *one CLAUDE.md closes the High tier*
- **React/Tailwind/a11y best practices**: a11y (2 Crit / 2 High / 3 Med), React (1 High / 3 Med), Tailwind (1 High / 1 Med / 1 Low), TS (1 Med)
- **CI/CD & DevOps**: 9 findings (0 Critical, 3 High, 3 Medium, 3 Low) — *release machinery strong; quality gates weak*

*(Counts include cross-phase confirmations of the same root issue, e.g. C1=A-H3=TS-L1, A-H2=TW-H1=D-H2, P-H1=BUILD-H1, H3=P-H2=R-M1.)*

---

## Recommended Action Plan

**Sprint 1 — Accessibility + cheap mechanical wins (highest UX leverage):**
1. **A11Y-C1 + A11Y-C2** — combobox/listbox ARIA on input+list (keep the roving index, expose via `aria-activedescendant`) and a single polite live region for run-state transitions. *(Medium effort; the defining UX fix.)*
2. **C1** — delete the dead `RunState`; **H1/H2** — consolidate the ~10 duplicated helpers, delete dead `lib/url.ts`; **M5** — strip dead CSS. *(Small, low-risk, removes the most footguns.)*
3. **A11Y-H1/H2** — make the switcher keyboard-operable (or demote) + a `useFocusReturn` hook for overlays. *(Small–medium.)*

**Sprint 2 — Performance + guardrails:**
4. **P-H1/BUILD-H1** — lazy-load + `manualChunks` shiki/streamdown (cold-start win); **P-H2** — ref-based keydown listener. *(Small–medium.)*
5. **CI-H1** (tauri build in CI), **CI-H3** (palette clippy/fmt), **CI-H2** (Biome/ESLint + `jsx-a11y`+`react-hooks` — would have caught these gaps automatically). *(Small each; high prevention value.)*
6. **T-C1/T-H1/T-H2/T-H3** — jest-axe + `@testing-library/user-event`; real keyboard, streaming, and settings tests. *(Medium.)*

**Sprint 3 — Structural + hardening:**
7. **A-H1** — action-behavior registry (unblocks exhaustiveness + dissolves A-M3 probing). **A-M1/A-M2** — view-state union/reducer. *(Large; highest future-velocity payoff.)*
8. **A-H2/TW-H1** — resolve the design-system split-brain + `@theme` bridge; **D-H1/H2/H3** — write `apps/palette-tauri/CLAUDE.md` (closes 3 High docs findings at once). *(Medium; mostly decision + documentation.)*
9. **R-H1** — migrate one-shot actions to `useActionState` (fixes A-M5). **S-M1/S-M2** — shared hardened `rehypePlugins`. **A-M3/#177** — wire OpenAPI types. *(Medium.)*

**Effort key:** Small = <½ day · Medium = 1–3 days · Large = multi-day refactor.

---

## Review Metadata

- Review date: 2026-06-15
- Phases completed: 1 (Quality & Architecture), 2 (Security & Performance), 3 (Testing & Documentation), 4 (Best Practices & Standards), 5 (Consolidated Report)
- Agents: 10 specialized reviewers across 5 phases (parallel within each phase)
- Flags applied: none (`--strict-mode`/`--security-focus`/`--performance-critical` not set); framework auto-detected: React 19 + Tauri 2 + Tailwind v4 + Aurora
- Scope note: UI/UX-weighted per request; backend/Rust de-emphasized except the IPC bridge & persistence as they affect the UI
- Supporting files: `00-scope.md`, `01-quality-architecture.md`, `02-security-performance.md`, `03-testing-documentation.md`, `04-best-practices.md`, `04-react-tailwind-a11y.md`
