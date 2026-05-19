# Axon Shell State Narrowing Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce the re-render blast radius of `useAxonShellState` by replacing its flat view-model with grouped concern-based sections while preserving `AxonShell` behavior.

**Architecture:** Keep `useAxonShellState` as the top-level composition hook for now, but return memoized grouped sections such as `layout`, `chat`, `editor`, and `settings` instead of one broad flat object. Update `AxonShell` to consume those groups incrementally so the existing helper hooks (`axon-shell-state-layout`, `-session`, `-messages`, `-settings`, `-actions`) stay intact while the shell-facing API becomes narrower and more explicit.

**Tech Stack:** React, Next.js App Router, Zustand, Vitest, Testing Library

---

## File map

- Modify: `apps/web/components/shell/axon-shell-state.ts`
- Modify: `apps/web/components/shell/axon-shell.tsx`
- Modify: `apps/web/__tests__/shell-store.test.ts`
- Create: `apps/web/__tests__/axon-shell-state.test.ts`
- Modify: `apps/web/AGENTS.md`
- Modify: `apps/web/.full-review/05-final-report.md`

## Chunk 1: Group the shell-state contract

### Task 1: Lock the first grouped contract with tests

**Files:**
- Create: `apps/web/__tests__/axon-shell-state.test.ts`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Write the failing test**

Add a focused test for `useAxonShellState()` that asserts the returned object exposes grouped sections for the lowest-risk concerns first:
- `layout` with `canvasProfile`, `mobilePane`, `chatOpen`, `rightPane`, `sidebarOpen`, and existing layout mutators
- `settings` with `enableFs`, `enableTerminal`, `permissionTimeoutSecs`, `adapterTimeoutSecs`, and their setters
- `chat` with `agentLabel`, `chatTitle`, `displayMessages`, `liveMessages`, `sessionLoading`, `sessionError`, `reloadSession`, and composer-related callbacks
- `editor` with `editorMarkdown`, `setEditorMarkdown`, and `onEditorUpdate`

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: FAIL because the hook still returns a flat object.

- [ ] **Step 3: Write minimal implementation**

Refactor `apps/web/components/shell/axon-shell-state.ts` so the returned value is grouped into memoized sections.

Guidelines:
- Keep existing helper hooks and derived values intact.
- Build small memoized group objects (`layoutState`, `settingsState`, `chatState`, `editorState`).
- Return a thin top-level object composed from those group objects plus only the root-level fields still genuinely shared.
- Do not change behavior of existing callbacks.

- [ ] **Step 4: Run test to verify it passes**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/components/shell/axon-shell-state.ts apps/web/__tests__/axon-shell-state.test.ts
git commit -m "refactor: group axon shell state contract"
```

### Task 2: Update `AxonShell` to consume grouped state

**Files:**
- Modify: `apps/web/components/shell/axon-shell.tsx`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Extend test coverage for grouped consumption**

Add assertions that `AxonShell` still renders correctly when consuming grouped shell state, especially:
- mobile sidebar/chat/editor switching
- desktop chat title / live message count / disconnected badge
- settings pane props

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: FAIL because `AxonShell` still expects flat fields.

- [ ] **Step 3: Write minimal implementation**

Refactor `apps/web/components/shell/axon-shell.tsx` to read from grouped sections:
- `shell.layout.*`
- `shell.chat.*`
- `shell.editor.*`
- `shell.settings.*`

Keep render output and callbacks unchanged.

- [ ] **Step 4: Run targeted tests**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts __tests__/shell-store.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/components/shell/axon-shell.tsx apps/web/__tests__/axon-shell-state.test.ts apps/web/__tests__/shell-store.test.ts
git commit -m "refactor: consume grouped axon shell state"
```

## Chunk 2: Tighten memo boundaries and remove leftover flat fields

### Task 3: Minimize the remaining top-level shell object

**Files:**
- Modify: `apps/web/components/shell/axon-shell-state.ts`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Write the failing test**

Add assertions that the top-level return value is limited to grouped sections plus only intentionally shared primitives such as `canvasRef`.

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts`

Expected: FAIL until leftover flat fields are removed.

- [ ] **Step 3: Write minimal implementation**

Trim duplicated top-level fields once `AxonShell` no longer depends on them.

- [ ] **Step 4: Run targeted verification**

Run: `pnpm test -- __tests__/axon-shell-state.test.ts __tests__/use-ws-messages.test.ts __tests__/workspace-persistence.test.ts`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/components/shell/axon-shell-state.ts apps/web/__tests__/axon-shell-state.test.ts
git commit -m "refactor: narrow top-level axon shell state surface"
```

## Chunk 3: Verification and docs

### Task 4: Verify and document the narrowed shell-state boundary

**Files:**
- Modify: `apps/web/AGENTS.md`
- Modify: `apps/web/.full-review/05-final-report.md`
- Test: `apps/web/__tests__/axon-shell-state.test.ts`

- [ ] **Step 1: Update docs**

Document that `useAxonShellState` is now a grouped composition hook rather than a flat 60-field view-model.

- [ ] **Step 2: Run verification suite**

Run:
`pnpm test -- __tests__/axon-shell-state.test.ts __tests__/shell-store.test.ts __tests__/shell/axon-cortex-pane-redesign.test.tsx __tests__/workspace-persistence.test.ts`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/AGENTS.md apps/web/.full-review/05-final-report.md apps/web/__tests__/axon-shell-state.test.ts
git commit -m "docs: record axon shell state narrowing"
```
