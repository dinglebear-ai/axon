# Axon Shell State Endgame Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the shell-state narrowing effort by splitting `layout` into `layoutState` and `layoutActions`, then split `AxonShell` into memoized mobile, desktop, sidebar, conversation, and right-pane subtrees with matching documentation and verification.

**Architecture:** Keep `useAxonShellState()` as the composition boundary for `AxonShell`, but narrow the remaining mixed `layout` concern into separate state and action groups while preserving the existing helper hooks and store ownership. Once the hook contract is stable, split the shell render tree into memoized mobile, desktop, sidebar, conversation, and right-pane components that align with those hook boundaries so the state surface and component surface reinforce each other.

**Tech Stack:** React, Next.js App Router, Zustand, Vitest, Testing Library

---

## File map

- Modify: `apps/web/components/shell/axon-shell-state.ts`
- Modify: `apps/web/components/shell/axon-shell.tsx`
- Modify: `apps/web/__tests__/axon-shell-state.test.ts`
- Modify: `apps/web/__tests__/axon-shell.test.tsx`
- Modify: `apps/web/AGENTS.md`
- Modify: `apps/web/.full-review/05-final-report.md`
- Create: `apps/web/components/shell/axon-shell-mobile.tsx`
- Create: `apps/web/components/shell/axon-shell-desktop.tsx`
- Create: `apps/web/components/shell/axon-shell-sidebar-pane.tsx`
- Create: `apps/web/components/shell/axon-shell-conversation-pane.tsx`
- Create: `apps/web/components/shell/axon-shell-right-pane.tsx`

## Guardrails

- Preserve behavior. This is a contract-narrowing and render-boundary refactor, not a feature pass.
- Follow TDD for each task: write or tighten the failing test first, then make the smallest implementation change.
- Do not split components and hook contracts in the same step.
- The component split is required for this plan; if a proposed component boundary creates a giant catch-all props bag, refine the boundary rather than skipping the split.
- Prefer explicit concern props (`layoutState`, `layoutActions`, `conversation`, `composer`, `sidebar`, `editor`, `settings`) over passing an omnibus `shell` object into extracted components.
- The documentation phase is required, not cleanup-only; each architectural change must be reflected in app docs and the review report before the plan is complete.

## Endpoint criteria

This plan is complete only when all of the following are true:
- `useAxonShellState()` exposes `layoutState` and `layoutActions` as distinct groups.
- `AxonShell` delegates render work to extracted memoized subtrees for:
  - `AxonShellMobile`
  - `AxonShellDesktop`
  - `AxonShellSidebarPane`
  - `AxonShellConversationPane`
  - `AxonShellRightPane`
- Focused tests lock both the hook contract and the extracted component boundaries.
- App docs and the review report describe both the hook split and the component split as the current endpoint of the shell-state narrowing effort.

## Chunk 1: Finish the hook-level split

### Task 1: Lock the `layoutState` / `layoutActions` contract with tests

**Files:**
- Modify: `apps/web/__tests__/axon-shell-state.test.ts`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Write the failing contract test**

Extend `useAxonShellState()` tests to assert that:
- `result.current.layoutState` contains view state only: `canvasProfile`, `chatFlex`, `chatOpen`, `editorOpen`, `isDragging`, `layoutRestored`, `mobilePane`, `railMode`, `rightPane`, `sidebarOpen`, `sidebarWidth`, `transitionClass`, `density`, and `sectionRef`
- `result.current.layoutActions` contains mutators only: `handleCanvasProfileChange`, `nudgeChatFlex`, `nudgeSidebar`, `persistChatOpen`, `persistRightPane`, `persistSidebarOpen`, `resetChatFlex`, `resetSidebarWidth`, `setMobilePaneTracked`, `setRailModeTracked`, `startChatResize`, `startSidebarResize`, and `setDensityTracked`
- the old mixed `layout` object is no longer returned

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: FAIL because `useAxonShellState()` still returns a mixed `layout` object.

- [ ] **Step 3: Add stability assertions before implementation**

Add a second failing test that mutates only editor or conversation state and asserts:
- `layoutState` identity stays stable
- `layoutActions` identity stays stable
- only the changed concern churns

- [ ] **Step 4: Run the focused test again and verify it fails for the right reason**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: FAIL on the missing `layoutState` / `layoutActions` shape or stability assertions, not on unrelated mocks.

- [ ] **Step 5: Commit the tests**

```bash
git add apps/web/__tests__/axon-shell-state.test.ts
git commit -m "test: lock split layout shell state contract"
```

### Task 2: Implement the `layoutState` / `layoutActions` split

**Files:**
- Modify: `apps/web/components/shell/axon-shell-state.ts`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Write the minimal hook change**

Refactor `useAxonShellState()` so it returns:
- `layoutState`
- `layoutActions`
- `conversation`
- `composer`
- `sidebar`
- `editor`
- `settings`
- `canvasRef`

Implementation requirements:
- keep `useAxonShellLayoutControls()` intact
- build separate `useMemo()` objects for `layoutState` and `layoutActions`
- keep all existing callbacks and behavior unchanged
- do not add a second abstraction layer inside `axon-shell-state-layout.ts` yet

- [ ] **Step 2: Run the focused hook test**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: PASS.

- [ ] **Step 3: Update any stale top-level or grouped names inside the hook test mocks**

If the tests still fail because of unstable mocks or renamed fields, fix the test harness only as needed.

- [ ] **Step 4: Re-run the focused hook test**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: PASS with stable `layoutState` / `layoutActions` identities.

- [ ] **Step 5: Commit the hook split**

```bash
git add apps/web/components/shell/axon-shell-state.ts apps/web/__tests__/axon-shell-state.test.ts
git commit -m "refactor: split axon layout state and actions"
```

## Chunk 2: Rewire the `AxonShell` consumer

### Task 3: Update `AxonShell` to consume `layoutState` / `layoutActions`

**Files:**
- Modify: `apps/web/components/shell/axon-shell.tsx`
- Modify: `apps/web/__tests__/axon-shell.test.tsx`
- Test: `apps/web/__tests__/axon-shell.test.tsx`

- [ ] **Step 1: Write the failing consumer test**

Update the mocked `useAxonShellState()` return in `apps/web/__tests__/axon-shell.test.tsx` so `AxonShell` receives `layoutState` and `layoutActions` instead of `layout`.

Keep coverage focused on:
- desktop sidebar rendering
- desktop conversation header/message rendering
- mobile sidebar/chat switching
- settings pane props

- [ ] **Step 2: Run the shell consumer test and verify it fails**

Run: `pnpm test -- __tests__/axon-shell.test.tsx`

Expected: FAIL because `AxonShell` still reads `shell.layout.*`.

- [ ] **Step 3: Refactor `AxonShell` minimally**

Inside `apps/web/components/shell/axon-shell.tsx`:
- replace `const layout = shell.layout` with `const layoutState = shell.layoutState` and `const layoutActions = shell.layoutActions`
- update all call sites so reads use `layoutState.*` and mutators use `layoutActions.*`
- do not extract subcomponents yet

- [ ] **Step 4: Run targeted shell verification**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts __tests__/axon-shell.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit the consumer rewrite**

```bash
git add apps/web/components/shell/axon-shell.tsx apps/web/__tests__/axon-shell.test.tsx apps/web/__tests__/axon-shell-state.test.ts
git commit -m "refactor: consume split axon layout state"
```

## Chunk 3: Required component-level pass

### Task 4: Lock the component boundaries with tests

**Files:**
- Modify: `apps/web/__tests__/axon-shell.test.tsx`
- Test: `apps/web/__tests__/axon-shell.test.tsx`

- [ ] **Step 1: Add a failing test or render-structure assertion for the extracted boundaries**

Add focused assertions that demonstrate the required split points are real and testable, for example:
- mobile and desktop trees are rendered by dedicated components with the same visible output as before
- sidebar concerns are isolated enough to support a dedicated sidebar pane component
- conversation concerns are isolated enough to support a dedicated conversation pane component
- right-pane rendering can be delegated without mixing in sidebar or conversation behavior

Do not over-test implementation details; the goal is to prove the boundaries are clean.

- [ ] **Step 2: Run the targeted shell test and verify it fails**

Run: `pnpm test -- __tests__/axon-shell.test.tsx`

Expected: FAIL because the new subtree contract or named component boundary does not exist yet.

- [ ] **Step 3: Commit the failing test checkpoint**

```bash
git add apps/web/__tests__/axon-shell.test.tsx
git commit -m "test: lock axon shell component boundaries"
```

### Task 5: Extract the first memoized shell subtrees

**Files:**
- Create: `apps/web/components/shell/axon-shell-mobile.tsx`
- Create: `apps/web/components/shell/axon-shell-desktop.tsx`
- Create: `apps/web/components/shell/axon-shell-sidebar-pane.tsx`
- Create: `apps/web/components/shell/axon-shell-conversation-pane.tsx`
- Create: `apps/web/components/shell/axon-shell-right-pane.tsx`
- Modify: `apps/web/components/shell/axon-shell.tsx`
- Modify: `apps/web/__tests__/axon-shell.test.tsx`
- Test: `apps/web/__tests__/axon-shell.test.tsx`

- [ ] **Step 1: Extract `AxonShellMobile` and `AxonShellDesktop` first**

Move the mobile JSX and desktop JSX into dedicated components that receive already-separated concern props.

Rules:
- keep the root `AxonShell` responsible only for hook wiring and the `isMobile` branch
- avoid introducing a single giant props object named `shell`
- pass `layoutState`, `layoutActions`, `conversation`, `composer`, `sidebar`, `editor`, and `settings` explicitly

- [ ] **Step 2: Run the targeted shell test**

Run: `pnpm test -- __tests__/axon-shell.test.tsx`

Expected: PASS or a small number of focused failures.

- [ ] **Step 3: Extract the pane-level components**

Extract:
- `AxonShellSidebarPane`
- `AxonShellConversationPane`
- `AxonShellRightPane`

Rules:
- keep each pane focused on one render concern
- do not move unrelated state derivation into the pane component
- keep callback behavior identical
- use `memo()` where the boundary is intended to protect against unrelated rerenders

- [ ] **Step 4: Re-run the targeted shell tests**

Run: `pnpm test -- __tests__/axon-shell.test.tsx __tests__/axon-shell-state.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit the subtree extraction**

```bash
git add apps/web/components/shell/axon-shell.tsx apps/web/components/shell/axon-shell-mobile.tsx apps/web/components/shell/axon-shell-desktop.tsx apps/web/components/shell/axon-shell-sidebar-pane.tsx apps/web/components/shell/axon-shell-conversation-pane.tsx apps/web/components/shell/axon-shell-right-pane.tsx apps/web/__tests__/axon-shell.test.tsx apps/web/__tests__/axon-shell-state.test.ts
git commit -m "refactor: split axon shell render subtrees"
```

## Chunk 4: Verification, docs, and endpoint decision

### Task 6: Verify, document, and record the endpoint

**Files:**
- Modify: `apps/web/AGENTS.md`
- Modify: `apps/web/.full-review/05-final-report.md`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`
- Test: `apps/web/__tests__/axon-shell.test.tsx`
- Test: `apps/web/__tests__/shell-store.test.ts`
- Test: `apps/web/__tests__/workspace-persistence.test.ts`
- Test: `apps/web/__tests__/shell/axon-cortex-pane-redesign.test.tsx`

- [ ] **Step 1: Update app docs with the final hook and component boundaries**

Update `apps/web/AGENTS.md` to document:
- `layoutState` and `layoutActions`
- the role of `conversation`, `composer`, `sidebar`, `editor`, and `settings`
- the extracted `AxonShellMobile` / `AxonShellDesktop` render split
- which dedicated pane components own sidebar, conversation, and right-pane rendering
- why this is the intended stopping point unless profiling identifies a new hotspot

- [ ] **Step 2: Update the review report thoroughly**

Update `apps/web/.full-review/05-final-report.md` so `PERF-H3` reflects:
- the final hook contract after the `layout` split
- the memoized component subtree split
- the remaining reasons the item is still partial or, if justified by the final result, whether the current endpoint should be treated as the practical resolution for now
- any explicit note that future work must be profiling-driven rather than decomposition-for-its-own-sake

- [ ] **Step 3: Run the targeted verification suite**

Run:
`pnpm test -- __tests__/axon-shell-state.test.ts __tests__/axon-shell.test.tsx __tests__/shell-store.test.ts __tests__/workspace-persistence.test.ts __tests__/shell/axon-cortex-pane-redesign.test.tsx`

Expected: PASS.

- [ ] **Step 4: Add a brief endpoint note to the plan or handoff summary**

Record in the execution handoff or completion summary:
- that the hook-level and component-level passes both landed
- that future shell-state work should require profiling evidence
- any intentionally deferred follow-up, if one remains

- [ ] **Step 5: Commit docs and verification updates**

```bash
git add apps/web/AGENTS.md apps/web/.full-review/05-final-report.md apps/web/__tests__/axon-shell-state.test.ts apps/web/__tests__/axon-shell.test.tsx
git commit -m "docs: record axon shell state endgame plan"
```
