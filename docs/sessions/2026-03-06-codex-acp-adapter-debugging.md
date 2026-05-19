# Session: Codex ACP Adapter Debugging + Unified Model Probing

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Duration:** ~2 hours (continued from prior compacted session)

## Session Overview

Systematically debugged Codex and Claude ACP adapters in the Axon web UI. Fixed multiple issues preventing model selection and chat responses from working. Unified the config probe and model selection logic so both agents use ACP-based dynamic model discovery instead of hardcoded values.

## Timeline

1. **Verified infrastructure** — Confirmed `axon serve` (PID 2782790, port 3939) and Next.js dev server (port 3000) running
2. **Discovered SSR crash** — `isPulseWorkspaceActive` TDZ error in page.tsx; turned out to be transient Turbopack cache issue, self-resolved
3. **Tested config probes** — Codex probe returned 6 models; Claude probe returned empty `[]`
4. **Root-caused Claude probe failure** — `route.ts:43` had `if (req.agent !== 'codex') return empty` guard
5. **Tested actual chat responses** — Codex: connected but hit OpenAI usage limit; Claude: full round-trip working (12.5s, "Hey there. What can I help you build today?")
6. **Fixed config probe** — Removed Codex-only guard, all agents now probed via ACP
7. **Fixed hardcoded Claude models** — Removed `CLAUDE_MODEL_OPTIONS` branching from settings, omnibox, and workspace hooks
8. **Unified model discovery** — All agents now use ACP probe results for model selection

## Key Findings

- **Claude ACP works end-to-end**: `started` -> `config_options_update` (3 models) -> `assistant_delta`s -> `finalizing` -> `done` (~12.5s)
- **Codex ACP works but rate-limited**: OpenAI `usage_limit_exceeded`, resets Mar 10th 2026
- **Config probe was Codex-only**: `app/api/pulse/config/route.ts:43` — `if (req.agent !== 'codex') return Response.json({ configOptions: [] })`
- **Frontend had hardcoded Claude models** in 3 locations: `settings-sections.tsx`, `omnibox-input-bar.tsx`, `use-pulse-workspace.ts`
- **Probe effect had dependency bug**: `[pulseAgent, pulseModel]` deps caused re-probe loops when model changed
- **ACP SDK `usage_update` parse error**: Non-fatal — `agent_client_protocol` crate doesn't recognize new `usage_update` session update variant. Logged as ERROR but doesn't break sessions.
- **Large `available_commands_update` message split**: Claude's skills listing is so large the ACP RPC parser splits it mid-stream. Non-fatal.

## Technical Decisions

- **Removed Codex-only guard** instead of adding Claude as second case — all future agents automatically supported
- **Removed hardcoded `CLAUDE_MODEL_OPTIONS`** — both agents now use dynamic ACP probe results uniformly
- **Removed `pulseModel` from probe effect deps** — prevents re-probe loops; model is set once when probe returns
- **Kept `usage_update` parse error unfixed** — this is in the upstream `agent_client_protocol` crate, not our code

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/api/pulse/config/route.ts` | Removed Codex-only guard (line 43-45 deleted) |
| `apps/web/hooks/use-ws-messages.ts:260-290` | Unified probe effect for all agents; removed Claude clear + Codex-only guard |
| `apps/web/app/settings/settings-sections.tsx:57-67` | Removed `pulseAgent === 'claude'` branch using `CLAUDE_MODEL_OPTIONS` |
| `apps/web/components/omnibox/omnibox-input-bar.tsx:132-161` | Removed hardcoded Claude model branching + `CLAUDE_MODEL_IDS` import |
| `apps/web/hooks/use-pulse-workspace.ts:307-310` | Removed hardcoded `['sonnet', 'opus', 'haiku']` keyboard shortcut mapping |
| `apps/web/__tests__/api/pulse-config-route.test.ts:92-104` | Updated test: Claude now probed via WS, not short-circuited |

### Prior Session Changes (from compacted context)

| File | Purpose |
|------|---------|
| `apps/web/lib/pulse/types.ts:48,75` | `PulseModel` changed from `z.string().default('sonnet')` to `z.string().optional()` |
| `apps/web/hooks/use-ws-messages.ts:271` | Removed model from config probe call |
| `apps/web/app/api/pulse/chat/route.ts:464-469` | Simplified model flag logic |
| `apps/web/app/settings/settings-sections.tsx:67` | Updated fallback label to "Loading models..." |
| `apps/web/__tests__/pulse-types.test.ts:43` | Updated test for optional model |
| `.env:116` | Changed `AXON_ACP_CODEX_ADAPTER_CMD` from `/usr/local/bin/codex-acp` to `codex-acp` |

## Commands Executed

| Command | Result |
|---------|--------|
| `curl POST /api/pulse/config {"agent":"codex"}` | 6 models returned (gpt-5.3-codex, gpt-5.4, etc.) |
| `curl POST /api/pulse/config {"agent":"claude"}` | Before fix: `{configOptions:[]}`. After fix: 3 models (default/opus/haiku) |
| `curl POST /api/pulse/chat {"agent":"codex"}` | Connected, config options streamed, then `usage_limit_exceeded` |
| `curl POST /api/pulse/chat {"agent":"claude"}` | Full response: "Hey there. What can I help you build today?" in 12.5s |
| `pnpm test` | 647/647 passing after all changes |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Claude config probe | Returned empty `[]` immediately | Returns 3 models via ACP (Default/Opus/Haiku) |
| Claude model selector (settings) | Hardcoded: sonnet/opus/haiku | Dynamic from ACP probe |
| Claude model selector (omnibox) | Hardcoded: `CLAUDE_MODEL_OPTIONS` | Dynamic from ACP probe |
| Codex model selector | Only showed "Default" initially | Shows 6 real models from ACP probe |
| Model keyboard shortcuts (Ctrl+1/2/3) | Hardcoded Claude models | Uses ACP config options by index for all agents |
| Probe effect | Only fired for Codex, re-probed on model change | Fires for all agents, only on agent switch |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `curl POST /api/pulse/config {"agent":"claude"}` | Models returned | 3 models (default, opus, haiku) | PASS |
| `curl POST /api/pulse/config {"agent":"codex"}` | Models returned | 6 models (gpt-5.3-codex, etc.) | PASS |
| `curl POST /api/pulse/chat {"agent":"claude"}` | Assistant text | "Hey there. What can I help you build today?" | PASS |
| `curl POST /api/pulse/chat {"agent":"codex"}` | Events streamed | Config options + usage_limit_exceeded error | PASS (rate limit is external) |
| `pnpm test` | 647 pass | 647 pass | PASS |

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during debugging.

## Risks and Rollback

- **Low risk**: All changes are frontend + one API route guard removal
- **Rollback**: `git checkout -- apps/web/` reverts all changes
- **Claude probe adds ~10s startup latency**: The ACP adapter takes ~10s to initialize and return config. Users will see "Loading models..." briefly. This is the same behavior Codex already had.

## Decisions Not Taken

- **Did not fix `usage_update` ACP SDK error** — upstream crate issue, not our code; non-fatal
- **Did not fix large message split** — ACP SDK's RPC parser doesn't handle multi-frame messages; non-fatal, sessions complete successfully
- **Did not remove `CLAUDE_MODEL_OPTIONS` / `settings-data.ts` exports** — they may be used elsewhere or useful as fallback reference; just removed the branching that preferred them

## Open Questions

- **Will Claude probe latency (~10s) be acceptable UX?** The settings page will show "Loading models..." for ~10s on first load. May want to cache probe results.
- **Should we persist ACP config options in localStorage?** Would eliminate re-probe on page refresh
- **`usage_update` variant**: When will `agent_client_protocol` crate add support? This fills logs with ERROR-level noise.
- **`conn.prompt()` race condition**: Previous session noted that `prompt()` may return before all `SessionNotification` messages arrive, leaving `assistant_text` empty. Not observed in practice but architecturally possible.

## Next Steps

- Monitor web UI for correct model display after page refresh
- Consider caching ACP probe results to avoid 10s delay on every page load
- File issue on `agent_client_protocol` for `usage_update` variant support
- Test Codex chat after usage limit resets (Mar 10th)
