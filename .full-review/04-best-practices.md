# Phase 4: Best Practices & Standards

Target: `apps/palette-tauri` — UI/UX-weighted (React 19, Tailwind v4, accessibility, TypeScript, build/release).

> Full React/Tailwind/a11y detail with code examples: [04-react-tailwind-a11y.md](04-react-tailwind-a11y.md). Summary below.

---

## Framework & Language Findings (React 19 / Tailwind v4 / a11y / TS)

**Verdict:** TypeScript discipline is excellent (strict, discriminated unions, zero `any`, type-only imports). The **accessibility implementation is the weakest dimension by far** — and for a command palette (the one UI pattern with a canonical WAI-ARIA spec) that's the headline. Tailwind v4 is installed correctly but functionally bypassed. React 19's new affordances aren't adopted where they'd directly help.

### Accessibility — HEADLINE

**Critical**
- **A11Y-C1 — Command palette has none of the combobox/listbox ARIA pattern.** Input (`PaletteCommandBar.tsx:156`) has no `role="combobox"`/`aria-expanded`/`aria-controls`/`aria-activedescendant`; `ActionList` rows (`:31-114`) are `<div>`+`<button>` with no `role="listbox"`/`role="option"`/`aria-selected`. Arrow nav moves a visual-only React index — AT hears nothing. Fails WCAG 4.1.2 + 1.3.1. The defining gap. (Section headings inside the listbox also need `role="group"`.)
- **A11Y-C2 — Streamed/async results never announced (no live region).** Output panel + streaming bodies have no `aria-live`; only the settings connection-test does (`SettingsPanel.tsx:251`). Fails WCAG 4.1.3. **Fix:** one visually-hidden polite live region driven off `run.kind` (terse status transitions, not per-token), extend `role="alert"` to `ErrorResultView`.

**High**
- **A11Y-H1 — Action-switcher `role="menu"` is pointer-only, no keyboard.** `PaletteCommandBar.tsx:107-152` advertises menu semantics but has no `onKeyDown` — can't open/arrow/Escape, no focus move/return. A keyboard-broken `role="menu"` is worse than none (WCAG 2.1.1). **Fix:** implement APG menu-button pattern OR demote to an `aria-expanded` disclosure of plain buttons (less code, honest).
- **A11Y-H2 — No focus management for overlays.** Settings/History/result panels mount without moving focus in or restoring on close; no focus trap. Settings "tabs" (`SettingsPanel.tsx:145`) are tablist-shaped buttons lacking `role="tablist"`/`tab`/`tabpanel`. (WCAG 2.4.3.) **Fix:** `useFocusReturn(open)` hook across the three overlays.

**Medium**
- **A11Y-M1 — `focus-visible` inconsistent across the two layers.** `ui/aurora/*` primitives have proper rings; hand-written controls (`.command-input`, `.command-submit`, `.action-row-main`, etc.) need a per-class focus-visible audit (WCAG 2.4.7/2.4.11). Not centralizable due to the split-brain (A-H2).
- **A11Y-M2 — State conveyed by color/shape only.** Status dot (`PaletteCommandBar.tsx:102`) + disabled-submit feedback are color/`title`-only, invisible to AT (ties to A-M5). **Fix:** sr-only label on the dot; surface validation via `aria-describedby` text not just `title`.
- **A11Y-M3 — Aurora token contrast unverified.** Small muted text (`.action-description`/`.action-method`/placeholder) on control surfaces are the WCAG 1.4.3 risks; can't compute from source. **Fix:** axe/Lighthouse contrast pass in both `.dark`/`.light`.

**Good (keep):** `prefers-reduced-motion` honored (`aurora.css:500`); descriptive `alt` on the one `<img>`; decorative lucide icons `aria-hidden`, icon-only buttons have `aria-label`; native `<select>` + `aria-pressed` toggle + stateful secret-reveal `aria-label` in Settings; native `<details>`/`<summary>` for "More actions".

### React 19 Idioms
- **R-H1 (High) — Manual async state machine where `useActionState`/Actions fit.** `useActionRunner.submit` (~230 lines) hand-rolls pending→success/error + duplicated error-run object + silent early-returns (= A-M5). One-shot actions (`stats`/`status`/`sources`/`doctor`) are a clean `useActionState` fit; **keep streaming/job paths imperative** (Actions don't model streams/pollers).
- **R-M1 (Med) — keydown effect re-binds every keystroke/delta** (= H3/P-H2). Ref-for-latest-value pattern; mirror the already-correct `blur` listener.
- **R-M2 (Med) — `forwardRef` unnecessary in React 19** across all 7 `ui/aurora` primitives — ref-as-prop is idiomatic; `input.tsx` `useImperativeHandle` ref-merge simplifies too.
- **R-M3 (Med) — Effects that should be derived/event-driven.** `App.tsx:180-182` reset-selection-in-effect (you-might-not-need-an-effect); causes double render. Derive clamped selection in render.
- **Good:** discriminated `RunState` union; `useMemo` used where it pays, not sprayed.

### Tailwind v4 Idioms
- **TW-H1 (High) — v4 installed correctly but functionally bypassed; no `@theme` bridge.** Setup is exemplary (`@import "tailwindcss"`, `@tailwindcss/vite`, smart `@source` for streamdown) but **zero `@theme`/`@apply`/`@layer`/`@utility`**. The ~80 Aurora tokens are plain CSS vars invisible to Tailwind → verbose `bg-[var(--aurora-…)]` everywhere, alongside the 4,005-line hand-written `styles.css` which is the real design system. **Fix (incremental):** promote tokens into `@theme` so `text-text-primary`/`ring-accent-primary` work; then pick the canonical layer (= A-H2).
- **TW-M1 (Med) — CVA + tailwind-merge idiomatic but inline-`style`/`VARIANT_CONFIG` split** (`button.tsx:70-166`) is outside `tailwind-merge`'s reach → `className` overrides don't reliably win over inline `style`. Same "inline to win" smell in `input.tsx:240`. Move box-shadow/gradient into theme vars once `@theme` lands.
- **TW-L1 (Low) — `input.tsx:218-220` runtime-interpolated arbitrary class** (`border-[${tokens.border}]`) never compiles (v4 static extraction) — inert, saved only by a parallel inline `style.borderColor`. Misunderstanding of v4's build model.

### TypeScript (strongest dimension)
- **Good:** strict + modern tsconfig (`strict`, `noUnusedLocals/Parameters`, `noFallthroughCasesInSwitch`, ES2022, bundler resolution); zero `any`; type-only imports; discriminated unions.
- **TS-M1 (Med) — Untyped server payloads + defensive key-probing** (= A-M3). `axon-api.d.ts` imported by nothing; `bodyFor` builds `Record<string,unknown>`; views probe `markdown ?? content ?? ...`. Strict mode undermined at the I/O boundary. Complete GitHub #177.

### Deprecated / Legacy
- **DEP-1 — `forwardRef`** across `ui/aurora` (React 19 deprecation runway; = R-M2).
- **DEP-2 — `lucide-react ^1.16.0` is CURRENT, not old — prior-phase concern RETRACTED.** Verified lockfile: valid release, `react@19.2.6` peer. Non-finding.
- **DEP-3 — `"use client"` in `input.tsx:1`** is a no-op Next.js/RSC copy-paste artifact. Remove for honesty.
- No deprecated React APIs (`createRoot` used, no legacy context/string refs).

### Build Config
- **Good:** `vite.config.ts` clean — `@tailwindcss/vite` + react plugin, `@`-alias, dev proxy injects bearer token server-side (never bundled), Tauri-aware.
- **BUILD-H1 — No code-splitting / `manualChunks`** (= P-H1). shiki+streamdown in main chunk, highlighter at module-eval on startup path; no `React.lazy` anywhere. Largest perceived-perf win.

---

## CI/CD & DevOps Findings (UI-shipping-relevant)

**Verdict:** Release/version-parity machinery is genuinely strong (better than most single-app setups). Gaps are on the quality-gate side: no JS/TS linter, palette Rust skips clippy/fmt, CI never builds the actual Tauri binary, no OpenAPI drift check, no bundle splitting.

### High
- **CI-H1 — CI never builds the Tauri binary; first real desktop build is at release time.** `ci.yml` palette job (`:196-205`) runs test→typecheck→`vite:build`→`cargo check`→`cargo test`, never `tauri build`. Full build happens only in `palette-release.yml` *after* the `palette-v*` tag is cut → a build failure leaves a live tag on the public release path. **Fix:** add `tauri build --no-bundle --ci` (Linux min, ideally Windows leg) to CI before tagging. Highest-value gap.
- **CI-H2 — No JS/TS linter anywhere.** No ESLint/Biome/Prettier, no `lint` script; only `tsc --noEmit`. This is *why* P-H2 (keydown dep-array) and the a11y gaps shipped — `eslint-plugin-react-hooks` + `jsx-a11y` flag exactly those. **Fix:** add Biome (lowest friction) or ESLint flat config with `react-hooks` + `jsx-a11y`; wire into `verify` + CI.
- **CI-H3 — Palette Rust excluded from clippy/fmt.** `src-tauri/Cargo.toml:10` declares its own `[workspace]`, so the repo's root `cargo fmt --all`/`clippy --workspace` never touch the IPC/network bridge; palette job runs only `cargo check`+`cargo test`. Zero lint on the security-relevant bridge. **Fix:** add `clippy`/`fmt --check` steps with `--manifest-path apps/palette-tauri/src-tauri/Cargo.toml`.

### Medium
- **CI-M1 — No OpenAPI type-drift check.** `axon-api.d.ts` is generated + committed; nothing regenerates+diffs in CI. Spec can change while palette types go stale → typecheck green, runtime wrong shape. **Fix:** `pnpm generate:api && git diff --exit-code src/lib/axon-api.d.ts`.
- **CI-M2 — No bundle code-splitting** (build-side half of P-H1). `vite.config.ts` has no `build` block. **Fix:** add `manualChunks` for shiki/streamdown when P-H1 lands; set `build.target` to the WebView floor.
- **CI-M3 — No coverage gate** (carried from Phase 3). **Fix:** add `test.setupFiles` + a modest `coverage.thresholds` floor on the already-good lib layer.

### Low
- **CI-L1 — `verify` is sound but CI open-codes it** (`ci.yml:194-205`) → drift risk. Have CI call `pnpm verify` + cargo steps, or comment both to stay in sync.
- **CI-L2 — Inconsistent pinning** (shiki exact, rest caret) — Low; frozen lockfile + `.npmrc` make installs reproducible regardless.
- **CI-L3 — CSP shared dev/prod; `style-src 'unsafe-inline'` permanent** — justified & documented (Tailwind v4 inline styles); revisit at Vite CSP-nonce stabilization. No action.

### Done well (don't regress)
- Version parity enforced twice (`palette-release.yml:59-66` three-file check + `:70-76` tag match).
- Robust release gating (`auto-tag.yml` waits for green CI, monotonic bump, fails on code-without-bump, `concurrency` serialization, `make_latest: false`).
- Reproducible installs (frozen lockfile, pinned pnpm/Node/Rust, SHA256 checksums).
- Self-hosted fonts (CSP-clean), portable-binary RUSTFLAGS on Windows.

### Priority order
1. CI-H1 (tauri build in CI) → 2. CI-H3 (palette clippy/fmt) → 3. CI-H2 (linter + jsx-a11y/react-hooks) → 4. CI-M1 (OpenAPI drift) → 5. CI-M2/M3 (splitting + coverage floor).
