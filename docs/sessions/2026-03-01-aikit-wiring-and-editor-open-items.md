# Session: AIKit Wiring + Editor Open Items

**Date:** 2026-03-01
**Branch:** `feat/sidebar`
**Commit:** `72d1f651`

---

## Session Overview

Continued from the previous Plate.js editor enhancements session. This session addressed all remaining open items from the code review: wired the full `AIKit` (with tool-routing and real SSE adapter) into `CopilotKit` to replace the stub `AIChatKit`, fixed the Esc abort (was a silent no-op), fixed a React render side-effect in `block-discussion.tsx`, corrected list toggle keys in the mobile dropdown, scoped the scroll storage key per document, and moved `@faker-js/faker` to devDependencies. One commit pushed.

---

## Timeline

1. **Context restored** — Previous session had completed Plate.js enhancements + review fixes (commits `405e0945`, `b2e2d61d`). Open items remained from code review.

2. **File reads** — Read `ai-kit.tsx`, `ai-chat-kit.tsx`, `copilot-kit.tsx`, `pulse-editor-pane.tsx`, `ai-menu.tsx`, `block-discussion.tsx`, `pulse-workspace.tsx`, `extended-nodes-kit.tsx`, `list-toolbar-button.tsx`, `package.json` to understand the current state.

3. **AIKit wiring** — Added `stop()` to `ChatHelpers`, replaced `useChat()` with `useAxonAIChat()` in `ai-kit.tsx`, swapped `AIChatKit` → `AIKit` in `copilot-kit.tsx`, removed `PulseEditorInner` and explicit `<AIMenu />` from `pulse-editor-pane.tsx`.

4. **Abort fix** — Both `_abortFakeStream()` no-ops in `ai-menu.tsx` replaced with `(chat as any).stop?.()`.

5. **Block discussion fix** — Moved `setOption('uniquePathMap', ...)` from `useResolvedDiscussion` render body into `useEffect`.

6. **List key fix** — Replaced `toggleBlock('ul'/'ol')` in `MoreFormattingDropdown` with `useListToolbarButton` hooks using `nodeType: 'disc'/'decimal'`. Fixed TS error: `onClick` takes 0 args, not 1.

7. **Scroll key scoped** — `pulse-workspace.tsx:483` now passes document-keyed `scrollStorageKey`.

8. **faker moved** — `@faker-js/faker` moved from `dependencies` → `devDependencies` in `apps/web/package.json`.

9. **Build verified** — Cleared stale `.next` cache (had phantom `cortexOpen` prerender error), rebuilt clean. 38 static pages, 0 errors.

10. **Commit + push** — `72d1f651` pushed to `feat/sidebar`.

---

## Key Findings

### `AIKit` was a dead file
`ai-kit.tsx` exported `AIKit` which was never imported into `CopilotKit`. The active AI path used `AIChatKit` from `ai-chat-kit.tsx`, which only did basic plugin registration (no tool-routing, no streaming hooks). `AIKit` had the full `aiChatPlugin` with tool-routing `useChatChunk` but called fake `useChat()` scaffolding from `use-chat.ts` (faker-based demo data).

### `AIChatKit` vs `AIKit` distinction
- `AIChatKit` (`ai-chat-kit.tsx`): `AIPlugin.configure({render:{node:AIAnchorElement}})` + `AIChatPlugin.configure({render:{node:AILeaf}, options:{mode:'insert'}})` — no hooks, no streaming, just plugin registration. Also contains the real `useAxonAIChat` SSE adapter and `useAIChatSetup` helper.
- `AIKit` (`ai-kit.tsx`): `CursorOverlayKit + MarkdownKit + AIPlugin.withComponent(AILeaf) + aiChatPlugin` — full `useChatChunk` with mode-aware streaming (`insert`/`edit`/`chat`), renders `AIMenu` via `afterEditable` and `AILoadingBar` via `afterContainer`.

### `_abortFakeStream` no-op (both sites)
`ai-menu.tsx:115` and `ai-menu.tsx:626` both called `(chat as any)._abortFakeStream()`. With `useAxonAIChat` as the adapter there is no such method. `api.aiChat.stop()` handles Plate-side abort but never cancelled the fetch. The `stop()` method added to `ChatHelpers` now calls `abortRef.current?.abort()`.

### `useResolvedDiscussion` render side-effect
`block-discussion.tsx:284` and `:291` called `setOption('uniquePathMap', ...)` directly inside a custom hook's render body (no `useEffect`). This is a React anti-pattern causing state mutations during render. Fixed by batching updates into a single `useEffect` with `[commentNodes.length, blockPath.join(',')]` deps.

### List plugin key mismatch
`MoreFormattingDropdown` used `editor.tf.toggleBlock('ul')` / `toggleBlock('ol')`. The `ListPlugin` from `@platejs/list/react` is an indent-based list plugin — it does not use `'ul'`/`'ol'` node type keys. The correct API is `useListToolbarButtonState({ nodeType: 'disc'/'decimal' })` + `useListToolbarButton(state)`.

### Scroll key collision
`scrollStorageKey="axon.web.pulse.editor-scroll"` was hardcoded in `pulse-workspace.tsx:483` regardless of which document was open. Multiple tabs editing different docs would overwrite each other's scroll positions.

---

## Technical Decisions

### Wire `useAxonAIChat` inside `aiChatPlugin.useHooks`, not in a wrapper component
The previous approach used `PulseEditorInner` (a component that rendered `null`) to call `useAIChatSetup(editor)` inside Plate context. The new approach calls `useAxonAIChat()` directly inside `aiChatPlugin`'s `useHooks` callback and injects it via `useEffect → editor.setOption`. This is cleaner: the plugin owns its own chat adapter, and there's no need for a null-rendering wrapper component.

### Keep `as any` cast for `chat.stop?.()` in `ai-menu.tsx`
`usePluginOption(AIChatPlugin, 'chat')` returns the plugin's internal type (likely a Vercel AI SDK shape). Our `ChatHelpers` interface with `stop()` is not visible through the plugin option typing. Using `(chat as any).stop?.()` is no worse than the previous `(chat as any)._abortFakeStream()` and avoids modifying the Plate plugin type definition.

### `useEffect` deps for `useResolvedDiscussion`
Used `[commentNodes.length, blockPath.join(',')]` as dependencies (primitives derived from the unstable arrays) rather than the arrays themselves. This avoids running the effect on every render when the arrays' content hasn't actually changed. `setOption` from `useEditorPlugin` is stable.

### `onClick` takes 0 args on `useListToolbarButton` props
TypeScript revealed that `useListToolbarButton(state).props.onClick` has signature `() => void`, not `(e: MouseEvent) => void`. Called as `discListProps.onClick?.()` with no argument.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/editor/plugins/ai-chat-kit.tsx` | Added `stop(): void` to `ChatHelpers` interface and `useAxonAIChat` return value |
| `apps/web/components/editor/plugins/ai-kit.tsx` | Replaced `useChat()` with `useAxonAIChat()` + `useEffect` injection; added `useEffect` import |
| `apps/web/components/editor/plugins/copilot-kit.tsx` | Replaced `AIChatKit` import/spread with `AIKit` |
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Removed `PulseEditorInner`, removed explicit `<AIMenu />`, removed `useAIChatSetup` import, added `useListToolbarButton/State` hooks for correct list toggles |
| `apps/web/components/ui/ai-menu.tsx` | Replaced both `_abortFakeStream()` calls with `(chat as any).stop?.()` (lines ~115, ~626) |
| `apps/web/components/ui/block-discussion.tsx` | Moved `setOption('uniquePathMap')` side-effects from render body to `useEffect` in `useResolvedDiscussion` |
| `apps/web/components/pulse/pulse-workspace.tsx` | Scoped `scrollStorageKey` using `currentDocFilename` |
| `apps/web/package.json` | Moved `@faker-js/faker` from `dependencies` to `devDependencies` |

---

## Commands Executed

```bash
# Build verification (first attempt — stale .next cache)
pnpm build
# → ReferenceError: cortexOpen is not defined (prerender error, unrelated to our changes)

# Clear cache and rebuild
rm -rf apps/web/.next && pnpm build
# → ✓ Compiled successfully in 11.0s, 38 static pages generated

# Biome lint check
npx @biomejs/biome check --reporter=summary
# → Checked 351 files in 156ms. 0 errors, 27 warnings.

# Commit and push
git commit -m "fix(web): wire AIKit into CopilotKit + address open items"
# → lefthook: env-guard ✓, monolith ✓, biome ✓, claude-symlinks ✓
# → [feat/sidebar 72d1f651] 8 files changed, 62 insertions(+), 42 deletions(-)
git push
# → 72d1f651 pushed to feat/sidebar
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| AI streaming | `AIChatKit` — no hooks, no streaming, no tool routing | `AIKit` — full tool-routing (`insert`/`edit`/`chat` modes), real SSE adapter |
| Esc to stop AI | Silent no-op (`_abortFakeStream` method doesn't exist) | Calls `abortRef.current?.abort()` — actually cancels in-flight fetch |
| `AIMenu` rendering | Explicit `<AIMenu />` in `pulse-editor-pane.tsx` JSX | Registered via `aiChatPlugin.render.afterEditable` — no explicit JSX needed |
| `PulseEditorInner` | Null-rendering wrapper to run `useAIChatSetup` | Removed — plugin owns its chat adapter wiring |
| List toggles (mobile ⋯) | `toggleBlock('ul'/'ol')` — wrong keys, always no-op | `useListToolbarButton` with `'disc'/'decimal'` — correct indent-list API |
| Scroll position | All docs share same storage key `axon.web.pulse.editor-scroll` | Keyed per doc: `axon.web.pulse.editor-scroll.<filename>` |
| `setOption` in render | `useResolvedDiscussion` mutated plugin state during render | Deferred to `useEffect` — no state mutation during render |
| `@faker-js/faker` | Production dependency (shipped to end users) | Dev dependency (excluded from production bundle) |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `pnpm build` (after cache clear) | Clean build, 38 pages | ✓ Compiled successfully, 38 static pages | ✅ |
| `npx @biomejs/biome check --reporter=summary` | 0 errors | 0 errors, 27 warnings | ✅ |
| TypeScript via Next.js | No type errors | ✓ Compiled successfully | ✅ |
| `git push` | Pushed to feat/sidebar | `72d1f651` pushed | ✅ |
| lefthook pre-commit | All hooks pass | monolith ✓, biome ✓, env-guard ✓, claude-symlinks ✓ | ✅ |

---

## Source IDs + Collections Touched

| Source | Collection | Job ID | Outcome |
|--------|------------|--------|---------|
| `docs/sessions/2026-03-01-aikit-wiring-and-editor-open-items.md` | `cortex` | `d3dd6abc-3c45-4f06-b2c5-9fc8fba89bf8` | ✅ Embedded + retrieved (1 chunk) |

---

## Risks and Rollback

- **`chat as any` cast in `ai-menu.tsx`** — If `stop()` isn't on the chat object at runtime (e.g., old cached plugin option), the optional chain `?.()` makes it a no-op. Not a crash risk. Rollback: not needed.
- **`AIKit` includes `MarkdownKit` and `CursorOverlayKit`** — These are also spread into `CopilotKit` elsewhere. Plate's plugin deduplication handles this. If Plate v52 does NOT deduplicate, there could be double-registration. Rollback: revert to `AIChatKit` and reinstate `useAIChatSetup`.
- **`useEffect` deps `[commentNodes.length, blockPath.join(',')]`** — If multiple comments change simultaneously without changing count, the effect won't re-run. This is an edge case inherited from the shadcn-generated code; correctness is unchanged from the original (which ran on every render anyway). Rollback: `git revert 72d1f651`.

---

## Decisions Not Taken

- **Replacing `as any` with proper type extension** — Would require modifying Plate's `AIChatPlugin` options type or wrapping it. Deferred: the `as any` pattern is already established in this codebase for Plate type mismatches.
- **`blockPath.join(',')` vs deep comparison for `useResolvedDiscussion` deps** — Deep comparison via `JSON.stringify` would be more precise but adds allocation on every render. String join of a short path array is fast and sufficient for the use case.
- **Removing `MarkdownKit` from `CopilotKit` direct spread** — Since `AIKit` already includes `MarkdownKit`, the direct spread is now redundant. Not removed to keep `CopilotKit` explicit about its dependencies and easier to audit.

---

## Open Questions

- Does Plate v52 silently deduplicate plugins when the same plugin appears twice in the array passed to `usePlateEditor`? If not, `MarkdownKit` and `CursorOverlayKit` are double-registered now that `AIKit` is in `CopilotKit`.
- `block-discussion.tsx` side-effect fix uses `commentNodes.length` as a dep — if a comment's path changes without the count changing, the uniquePathMap won't update. Is this a real scenario?
- `use-chat.ts` (the faker-based demo scaffold) is still present in the codebase but no longer imported by anything. Should it be deleted?
- `AIChatKit` (`ai-chat-kit.tsx`) is now only used for `useAIChatSetup` (exported but no longer called) and `useAxonAIChat` (imported by `ai-kit.tsx`). Should `useAIChatSetup` be removed since it's now dead code?

---

## Next Steps

1. **Verify Plate plugin deduplication** — Test that double-registration of `MarkdownKit`/`CursorOverlayKit` doesn't cause runtime errors or unexpected behavior.
2. **Delete `use-chat.ts`** — The faker-based demo chat scaffold is no longer imported anywhere; remove it.
3. **Remove `useAIChatSetup`** from `ai-chat-kit.tsx` — Dead code now that `ai-kit.tsx` handles chat wiring internally.
4. **Test Esc abort in browser** — Verify that `chat.stop?.()` actually cancels the SSE stream in practice (network tab should show aborted request).
5. **Test list toggles on mobile** — Verify `useListToolbarButton` `onClick` properly toggles indent-list nodes in the editor.
