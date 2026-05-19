# React Native Android Feasibility Study
**Date:** 2026-03-09
**Branch:** refactor/acp-performance-modern-rust
**Duration:** ~15 minutes

---

## Session Overview

Conducted a parallel-agent feasibility study to determine what it would take to build an Android app with React Native for Axon. Three independent exploration agents ran concurrently to cover frontend features, backend API surface, and hooks/reusability. Concluded that a native app is unnecessary given the existing PWA setup and Gotify for push notifications.

---

## Timeline

1. **Dispatched 3 parallel agents** via `dispatching-parallel-agents` skill pattern
   - Agent 1: Frontend feature audit (`apps/web/` screens, components, state, user flows)
   - Agent 2: Backend API & WebSocket protocol analysis (`crates/web/`, `docs/WS-PROTOCOL.md`, ACP)
   - Agent 3: Hooks & dependency reusability analysis (`apps/web/hooks/`, `package.json`)
2. **Agent 2 returned first** (~88s) — full REST + WS protocol map, auth mechanism, mobile concerns
3. **Agents 1 & 3 returned** (~118s) — complete UI inventory and reusability breakdown
4. **Synthesized findings** — presented unified assessment
5. **Decision reached** — PWA + Gotify covers all requirements; native app not worth the effort

---

## Key Findings

### Frontend Scope
- 18 screens/pages across: Omnibox (command shell), Pulse/Reboot Chat (ACP), Cortex dashboards (RAG ops), Terminal, Workspace, Settings
- Primary state management: React Context + `useReducer` (no Redux/Zustand)
- Two web-only features dominate complexity: **Plate.js rich text editor** (40+ plugins, 15 npm packages) and **xterm.js terminal** (node-pty subprocess)

### Why Plate.js Won't Work in React Native
- Built on Slate.js which requires `contenteditable` HTML — no DOM in RN
- Depends on `document.getSelection()`, `Selection` API, `Range` API, `MutationObserver`
- All Radix UI popover/toolbar components render DOM elements
- Not a porting problem — architecture assumes browser DOM exists

### Why xterm.js + node-pty Won't Work in React Native
- xterm.js renders to HTML `<canvas>` — no canvas in RN
- node-pty is a native Node.js addon using POSIX `forkpty()` — requires Node.js runtime, not available in Hermes/JSC
- RN has no `child_process`, no `fork()`, no OS process control

### Backend API (Ready for Mobile As-Is)
- 26 REST endpoints under `/api/*`
- Single multiplexed WebSocket at `/ws` with 25+ message types
- Auth: static token (`?token=` on WS, `x-api-key` header on REST) — simple, secure
- ACP: bidirectional LLM streaming with tool calls, permission gates, session tracking
- **One missing piece:** no CORS headers — would need ~10 lines added to axum for mobile

### Reusability Breakdown
- **~40–45% portable** to React Native: all Zod schemas, types, 9 hooks, API client logic
- **~55–60% must be rewritten**: all UI components, terminal, Plate.js, server routes
- Hooks that work as-is: `useAxonSession`, `useAxonAcp`, `useDebounce`, `usePulseAutosave`, `useTimedNotice`
- Hooks needing refactor: `useAxonWs` (replace browser event listeners with RN Network State)

### Effort Estimates
| Scope | Effort |
|-------|--------|
| MVP (Omnibox + Chat + Job monitor) | 8–12 weeks |
| Feature-complete (all 18 screens) | 20–30 weeks |
| Full parity (editor + terminal) | 40–60 weeks |

---

## Technical Decisions

### Decision: Do Not Build React Native App
**Rationale:**
- App is already a PWA (`service-worker.tsx` is wired in the codebase)
- PWA on Android via Chrome gives install-to-home-screen, offline capability, same codebase
- Push notifications covered by **Gotify** (already self-hosted and in workflow)
- Native app would cost 6–20 weeks minimum with no added capability for Axon's use case

### Decision: PWA + Gotify = Complete Mobile Story
- Gotify handles push notifications to Android natively via its app
- Self-hosted, no Firebase/FCM dependency, works over Tailscale
- Already integrated into the project workflow

---

## Files Modified

None — this was a read-only feasibility study.

---

## Commands Executed

None — all analysis performed by subagents via file exploration tools.

---

## Behavior Changes (Before/After)

None — no code changes made.

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Agent 1 (frontend audit) | Complete screen/component inventory | 18 screens, full component tree, state mgmt mapped | ✅ |
| Agent 2 (API/WS protocol) | REST + WS endpoints, auth, mobile concerns | 26 REST routes, 25 WS message types, full auth docs | ✅ |
| Agent 3 (hooks/reusability) | Dependency classification, % reusable | 103 deps classified, 40–45% portable, monorepo structure | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session.

---

## Risks and Rollback

None — read-only session, no changes made.

---

## Decisions Not Taken

- **React Native Android app** — 6–20 weeks effort, replaces something already solved by PWA + Gotify
- **Expo** — was going to be the recommended starting point if native was chosen
- **NativeWind** — recommended styling approach if native was chosen (80% Tailwind compatibility)
- **TextInput + markdown preview** — simplest editor replacement for RN MVP (not Plate.js)
- **Shared packages extraction** (`packages/shared-types`, `packages/axon-client`) — valid regardless of mobile, but not prioritized without a native app target

---

## Open Questions

- Service worker in `service-worker.tsx` — is it fully configured for offline use or just registered?
- PWA manifest (`manifest.json`) — does it exist / is it configured for Android install prompts?

---

## Next Steps

- Verify PWA manifest and service worker are properly configured for Android installability
- Gotify is already integrated — no additional setup needed for push notifications
