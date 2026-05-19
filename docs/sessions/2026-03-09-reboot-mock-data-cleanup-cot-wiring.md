# Session: Remove Mock Data + Wire Chain-of-Thought — `/reboot`

**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Duration:** Single focused session

---

## Session Overview

Executed a 7-file refactor to clean up `axon-mock-data.ts`, which was a dumping ground mixing real UI config with fake data and a conflicting `MessageItem` type. Created a canonical `AxonMessage` type used everywhere, extracted real UI constants to `axon-ui-config.ts`, and wired the `chainOfThought` field (previously accumulated by `useAxonAcp` but never rendered) into `AxonMessageList`.

---

## Timeline

1. **Read all affected files** — `axon-mock-data.ts`, `use-axon-session.ts`, `axon-message-list.tsx`, `axon-sidebar.tsx`, `axon-shell.tsx`, `axon-prompt-composer.tsx`, `use-axon-acp.ts`
2. **Created `axon-ui-config.ts`** — real UI constants only (`RAIL_MODES`, `AXON_PERMISSION_OPTIONS`, `RailMode`, `AxonPermissionValue`)
3. **Updated `use-axon-session.ts`** — promoted `MessageItem` → `AxonMessage` with full fields; added `ReasoningStep` type; kept `MessageItem` as deprecated alias
4. **Updated `axon-message-list.tsx`** — new import, prop type, `hasChainOfThought` check, `chainOfThought` renderer; removed dead `reasoning` block
5. **Updated `axon-sidebar.tsx`** — import from `axon-ui-config`; inlined `PAGE_ITEMS` and `AGENT_ITEMS` as local constants
6. **Updated `axon-shell.tsx`** — all imports updated; `displayMessages = liveMessages` (no unsafe cast); stale comment removed
7. **Updated `axon-prompt-composer.tsx`** — import from `axon-ui-config`
8. **Updated `use-axon-acp.ts`** — `MessageItem` → `AxonMessage`
9. **Deleted `axon-mock-data.ts`** — confirmed no remaining references
10. **Verified** — `pnpm lint` (0 errors), `pnpm test` (740/740 pass); pre-existing build error in `components/editor/transforms.ts:153` confirmed not introduced by this session

---

## Key Findings

- `axon-mock-data.ts` had two `MessageItem` definitions in the codebase — itself (with `timestamp: string`, no `chainOfThought`) and `use-axon-session.ts` (with `timestamp: number`, `chainOfThought`). The mock-data version was wrong.
- `chainOfThought: string[]` was accumulated by `useAxonAcp` on every `thinking_content` ACP event but the render check in `axon-message-list.tsx:158` only checked `message.steps?.length || message.reasoning || thinkingBlock` — never `chainOfThought`. Chain-of-thought text was silently dropped.
- `message.reasoning` had no real source anywhere in the system; it was dead render code.
- `axon-shell.tsx:595` used `as unknown as import('./axon-mock-data').MessageItem[]` — a double-cast that was both unsafe and referenced the deleted file.
- The build failure at `components/editor/transforms.ts:153` (Plate.js untyped generic call) was pre-existing — confirmed by `git stash` + build attempt on clean tree.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Keep `MessageItem` as deprecated alias | `use-axon-acp.ts` and `axon-shell.tsx` both referenced it; alias costs nothing and avoids a noisy rename in the ACP hook |
| Inline `PAGE_ITEMS` / `AGENT_ITEMS` in sidebar | They're only used in `axon-sidebar.tsx`; a shared module for a single consumer is premature abstraction. The "mock" label was misleading — they're navigation constants |
| Remove `message.reasoning` render block | No field named `reasoning` exists on `AxonMessage`; the mock-data type had it but nothing ever set it. Keeping it would be dead code on the canonical type |
| `chainOfThought.join('')` not `join('\n')` | Thinking chunks arrive as streaming deltas and already contain their own whitespace/newlines from the LLM output |
| `displayMessages = liveMessages` (no cast) | Both `liveMessages` and `AxonMessageList.messages` are now `AxonMessage[]` — cast was only needed because of the type mismatch with the old mock-data type |

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `apps/web/components/reboot/axon-mock-data.ts` | **Deleted** | Removed dumping ground |
| `apps/web/components/reboot/axon-ui-config.ts` | **Created** | Real UI constants: `RAIL_MODES`, `AXON_PERMISSION_OPTIONS`, `RailMode`, `AxonPermissionValue` |
| `apps/web/hooks/use-axon-session.ts` | **Modified** | Canonical `AxonMessage` type with `blocks?`, `toolUses?`, `steps?`, `chainOfThought?`; `ReasoningStep` type; deprecated `MessageItem` alias |
| `apps/web/components/reboot/axon-message-list.tsx` | **Modified** | Import `AxonMessage`; `hasChainOfThought` includes `chainOfThought`; renders thinking chunks; removed dead `reasoning` block |
| `apps/web/components/reboot/axon-sidebar.tsx` | **Modified** | Import from `axon-ui-config`; `PAGE_ITEMS` and `AGENT_ITEMS` inlined as local constants |
| `apps/web/components/reboot/axon-shell.tsx` | **Modified** | All imports updated; `liveMessages` typed as `AxonMessage[]`; cast removed; stale comment removed |
| `apps/web/components/reboot/axon-prompt-composer.tsx` | **Modified** | Import from `axon-ui-config` |
| `apps/web/hooks/use-axon-acp.ts` | **Modified** | `MessageItem` → `AxonMessage` throughout |

---

## Commands Executed

```bash
# Verified no remaining references to deleted file
grep -r "axon-mock-data" apps/web/  # → No matches

# Pre-existing build error check
git stash && cd apps/web && pnpm build  # → Build failed with same transforms.ts error
git stash pop                           # → Restored all changes

# Lint
pnpm lint   # → Checked 500 files in 291ms. No fixes applied. Found 6 warnings. Found 1 info.

# Tests
pnpm test   # → 67 test files, 740 tests, all passed
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| Chain-of-thought from ACP streaming | Accumulated in `liveMessages[n].chainOfThought[]` but never rendered; silently dropped | Rendered inside `<ChainOfThought>` accordion as `whitespace-pre-wrap` text block |
| Type safety | `axon-shell.tsx` used `as unknown as import('./axon-mock-data').MessageItem[]` | Direct assignment, no cast needed |
| Import graph | 4 files importing from `axon-mock-data` | 0 files; file deleted |
| `message.reasoning` render | Dead code path — field had no source | Removed from render entirely |
| `timestamp` field | Mock-data type had `timestamp?: string`; could cause NaN in `formatTimestamp()` | Canonical type has `timestamp: number` (required) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep -r "axon-mock-data" apps/web/` | No matches | No matches | ✅ PASS |
| `pnpm lint` | 0 errors | 0 errors, 6 pre-existing warnings | ✅ PASS |
| `pnpm test` | 740 pass | 740 pass (67 test files) | ✅ PASS |
| Build error in `transforms.ts:153` | Pre-existing | Confirmed pre-existing via `git stash` + build | ✅ CONFIRMED PRE-EXISTING |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations during this session (pure refactor).

---

## Risks and Rollback

**Risk:** `MessageItem` alias is deprecated but not removed — consumers that import it by the old name still compile. Future sessions should finish the rename.

**Risk:** `chainOfThought` renderer shows raw LLM thinking text, which may include artifacts from extended thinking mode. This is intentional but visually unformatted thinking is a new user-visible surface.

**Rollback:** `git revert` the relevant commits, or `git checkout HEAD -- apps/web/components/reboot/axon-mock-data.ts` to restore the file and revert the import changes.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Move `PAGE_ITEMS` / `AGENT_ITEMS` into `axon-ui-config.ts` | They're sidebar-only constants; putting them in a shared config would re-create the same anti-pattern with a different name |
| Remove `MessageItem` alias immediately | Too noisy for this PR scope; the alias makes the change backward-compatible with any untracked consumers |
| Render `chainOfThought` expanded by default | Most messages won't have thinking; an expanded CoT section would dominate the UI. `defaultOpen={false}` matches the existing `steps`/`thinkingBlock` behavior |
| Delete `message.reasoning` from the type entirely | The mock-data type exported it; removing it entirely from the canonical type would break anyone who accidentally relies on it — it's just unused now |

---

## Open Questions

- `ingest_thread` in `crates/ingest/reddit.rs:169` still uses `embed_text_with_metadata` instead of `extra_payload` — unrelated but known gap (from MEMORY.md)
- `MessageItem` alias should be cleaned up in a follow-up; `use-axon-acp.ts` and `axon-shell.tsx` are the only real consumers now
- The `steps` field on `AxonMessage` is described as "future use" — no current producer. Should it be added to `useAxonAcp` eventually?

---

## Next Steps

1. Remove `MessageItem` deprecated alias once all consumers (if any) are confirmed migrated
2. Test CoT rendering manually: connect ACP agent with extended thinking → confirm chain-of-thought accordion appears with thinking text
3. Address the pre-existing build error in `components/editor/transforms.ts:153` (Plate.js untyped generic)
4. Consider wiring `steps` field from structured ACP events if/when the ACP protocol adds structured reasoning steps
