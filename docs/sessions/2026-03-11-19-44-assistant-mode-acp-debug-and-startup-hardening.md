# Session Log — Assistant Mode ACP Debug and Startup Hardening

- Timestamp: 19:44 | 03/11/2026
- Scope: Reboot web UI chat persistence, assistant/session sidebar quality, ACP adapter selection correctness, dev startup reliability.

## What was fixed

1. Chat disappearing / submit failures
- Fixed insecure-origin `crypto.randomUUID()` crash in chat submit path by adding safe ID fallback.
- Files:
  - `apps/web/hooks/use-axon-acp.ts`
  - `apps/web/__tests__/use-axon-acp-editor.test.ts`

2. Hot reload persistence race
- Prevented empty historical sync/cache writes from wiping live messages during reload races.
- Files:
  - `apps/web/components/reboot/axon-shell.tsx`
  - `apps/web/components/reboot/live-message-sync.ts`
  - `apps/web/__tests__/live-message-sync.test.ts`

3. Sidebar session title quality
- Reworked preview extraction to use substantive user prompts, strip system wrappers, and apply adaptive first-vs-latest heuristic.
- Applied across Claude/Codex/Gemini scanners.
- Files:
  - `apps/web/lib/sessions/session-utils.ts`
  - `apps/web/lib/sessions/session-scanner.ts`
  - `apps/web/lib/sessions/codex-scanner.ts`
  - `apps/web/lib/sessions/gemini-scanner.ts`
  - `apps/web/__tests__/sessions/scanner.test.ts`

4. System context message leak in UI
- Stripped `[System context ...]` / `[User message]` wrappers from loaded user messages (inline + newline formats).
- Files:
  - `apps/web/hooks/use-axon-session.ts`
  - `apps/web/__tests__/use-axon-session.test.ts`

5. Assistant sidebar persistence across agents
- Assistant scanner now aggregates assistant-mode sessions from Claude + Codex + Gemini stores.
- Added resilience when Claude assistant directory is absent.
- Files:
  - `apps/web/lib/sessions/assistant-scanner.ts`
  - `apps/web/__tests__/sessions/assistant-scanner.test.ts`

6. ACP adapter fallback correctness
- Enforced no implicit Claude-specific fallback for non-Claude agent selection.
- `params` now seeds global adapter override from `AXON_ACP_ADAPTER_*` only (not Claude-specific vars).
- Added built-in per-agent defaults; env vars are now override-only.
- Files:
  - `crates/web/execute/sync_mode/params.rs`
  - `crates/web/execute/sync_mode/acp_adapter.rs`
  - `.env.example`
  - `docs/DEPLOYMENT.md`
  - `docs/OPERATIONS.md`

7. `just dev` startup hardening
- Added Rust build gate before stack start.
- Changed to build once and run `target/debug/axon` for serve/MCP/workers (reduces cargo lock contention).
- Files:
  - `Justfile`

## Verification run highlights

- Web tests:
  - `pnpm vitest run __tests__/use-axon-session.test.ts __tests__/sessions/assistant-scanner.test.ts __tests__/sessions/scanner.test.ts`
  - `pnpm vitest run __tests__/live-message-sync.test.ts`
  - `pnpm tsc --noEmit`
- Rust tests:
  - `cargo test -p axon web::execute::sync_mode::params -- --nocapture`
  - `cargo test -p axon web::execute::sync_mode::acp_adapter -- --nocapture`

## Notable root-cause findings

- Message submit failures on LAN/non-HTTPS origins were caused by `randomUUID` availability assumptions.
- Assistant wrapper text was being persisted in session files and only partially stripped in UI.
- Assistant-mode visibility issues were compounded by scanner source asymmetry (Claude-only path assumptions).
- `just dev` lock spam was amplified by parallel `cargo run` invocations.

## Follow-up checks to run after restart

1. `just dev` from clean state.
2. In assistant mode, send one turn each on Claude, Codex, Gemini.
3. Confirm all three appear in Assistant sidebar with clean titles.
4. Confirm selected agent failure surfaces as error (no silent fallback).
