# Session: ACP Config Options Overwrite Fix

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Duration:** ~1.5 hours (continuation of earlier ACP debugging session)

## Session Overview

Debugged why both Claude and Codex model selectors showed only "Default" in the web UI despite ACP probes returning real model options. Root-caused to a `useEffect` in `use-pulse-workspace.ts` that unconditionally overwrote probe results with empty chat config options. Fixed with a one-line guard. Verified both agents show real models via Chrome DevTools MCP.

## Timeline

1. **Verified probes work at API level** — `curl POST /api/pulse/config` returned real models for Codex (6 models) but Claude initially hung
2. **Killed stuck `claude-agent-acp` processes** — Two zombie processes from earlier probes (PIDs 2789901, 2794506, running since 06:59 AM)
3. **Tested Claude ACP adapter directly** — Confirmed adapter responds to `initialize` (protocol v1) and `session/new` with `mcpServers: []`
4. **Confirmed Claude probe works via WS** — Direct WS test showed full probe lifecycle completing in 7.4s with config options
5. **Root-caused the UI bug** — `use-pulse-workspace.ts:133-135` unconditionally synced empty `chatAcpConfigOptions` from `usePulseChat()` to WS state, overwriting probe results
6. **Applied fix** — Added `if (chatAcpConfigOptions.length > 0)` guard
7. **Verified via Chrome DevTools** — Both Claude (3 models) and Codex (6 models) display correctly in UI

## Key Findings

- **Root cause**: `use-pulse-workspace.ts:133` — `useEffect(() => { setAcpConfigOptions(chatAcpConfigOptions) }, ...)` overwrote probe results with `[]` from inactive chat hook
- **Two `acpConfigOptions` states exist**: one in `use-ws-messages.ts:124` (probe results) and one in `use-pulse-chat.ts:80` (chat session results). The workspace hook synced chat→WS unconditionally.
- **Claude ACP adapter v0.19.2** requires `protocolVersion: 1` (numeric, not string) and `mcpServers: []` in `session/new` — both correctly handled by SDK v0.9.5
- **Stuck adapter processes block probes** — Old `claude-agent-acp` processes from failed probes consumed resources; killing them restored functionality
- **Chrome DevTools origin check** — Remote Chrome at Tailscale IP was blocked by `AXON_WEB_ALLOWED_ORIGINS`; added `http://100.88.16.79:49010` and `https://axon.tootie.tv` to `.env.local`

## Technical Decisions

- **Guard with `length > 0`** instead of removing the sync entirely — chat sessions DO return updated config options (e.g., after `set_session_config_option`), and those should still flow to the UI
- **Did not add probe timeout to Rust `run_acp_event_loop`** — would be a good improvement but out of scope for this fix
- **Did not upgrade `agent-client-protocol` crate** — v0.9.5 works correctly with the v0.19.2 adapter; v0.10.0 is available but not needed

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/hooks/use-pulse-workspace.ts:133` | Added `length > 0` guard to prevent empty chat options from overwriting probe results |
| `apps/web/.env.local:6` | Added Tailscale IP and `axon.tootie.tv` to `AXON_WEB_ALLOWED_ORIGINS` |

## Commands Executed

| Command | Result |
|---------|--------|
| `curl POST /api/pulse/config {"agent":"codex"}` | 3 config options, 6 models (gpt-5.3-codex, gpt-5.4, etc.) |
| `curl POST /api/pulse/config {"agent":"claude"}` | 2 config options, 3 models (Default, Opus, Haiku) |
| `echo ... \| /usr/local/bin/claude-agent-acp` | Adapter responds to initialize (protocol v1), rejects string protocolVersion |
| `node WS test (codex)` | Full probe: scaffold→spawn→initialize→new_session→configOptions→done (3.5s) |
| `node WS test (claude)` | Full probe: scaffold→spawn→initialize→new_session→configOptions→done (7.4s) |
| `pnpm test` | 647/647 passing |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Claude model selector | Shows only "Default" | Shows Default (recommended), Opus, Haiku |
| Codex model selector | Shows only "Default" | Shows 6 real models (gpt-5.3-codex through gpt-5.1-codex-mini) |
| Initial model on page load (Claude) | `default` | `opus` (adapter's currentValue) |
| Initial model on page load (Codex) | `default` | `gpt-5.4` (adapter's currentValue) |
| Chat session config updates | Synced to UI | Still synced to UI (guard passes when length > 0) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `curl POST /api/pulse/config {"agent":"claude"}` | Models returned | 3 models (Default, Opus, Haiku) | PASS |
| `curl POST /api/pulse/config {"agent":"codex"}` | Models returned | 6 models | PASS |
| Chrome DevTools snapshot (Claude) | `opus` in toolbar | `"Pulse tools · claude · opus · accept-edits"` | PASS |
| Chrome DevTools snapshot (Codex) | Real model in toolbar | `"Pulse tools · codex · gpt-5.4 · accept-edits"` | PASS |
| Chrome DevTools model dropdown (Claude) | 3 options | Default (recommended), Opus, Haiku | PASS |
| Chrome DevTools model dropdown (Codex) | 6 options | All 6 GPT models listed | PASS |
| `pnpm test` | 647 pass | 647 pass | PASS |

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations during debugging.

## Risks and Rollback

- **Very low risk**: One-line guard addition, no behavioral change when chat IS active
- **Rollback**: `git checkout -- apps/web/hooks/use-pulse-workspace.ts`
- **Edge case**: If chat hook somehow receives config options before probe completes, chat options take precedence. This is correct behavior since chat options are more current.

## Decisions Not Taken

- **Did not add timeout to `run_acp_event_loop`** — would prevent stuck probes from consuming threads forever, but requires careful design (what timeout value? how to cancel the spawned adapter?)
- **Did not upgrade `agent-client-protocol` to v0.10.0** — v0.9.5 with schema v0.10.8 works correctly; upgrade risks breaking changes
- **Did not fix `axon-postgres` DNS resolution error in jobs route** — separate issue (Next.js using Docker hostname instead of localhost), not related to model selection

## Open Questions

- **Should `run_acp_event_loop` have a timeout?** Stuck probes consume a `spawn_blocking` thread forever. A 30-60s timeout with child process kill would prevent this.
- **Why did `claude-agent-acp` processes from earlier probes hang?** The processes were from 06:59 AM but the current serve process started at 18:02 — they may have been from a different serve instance.
- **Postgres DNS in Next.js** — Jobs route resolves `axon-postgres` (Docker hostname) instead of `127.0.0.1:53432`. The `.env.local` may need `AXON_PG_URL` override for local dev.

## Next Steps

- Consider adding timeout to `run_acp_event_loop` to prevent thread exhaustion from stuck probes
- Monitor for stuck `claude-agent-acp` processes — may need a process cleanup mechanism
- Fix `axon-postgres` DNS resolution in Next.js jobs route for local dev
- Test actual chat responses from both agents (model selection is fixed, chat flow needs verification)
