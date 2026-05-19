# Workspace Multi-Mode Docs Editing (Claude/Codex/Gemini) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extend the current `pulse`-only workspace into a multi-mode docs workspace that supports `claude`, `codex`, and `gemini` modes with safe read/write APIs, file explorer, PlateJS editing, and mode-aware AI prompt routing.

**Architecture:** Keep WebSocket execution for CLI modes unchanged, and route workspace modes entirely through Next.js API routes + client state in `useWsMessages`. Introduce a shared workspace file policy (allowed roots + safe path canonicalization), then build a reusable workspace shell rendered by `ResultsPanel` for all workspace modes. Reuse existing Pulse permission checks as guardrails for AI-generated operations while expanding prompt routing to include workspace mode context.

**Tech Stack:** Next.js 16 App Router, React 19, TypeScript 5.9, Vitest 4, PlateJS, Zod 4

---

## Scope and Non-Goals

- In scope:
  - Add workspace modes `claude`, `codex`, `gemini` to mode registries.
  - Generalize workspace rendering in `ResultsPanel`.
  - Add safe workspace file read/write APIs for:
    - `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`
    - `commands/**`, `skills/**`, `agents/**`, `prompts/**`
  - Add workspace file explorer sidebar.
  - Keep full PlateJS editor and load/save selected file.
  - Include workspace mode in AI prompt routing.
  - Keep permission checks for risky AI operations.

- Out of scope:
  - Rust WS backend command execution changes.
  - New auth model.
  - Replacing Pulse RAG behavior; this is a generalization, not a rewrite.

---

## Task 1: Add Workspace Modes To Registries (TDD First)

**Files:**
- Modify: `apps/web/lib/ws-protocol.ts`
- Modify: `apps/web/lib/axon-command-map.ts`
- Modify: `apps/web/__tests__/ws-protocol.test.ts`
- Modify: `apps/web/__tests__/command-map.test.ts`

**Step 1: Write failing protocol tests for new workspace modes**

Add assertions in `apps/web/__tests__/ws-protocol.test.ts`:

```ts
it('includes claude/codex/gemini workspace modes', () => {
  const ids = MODES.filter((m) => m.category === 'workspace').map((m) => m.id)
  expect(ids).toEqual(expect.arrayContaining(['pulse', 'claude', 'codex', 'gemini']))
})

it('marks all new workspace modes as workspace modes', () => {
  expect(isWorkspaceMode('claude')).toBe(true)
  expect(isWorkspaceMode('codex')).toBe(true)
  expect(isWorkspaceMode('gemini')).toBe(true)
})
```

**Step 2: Run test and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/ws-protocol.test.ts`
Expected: FAIL (missing mode ids).

**Step 3: Write failing command-map tests**

Add assertions in `apps/web/__tests__/command-map.test.ts`:

```ts
it('has command specs for claude/codex/gemini', () => {
  for (const mode of ['claude', 'codex', 'gemini']) {
    const spec = getCommandSpec(mode)
    expect(spec).toBeDefined()
    expect(spec?.category).toBe('workspace')
    expect(spec?.renderIntent).toBe('workspace')
    expect(spec?.input).toBe('text')
  }
})
```

**Step 4: Run test and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/command-map.test.ts`
Expected: FAIL (spec undefined).

**Step 5: Implement registry updates**

- In `apps/web/lib/ws-protocol.ts`, add `claude`, `codex`, `gemini` mode definitions under workspace category.
- In `apps/web/lib/axon-command-map.ts`, add command specs for each mode with:
  - `category: 'workspace'`
  - `input: 'text'`
  - `asyncByDefault: false`
  - `supportsJobs: false`
  - `renderIntent: 'workspace'`

**Step 6: Verify passing tests**

Run: `cd apps/web && pnpm vitest run __tests__/ws-protocol.test.ts __tests__/command-map.test.ts`
Expected: PASS.

**Step 7: Commit checkpoint**

```bash
git add apps/web/lib/ws-protocol.ts apps/web/lib/axon-command-map.ts apps/web/__tests__/ws-protocol.test.ts apps/web/__tests__/command-map.test.ts
git commit -m "feat(web): add claude/codex/gemini workspace modes"
```

---

## Task 2: Introduce Workspace File Policy + Safe Read/Write API

**Files:**
- Create: `apps/web/lib/workspace/file-policy.ts`
- Create: `apps/web/lib/workspace/file-service.ts`
- Create: `apps/web/app/api/workspace/files/route.ts`
- Create: `apps/web/__tests__/workspace-file-policy.test.ts`
- Create: `apps/web/__tests__/workspace-files-api.test.ts`

**Step 1: Write failing file-policy tests**

Create `apps/web/__tests__/workspace-file-policy.test.ts` to cover:
- allowed exact files: `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`
- allowed trees: `skills/**`, `prompts/**`, `agents/**`, `commands/**`
- reject traversal and absolute path escapes.

Example:

```ts
expect(isAllowedWorkspacePath('CLAUDE.md')).toBe(true)
expect(isAllowedWorkspacePath('skills/foo/SKILL.md')).toBe(true)
expect(isAllowedWorkspacePath('../.env')).toBe(false)
expect(isAllowedWorkspacePath('docs/plans/x.md')).toBe(false)
```

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-file-policy.test.ts`
Expected: FAIL (module missing).

**Step 3: Implement policy + canonical path guard**

In `apps/web/lib/workspace/file-policy.ts`:
- export allowlist constants.
- export `isAllowedWorkspacePath(relativePath: string): boolean`.
- export `resolveWorkspacePathOrThrow(...)` that canonicalizes and enforces workspace root containment.

In `apps/web/lib/workspace/file-service.ts`:
- `listWorkspaceFiles()` (returns grouped tree metadata).
- `readWorkspaceFile(path)`.
- `writeWorkspaceFile(path, content)` with size limits and UTF-8 text-only guard.

**Step 4: Write failing API tests**

Create `apps/web/__tests__/workspace-files-api.test.ts` for:
- `GET /api/workspace/files` list shape.
- `GET /api/workspace/files?path=...` reads allowed file.
- `PUT /api/workspace/files` writes allowed file.
- rejection cases for disallowed paths.

**Step 5: Implement route**

In `apps/web/app/api/workspace/files/route.ts`:
- `GET` with optional `path` query.
- `PUT` with `{ path, content }` body.
- Zod validation.
- return clear error statuses (`400`, `403`, `404`, `500`).

**Step 6: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-file-policy.test.ts __tests__/workspace-files-api.test.ts`
Expected: PASS.

**Step 7: Commit checkpoint**

```bash
git add apps/web/lib/workspace/file-policy.ts apps/web/lib/workspace/file-service.ts apps/web/app/api/workspace/files/route.ts apps/web/__tests__/workspace-file-policy.test.ts apps/web/__tests__/workspace-files-api.test.ts
git commit -m "feat(web): add safe workspace file read/write API"
```

---

## Task 3: Generalize Workspace Rendering Beyond Pulse

**Files:**
- Create: `apps/web/components/workspace/workspace-shell.tsx`
- Modify: `apps/web/components/results-panel.tsx`
- Modify: `apps/web/hooks/use-ws-messages.ts`
- Create: `apps/web/__tests__/workspace-rendering.test.tsx`

**Step 1: Write failing rendering tests**

Create `apps/web/__tests__/workspace-rendering.test.tsx`:
- if `workspaceMode === 'pulse'`, renders workspace shell.
- if `workspaceMode === 'claude'|'codex'|'gemini'`, renders workspace shell.
- if `workspaceMode === null`, preserves current non-workspace rendering.

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-rendering.test.tsx`
Expected: FAIL.

**Step 3: Implement reusable shell wiring**

- In `use-ws-messages.ts`, keep `workspaceMode/workspacePrompt` and add selected workspace file state:
  - `workspaceSelectedPath`
  - `setWorkspaceSelectedPath(...)`
- Create `WorkspaceShell` component that receives current mode and prompt via hook.
- In `results-panel.tsx`, replace Pulse-specific branch:

```tsx
if (workspaceMode) return <WorkspaceShell mode={workspaceMode} />
```

**Step 4: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-rendering.test.tsx`
Expected: PASS.

**Step 5: Commit checkpoint**

```bash
git add apps/web/components/workspace/workspace-shell.tsx apps/web/components/results-panel.tsx apps/web/hooks/use-ws-messages.ts apps/web/__tests__/workspace-rendering.test.tsx
git commit -m "refactor(web): generalize workspace rendering for all workspace modes"
```

---

## Task 4: Add Workspace File Explorer Sidebar + Selection Wiring

**Files:**
- Create: `apps/web/components/workspace/workspace-file-explorer.tsx`
- Modify: `apps/web/components/workspace/workspace-shell.tsx`
- Create: `apps/web/__tests__/workspace-file-explorer.test.tsx`

**Step 1: Write failing explorer tests**

Create `apps/web/__tests__/workspace-file-explorer.test.tsx`:
- renders grouped sections (`root`, `skills`, `prompts`, `agents`, `commands`).
- click item triggers `onSelect(path)`.
- currently selected path is highlighted.

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-file-explorer.test.tsx`
Expected: FAIL.

**Step 3: Implement sidebar component**

`workspace-file-explorer.tsx`:
- fetch list from `GET /api/workspace/files`.
- flatten/group for UI list.
- compact keyboard-friendly buttons.

`workspace-shell.tsx`:
- keep selected path state from hook.
- render left sidebar + right editor/chat split.
- default selection order: `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, then trees.

**Step 4: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-file-explorer.test.tsx`
Expected: PASS.

**Step 5: Commit checkpoint**

```bash
git add apps/web/components/workspace/workspace-file-explorer.tsx apps/web/components/workspace/workspace-shell.tsx apps/web/__tests__/workspace-file-explorer.test.tsx
git commit -m "feat(web): add workspace file explorer sidebar"
```

---

## Task 5: Keep PlateJS Editor And Load/Save Selected Workspace File

**Files:**
- Create: `apps/web/components/workspace/workspace-editor-pane.tsx`
- Modify: `apps/web/components/workspace/workspace-shell.tsx`
- Modify: `apps/web/components/pulse/pulse-editor-pane.tsx` (extract shared behavior if needed)
- Create: `apps/web/__tests__/workspace-editor-pane.test.tsx`

**Step 1: Write failing editor tests**

Create `apps/web/__tests__/workspace-editor-pane.test.tsx`:
- loads file content when path changes.
- debounced autosave sends `PUT /api/workspace/files`.
- save state badges: `idle/saving/saved/error`.

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-editor-pane.test.tsx`
Expected: FAIL.

**Step 3: Implement editor pane**

`workspace-editor-pane.tsx`:
- keep PlateJS editor (same stack as Pulse editor).
- on file selection, fetch file content and set editor value.
- on edit, serialize markdown and debounce save.
- show save status and last error.

`workspace-shell.tsx`:
- pass selected path + mode.
- keep existing Pulse behavior intact when mode is `pulse`.

**Step 4: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-editor-pane.test.tsx`
Expected: PASS.

**Step 5: Commit checkpoint**

```bash
git add apps/web/components/workspace/workspace-editor-pane.tsx apps/web/components/workspace/workspace-shell.tsx apps/web/components/pulse/pulse-editor-pane.tsx apps/web/__tests__/workspace-editor-pane.test.tsx
git commit -m "feat(web): load and save selected workspace files in PlateJS editor"
```

---

## Task 6: Include Workspace Mode In AI Prompt Path And Preserve Permission Checks

**Files:**
- Create: `apps/web/app/api/workspace/chat/route.ts`
- Modify: `apps/web/components/workspace/workspace-shell.tsx`
- Modify: `apps/web/lib/pulse/rag.ts`
- Modify: `apps/web/lib/pulse/permissions.ts`
- Create: `apps/web/__tests__/workspace-chat-route.test.ts`
- Modify: `apps/web/__tests__/pulse-permissions.test.ts`

**Step 1: Write failing chat-route tests**

Create `apps/web/__tests__/workspace-chat-route.test.ts` with cases:
- request includes `mode` and selected file context.
- system prompt includes mode marker (`Workspace mode: claude|codex|gemini|pulse`).
- operations still pass through `checkPermission` and are blocked when disallowed.

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-chat-route.test.ts`
Expected: FAIL.

**Step 3: Implement mode-aware workspace chat route**

`/api/workspace/chat/route.ts`:
- accept:

```ts
{ mode, prompt, selectedPath, documentMarkdown, selectedCollections, conversationHistory, permissionLevel }
```

- build system prompt using existing RAG helper + mode string.
- keep `checkPermission` call before returning operations.
- for non-`pulse` modes, default to read-only operation behavior in route (operations empty unless explicitly allowed by permission level).

**Step 4: Wire client prompt path**

`workspace-shell.tsx`:
- replace Pulse-only `/api/pulse/chat` calls with `/api/workspace/chat`.
- include `mode` and `selectedPath` in payload.

**Step 5: Expand permission tests**

Add in `pulse-permissions.test.ts`:
- non-current-doc remains blocked in `plan` mode.
- high-risk operations still require confirmation in `training-wheels`.

**Step 6: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/workspace-chat-route.test.ts __tests__/pulse-permissions.test.ts`
Expected: PASS.

**Step 7: Commit checkpoint**

```bash
git add apps/web/app/api/workspace/chat/route.ts apps/web/components/workspace/workspace-shell.tsx apps/web/lib/pulse/rag.ts apps/web/lib/pulse/permissions.ts apps/web/__tests__/workspace-chat-route.test.ts apps/web/__tests__/pulse-permissions.test.ts
git commit -m "feat(web): mode-aware workspace chat path with permission guardrails"
```

---

## Task 7: Omnibox/State Integration Hardening For New Workspace Modes

**Files:**
- Modify: `apps/web/components/omnibox.tsx`
- Modify: `apps/web/hooks/use-ws-messages.ts`
- Create: `apps/web/__tests__/omnibox-workspace-modes.test.tsx`

**Step 1: Write failing omnibox tests**

Create `apps/web/__tests__/omnibox-workspace-modes.test.tsx`:
- selecting `@claude`, `@codex`, or `@gemini` activates workspace mode.
- execute does not send WS `execute` when workspace mode is selected.
- prompt lands in `workspacePrompt` state.

**Step 2: Run and confirm failure**

Run: `cd apps/web && pnpm vitest run __tests__/omnibox-workspace-modes.test.tsx`
Expected: FAIL.

**Step 3: Implement state behavior**

- ensure `startExecution` resets workspace state only for non-workspace commands.
- preserve selected workspace file across prompts within same workspace mode.
- keep mention UX behavior unchanged.

**Step 4: Verify tests**

Run: `cd apps/web && pnpm vitest run __tests__/omnibox-workspace-modes.test.tsx`
Expected: PASS.

**Step 5: Commit checkpoint**

```bash
git add apps/web/components/omnibox.tsx apps/web/hooks/use-ws-messages.ts apps/web/__tests__/omnibox-workspace-modes.test.tsx
git commit -m "fix(web): harden omnibox workspace routing and state behavior"
```

---

## Task 8: Full Verification, Cleanup, And Final Acceptance Gate

**Files:**
- Modify as needed from earlier tasks only.

**Step 1: Run targeted tests added in this plan**

Run:

```bash
cd apps/web && pnpm vitest run \
  __tests__/ws-protocol.test.ts \
  __tests__/command-map.test.ts \
  __tests__/workspace-file-policy.test.ts \
  __tests__/workspace-files-api.test.ts \
  __tests__/workspace-rendering.test.tsx \
  __tests__/workspace-file-explorer.test.tsx \
  __tests__/workspace-editor-pane.test.tsx \
  __tests__/workspace-chat-route.test.ts \
  __tests__/omnibox-workspace-modes.test.tsx \
  __tests__/pulse-permissions.test.ts
```

Expected: PASS.

**Step 2: Run full suite + lint + build**

```bash
cd apps/web && pnpm test && pnpm lint && pnpm build
```

Expected: all green.

**Step 3: Manual smoke test checklist**

Run:

```bash
cd /home/jmagar/workspace/axon_rust/apps/web && pnpm dev
```

Validate:
1. Mode dropdown shows `Pulse`, `Claude`, `Codex`, `Gemini` under Workspace.
2. Selecting each mode opens workspace shell.
3. Sidebar lists allowed files only.
4. Selecting file loads content in PlateJS editor.
5. Editing file persists via API and survives refresh.
6. Workspace prompt includes mode context (visible in route logs/test output).
7. Permission-sensitive operations still require confirmation in training-wheels mode.

**Step 4: Final commit**

```bash
git add apps/web
git commit -m "feat(web): multi-mode workspace docs editor with safe APIs and mode-aware prompts"
```

---

## Risk Notes

- Path traversal/data-loss risk:
  - Mitigation: strict allowlist + canonical root checks + no writes outside allowed workspace paths.
- Symlink ambiguity (`AGENTS.md`/`GEMINI.md`):
  - Mitigation: treat them as first-class allowed files for reads; writes should resolve realpath and remain inside repo root.
- Editor autosave overwrite risk:
  - Mitigation: debounce + save status + server-side max-size and validation checks.
- Prompt behavior drift across modes:
  - Mitigation: explicit mode in prompt template and route-level tests for payload composition.
- Regression in existing Pulse flow:
  - Mitigation: preserve Pulse permission tests and keep current route behavior under compatibility branch until migration complete.

---

## Rollback Plan

1. Feature-flag workspace expansion with an env guard (e.g., `NEXT_PUBLIC_WORKSPACE_MULTI_MODE=true`).
2. If regressions occur:
   - disable new workspace modes in `MODES` and `AXON_COMMAND_SPECS`;
   - keep `pulse` branch only in `ResultsPanel`;
   - retain API route code but stop routing client traffic to it.
3. Revert in this order for minimal blast radius:
   - UI routes (`workspace-shell`, explorer/editor panes),
   - new API endpoints,
   - registry additions.
4. Confirm rollback with:
   - `pnpm vitest run __tests__/ws-protocol.test.ts __tests__/command-map.test.ts`
   - manual check that `pulse` mode still works.

---

## Acceptance Criteria

- [ ] Workspace mode list contains: `pulse`, `claude`, `codex`, `gemini`.
- [ ] `ResultsPanel` renders a generalized workspace shell for all workspace modes.
- [ ] API can list/read/write only allowed workspace files:
  - [ ] `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`
  - [ ] `skills/**`, `prompts/**`, `agents/**`, `commands/**`
- [ ] Workspace sidebar supports file selection and highlights active file.
- [ ] PlateJS editor loads and autosaves selected file content.
- [ ] Workspace prompt route includes current `mode` in system prompt path.
- [ ] Permission guardrails are still enforced for risky operations.
- [ ] Added tests pass, plus full `pnpm test`, `pnpm lint`, and `pnpm build`.

---

## Dependency Order

1. Task 1 (mode registry)
2. Task 2 (safe file API)
3. Task 3 (rendering generalization)
4. Task 4 (sidebar)
5. Task 5 (editor load/save)
6. Task 6 (mode-aware chat path + permission guardrails)
7. Task 7 (omnibox/state hardening)
8. Task 8 (final verification)

Parallelizable after Task 2:
- Task 4 and Task 6 can run in parallel if both consume the same file-policy module and agree on API payload shape.

---

Plan complete and saved to `docs/plans/2026-02-25-workspace-multi-mode-phase-2.md`. Two execution options:

1. Subagent-Driven (this session) - I dispatch fresh subagent per task, review between tasks, fast iteration
2. Parallel Session (separate) - Open new session with executing-plans, batch execution with checkpoints

Which approach?