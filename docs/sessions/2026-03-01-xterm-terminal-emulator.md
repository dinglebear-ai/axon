# Session: xterm.js Terminal Emulator at /terminal

**Date:** 2026-03-01
**Branch:** `feat/crawl-download-pack`
**Commits:** `ac16331b`, `817ef92c`

---

## Session Overview

Implemented a full-featured xterm.js terminal emulator in the axon web app. The terminal renders at `/terminal`, integrates with the existing axon WebSocket protocol (`ws-protocol.ts` / `use-axon-ws.ts`) for command execution, matches the axon dark neural-tech design system, and is reachable via a new "Terminal" nav link in the `PulseSidebar`. Also fixed a pre-existing TypeScript error (`TS2339`) and Biome lint issue in `app/api/logs/route.ts`.

---

## Timeline

1. **Plan received** — 3-unit implementation plan: xterm.js core component, session hook + history, page + navigation
2. **Codebase read** — `ws-protocol.ts`, `use-axon-ws.ts`, `pulse-sidebar.tsx`, `globals.css`, `logs-viewer.tsx`, `package.json`, `app-shell.tsx`, `logs/page.tsx`
3. **Unit 1** — Subagent: added xterm packages, created `terminal-emulator.tsx` + `terminal-emulator-wrapper.tsx`
4. **Unit 2** — Subagent: created `lib/terminal-history.ts` + `hooks/use-terminal-session.ts`
5. **Unit 3** — Subagent: created `components/terminal/terminal-toolbar.tsx`, `app/terminal/page.tsx`, updated `pulse-sidebar.tsx`
6. **Spec compliance review** — Reviewer found: stale `onData` closure (critical), duplicate Ctrl+K handler (important)
7. **Bug fixes** — Applied `onDataRef` pattern to `terminal-emulator.tsx`, removed duplicate Ctrl+K from window keydown
8. **Code quality review** — Reviewer found: Up/Down arrows lacked `isRunning` guard, wrapper missing unmount guard on dynamic import promise
9. **Further fixes** — Added `isRunning` guard to arrow handlers, added `active` flag to wrapper's import promise
10. **TypeScript clean** — `pnpm exec tsc --noEmit` → 0 errors (after installing packages)
11. **Pre-commit hooks** — Multiple Biome lint iterations: template literals, unused imports, `aria-label` on wrong element, `noControlCharactersInRegex` suppression
12. **Push** — `ac16331b` feat commit + `817ef92c` changelog SHA update

---

## Key Findings

- `terminal-emulator.tsx:182` — `terminal.onData(onData)` inside empty-dep `useEffect` captures initial prop forever. xterm stores the callback reference at registration time. Fix: `onDataRef.current = onData` on every render + stable wrapper `(data) => onDataRef.current(data)`.
- `page.tsx:121-139` — Up/Down arrow history navigation had no `isRunning` guard. During command execution, `clearCurrentLine()` would clobber live output lines and corrupt `inputRef`. Fix: `if (session.isRunning) return` added to both branches.
- `terminal-emulator-wrapper.tsx:24` — Dynamic import promise had no unmount guard. Post-unmount `setTermComp` call would log a React warning. Fix: `active` flag set to `false` in cleanup.
- `app/api/logs/route.ts:6` — Pre-existing `eslint-disable-next-line no-control-regex` comment does not suppress Biome's `noControlCharactersInRegex`. Replaced with `// biome-ignore lint/suspicious/noControlCharactersInRegex: intentional ANSI escape sequence stripping`.
- `app/api/logs/route.ts:56` — `container.logs()` return type resolved to Web API `ReadableStream` which lacks `destroy()`. Fix: cast to `import('node:stream').Readable`.
- xterm.js packages are browser-only — all `await import()` calls must be inside `useEffect`, never at module top level.

---

## Technical Decisions

- **Manual hydration guard over `next/dynamic`** — `dynamic()` doesn't cleanly forward refs through `forwardRef` components in Next.js 16. Used `useEffect` + `useState<ComponentType>` to import xterm client-side and pass `ref` as a normal prop.
- **`onDataRef` pattern** — Instead of adding `onData` to the `useEffect` dep array (which would remount the terminal and destroy scroll history on every render), store the latest callback in a ref updated synchronously each render. Stable wrapper registered once with xterm always calls the current callback.
- **`TerminalHistory` as plain class** — No React. Instantiated via `useRef` in the hook so it's stable across renders. localStorage access guarded with `typeof window !== 'undefined'` for SSR safety.
- **Refs for `isRunning` state in `use-terminal-session`** — State (`isRunning`, `currentExecId`) mirrored in refs to avoid stale closures inside the stable WS subscription callback.
- **Subagent-driven development** — 3 sequential implementation subagents + spec compliance reviewer + code quality reviewer, each with full context extracted upfront.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `apps/web/package.json` | Modified | Added `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links`, `@xterm/addon-search` |
| `apps/web/pnpm-lock.yaml` | Modified | Lockfile updated for new xterm packages |
| `apps/web/components/terminal/terminal-emulator.tsx` | Created | Core xterm.js component (forwardRef, TerminalHandle, axon theme) |
| `apps/web/components/terminal/terminal-emulator-wrapper.tsx` | Created | SSR-safe wrapper with manual hydration guard + unmount safety |
| `apps/web/components/terminal/terminal-toolbar.tsx` | Created | WS status dot, clear/copy/cancel/search-toggle toolbar |
| `apps/web/lib/terminal-history.ts` | Created | localStorage command history (max 500, prev/next cursor) |
| `apps/web/hooks/use-terminal-session.ts` | Created | WS subscription hook (output/done/error/start messages) |
| `apps/web/app/terminal/page.tsx` | Created | Full terminal page with readline loop, welcome banner, search overlay |
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Modified | Added Terminal nav link (TerminalSquare icon → /terminal) |
| `apps/web/app/api/logs/route.ts` | Modified | Fixed TS2339 (`Readable` cast) + Biome suppression comment |
| `docker/web/cont-init.d/12-docker-socket-group` | Created | Docker socket group init script (pre-existing untracked file, now committed) |
| `CHANGELOG.md` | Modified | Added 4 new entries (3 undocumented + terminal emulator commit) |

---

## Commands Executed

```bash
# TypeScript check (after pnpm install)
pnpm exec tsc --noEmit          # → 0 errors

# Package install (triggered by package.json update)
pnpm install                     # → Done in 1.7s

# Biome auto-fix
pnpm biome check --write --unsafe app/terminal/page.tsx \
  components/terminal/terminal-emulator-wrapper.tsx \
  hooks/use-terminal-session.ts lib/terminal-history.ts
# → Fixed 3 files

pnpm biome check --write --unsafe components/terminal/
# → Fixed terminal-emulator.tsx (dep array update)

# Final checks
pnpm biome check app/api/logs/route.ts   # → 0 errors
pnpm biome check components/terminal/    # → 0 errors

# Commit + push
git commit -m "feat(web): xterm.js terminal emulator at /terminal"
# → ac16331b, 19 files changed, 1843 insertions(+), 270 deletions(-)
git push  # → feat/crawl-download-pack -> feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `/terminal` route | 404 | Full xterm.js terminal with readline, WS execution, history |
| Sidebar nav | 9 links (Files…Logs) | 10 links (+ Terminal with TerminalSquare icon) |
| `logs/route.ts` TypeScript | `TS2339: destroy does not exist on ReadableStream` | Clean compile |
| `logs/route.ts` Biome | `noControlCharactersInRegex` error blocked commits | Suppressed with correct `biome-ignore` comment |
| xterm `onData` | Stale closure after first `isRunning` state change | Always calls current callback via `onDataRef` |
| Arrow keys during command run | Mutated `inputRef` and clobbered live output | No-op when `isRunning` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm exec tsc --noEmit` | 0 errors | 0 errors | ✅ |
| `pnpm biome check components/terminal/` | 0 errors | 0 errors | ✅ |
| `pnpm biome check app/api/logs/route.ts` | 0 errors | 0 errors | ✅ |
| `git push` | Accepted at remote | `ac16331b..817ef92c` pushed | ✅ |
| lefthook pre-commit (biome, monolith, env-guard, claude-symlinks) | All pass | All pass | ✅ |

---

## Source IDs + Collections Touched

| Source ID | Collection | Job ID | Outcome |
|-----------|------------|--------|---------|
| `repo:axon_rust/docs/sessions/2026-03-01-xterm-terminal-emulator.md` | `cortex` | `5ae14e01-f94b-4b04-93f5-85d1c355e054` | ✅ Embedded + verified (query score 1.083) |

---

## Risks and Rollback

- **xterm.js bundle size** — Adds ~500KB to the web client bundle. Mitigated by lazy loading via the wrapper's dynamic import.
- **WS protocol coupling** — Terminal sends `{ type: 'execute', mode, input, flags: {} }` directly. If the backend changes the allowed modes or message shape, terminal input silently fails. Monitor `command.error` messages.
- **Rollback** — `git revert ac16331b` removes all new terminal files. The `logs/route.ts` fix can be kept independently.

---

## Decisions Not Taken

- **`next/dynamic` for the terminal wrapper** — Rejected because `dynamic()` doesn't forward refs cleanly through `forwardRef` in Next.js 16. Manual `useEffect`+`useState` pattern chosen instead.
- **Full pty/shell** — This is a thin WS wrapper around axon's existing command execution, not a real PTY. A real PTY would require backend changes.
- **`readline` library** — Implemented basic readline (Enter, Backspace, Ctrl+C/K, arrows) inline rather than pulling in a dependency for a narrow use case.
- **`useEffect` dep array for `onData` and `onResize`** — Would cause terminal remount on every `isRunning` change, destroying scroll history. `onDataRef` pattern chosen instead.

---

## Open Questions

- The `TerminalHistory` class has no `'use client'` directive. It uses `typeof window` guards. If this module is ever imported from a Server Component, the guards prevent crashes but the intent is implicit. Consider adding `'use client'` as explicit documentation.
- The `use-terminal-session` hook's `writeln` is in the `useEffect` dep array via `subscribe`. It's stable today (depends on `terminalRef` which is stable), but could break if `terminalRef` becomes non-stable. Moving `writeln` inline in the effect would eliminate the implicit dependency.
- GitHub Dependabot flagged 2 high-severity vulnerabilities on the default branch. These are pre-existing and unrelated to this session's changes.

---

## Next Steps

- Verify terminal renders in-browser at `https://axon.tootie.tv/terminal`
- Test command execution: `scrape https://example.com` → output streams in terminal
- Test history: submit a command, press Up arrow → command appears
- Test Ctrl+C: start `crawl`, press Ctrl+C → cancel dispatched
- Consider adding `'use client'` to `lib/terminal-history.ts` for explicitness
- Address GitHub Dependabot vulnerabilities (pre-existing, not introduced this session)
