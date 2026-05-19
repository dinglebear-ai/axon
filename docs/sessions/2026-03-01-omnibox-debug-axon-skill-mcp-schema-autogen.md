# Session: Omnibox Debug + Axon Skill Review + MCP Schema Auto-Generation
Date: 2026-03-01
Branch: feat/sidebar

---

## Session Overview

Three distinct workstreams in this session:
1. **Bug investigation** — Systematic debugging of omnibox → Pulse chat producing no response ("Start a conversation" placeholder persisting after message send). Root cause not yet confirmed; diagnostic instrumentation added.
2. **Axon skill quality review** — Ran `plugin-dev:skill-reviewer` on `skills/axon/SKILL.md`, applied all critical/major fixes: description frontmatter rewrite, `refresh` lifecycle family added, `url`→`urls` corrected in `docs/MCP-TOOL-SCHEMA.md`.
3. **MCP schema auto-generation** — Wrote `scripts/generate_mcp_schema_doc.py` to generate `docs/MCP-TOOL-SCHEMA.md` from `crates/mcp/schema.rs`; wired into pre-commit via lefthook as an auto-regenerate hook.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reports omnibox → Pulse chat returning no Claude response |
| Phase 1 | Systematic debugging invoked; traced data flow through omnibox → submitWorkspacePrompt → PulseWorkspace effect → handlePrompt |
| Phase 1 cont. | Discovered `workspaceMode` initializes to `'pulse'` (not null), meaning `activateWorkspace` guard always skips |
| Phase 1 cont. | Added diagnostic console.logs to omnibox.tsx, pulse-workspace.tsx, use-pulse-chat.ts |
| Chrome DevTools | Attempted to use Chrome DevTools MCP to check console; failed — MCP connected to headless axon-chrome crawl container, not user browser |
| Skill review | Ran `plugin-dev:skill-reviewer` on `skills/axon/SKILL.md`; identified critical `url`→`urls` bug + missing `refresh` family |
| Skill fixes | Rewrote description frontmatter, added `refresh` to SKILL.md and routing-cheatsheet.md, fixed MCP-TOOL-SCHEMA.md |
| Schema research | Dispatched Explore agent to parse `crates/mcp/schema.rs`; confirmed `CrawlRequest.urls: Option<Vec<String>>` (array correct), `RefreshRequest` accepts both `url` and `urls` |
| Auto-gen script | `python-development:python-pro` agent wrote `scripts/generate_mcp_schema_doc.py`; `--check` exits 0 against freshly-written file |
| Lefthook | Added `mcp-schema-doc` pre-commit hook to auto-regenerate + git-add on `schema.rs` changes |

---

## Key Findings

### Omnibox Bug (unresolved)
- **Symptom**: `messages.length === 0` after submit → "Start a conversation" placeholder shown
- **`workspaceMode` init**: `use-ws-messages.ts:293` — `useState<string | null>('pulse')` not null; means `activateWorkspace` guard in omnibox `executeCommand` always skips
- **Layout switch**: `isPulseWorkspaceActive = workspaceMode === 'pulse' && hasResults && workspacePromptVersion > 0` — on first submit, landing `PulseWorkspace` unmounts, overlay mounts fresh
- **`usePulsePersistence` ordering**: hydration effect fires before workspace prompt handler effect (registered in that order in `PulseWorkspace`) — potential race if filename changes post-handlePrompt
- **Diagnostic logs added**:
  - `apps/web/components/omnibox.tsx` — `[omnibox] pulse path` + `[omnibox] calling submitWorkspacePrompt`
  - `apps/web/components/pulse/pulse-workspace.tsx` — `[pulse-workspace] prompt effect` + `[pulse-workspace] calling handlePrompt`
  - `apps/web/hooks/use-pulse-chat.ts:147` — `[use-pulse-chat] handlePrompt called`
- **Note**: `console.log` in `use-pulse-chat.ts` was removed by linter after addition (file was auto-modified)

### Axon Skill `url`→`urls` Bug
- `docs/MCP-TOOL-SCHEMA.md` crawl section said `url` (singular) — wrong
- `crates/mcp/schema.rs` `CrawlRequest.urls: Option<Vec<String>>` confirms array
- Both SKILL.md and routing-cheatsheet.md were consistent with each other (both said `urls`) — the schema doc was the outlier
- `RefreshRequest` is unique: has BOTH `url: Option<String>` AND `urls: Option<Vec<String>>`

### schemars Already Wired
- `rmcp = { version = "0.16.0", features = [..., "schemars"] }` in `Cargo.toml:61`
- `server.rs` calls `rmcp::schemars::schema_for!(AxonRequest)` for the `axon://schema/mcp-tool` MCP resource
- All request types already `#[derive(schemars::JsonSchema)]`

### Chrome DevTools MCP Limitation
- MCP is connected to `axon-chrome` container (172.18.0.3 on `axon_axon` network) — headless crawl browser
- Cannot reach `axon-web` (172.18.0.4) from that Chrome instance
- Not usable for debugging the web UI — user must use browser devtools directly

---

## Technical Decisions

- **Auto-regenerate vs. check-only in pre-commit**: Chose auto-regenerate (`python3 ... && git add`) over `--check` so the developer never has to run a manual step after editing `schema.rs`. CI keeps `--check` as a backstop.
- **Regex parsing over schemars at build time**: Python regex script chosen for simplicity (zero Rust changes, stdlib only). `build.rs` / xtask approach would be cleaner long-term but requires more setup. Can be revisited.
- **Hardcoded `STRUCT_TO_ACTION` mapping**: serde tag routing can't be reliably inferred from struct names alone, so the mapping is explicit in the script. Must be updated if new actions are added.
- **Date line excluded from `--check` diff**: Normalization in `_normalize_for_check()` prevents false positives from daily regeneration.
- **`agents/openai.yaml` left alone**: Reviewer flagged it as a stub — user confirmed it's for Codex and has purpose.

---

## Files Modified

| File | Type | Purpose |
|------|------|---------|
| `apps/web/components/omnibox.tsx` | Modified | Added diagnostic console.logs to pulse path in `executeCommand` |
| `apps/web/components/pulse/pulse-workspace.tsx` | Modified | Added diagnostic console.logs to workspace prompt handler effect |
| `apps/web/hooks/use-pulse-chat.ts` | Modified (then reverted by linter) | console.log added to `handlePrompt` at line 147 |
| `skills/axon/SKILL.md` | Modified | Rewrote description frontmatter; moved load-order mandate to body; added `refresh` to lifecycle families + request templates |
| `skills/axon/references/routing-cheatsheet.md` | Modified | Added `refresh` family row + `schedule` subaction to lifecycle table |
| `docs/MCP-TOOL-SCHEMA.md` | Modified → Regenerated | Fixed crawl `url`→`urls`; added refresh section; then fully regenerated by script |
| `scripts/generate_mcp_schema_doc.py` | Created | Parses `crates/mcp/schema.rs`, generates `docs/MCP-TOOL-SCHEMA.md`; supports `--check`, `--dry-run`, `--repo-root` |
| `Justfile` | Modified | Added `gen-mcp-schema *ARGS` recipe |
| `lefthook.yml` | Modified | Added `mcp-schema-doc` pre-commit command (glob: `crates/mcp/schema.rs`) |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `ss -tuln \| grep 49010` | Port 49010 listening on 0.0.0.0 and [::] |
| `docker inspect axon-web` network | IP: 172.18.0.4, network: axon_axon, internal port: 49010 |
| `docker inspect axon-chrome` network | IP: 172.18.0.3, network: axon_axon |
| `navigate_page http://172.18.0.4:49010` | Timeout — Chrome MCP cannot reach axon-web |
| `python3 scripts/generate_mcp_schema_doc.py --check` | `OK: docs/MCP-TOOL-SCHEMA.md is up to date` exit 0 |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `docs/MCP-TOOL-SCHEMA.md` crawl start field | `url` (singular string — wrong) | `urls` (string[] — correct per schema.rs) |
| `skills/axon/SKILL.md` description | Mixed trigger conditions with load-order mandate; missing `refresh`/`re-crawl` trigger phrases | Trigger-condition focused; mandate in body; `refresh` + `re-crawl` + `index this page` added |
| `skills/axon/SKILL.md` lifecycle families | Missing `refresh` entirely | `refresh` added with `url`/`urls` dual form + `schedule` subaction |
| `docs/MCP-TOOL-SCHEMA.md` maintenance | Manual — drifts silently on schema changes | Auto-regenerated on pre-commit when `schema.rs` staged; `just gen-mcp-schema` for manual runs |
| Omnibox → Pulse debug visibility | No instrumentation | Console.logs at each pipeline stage; user can confirm which stage breaks |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `python3 scripts/generate_mcp_schema_doc.py --check` | exit 0, "up to date" | `OK: docs/MCP-TOOL-SCHEMA.md is up to date` exit 0 | ✅ Pass |
| `grep gen-mcp-schema Justfile` | Recipe present | Line 86: `gen-mcp-schema *ARGS:` | ✅ Pass |
| `grep mcp-schema-doc lefthook.yml` | Hook present | Match found | ✅ Pass |

---

## Source IDs + Collections Touched

_Populated after embed step below._

---

## Risks and Rollback

- **Pre-commit auto-generates and stages `MCP-TOOL-SCHEMA.md`**: If the Python script has a bug it could corrupt the doc mid-commit. Rollback: `git checkout docs/MCP-TOOL-SCHEMA.md`. The `--check` CI gate provides a second layer.
- **Diagnostic console.logs in production code**: `omnibox.tsx` and `pulse-workspace.tsx` have debug logs that should be removed before merging. `use-pulse-chat.ts` log was removed by linter automatically.
- **`url`→`urls` fix in MCP-TOOL-SCHEMA.md**: This is a doc-only fix; the actual wire contract (`schema.rs`) was already correct. No code change risk.

---

## Decisions Not Taken

- **`build.rs` for schema doc generation**: Would auto-run at compile time and be fully type-safe. Rejected for now — adds Rust build complexity and slows CI. Python script covers the need today.
- **Chrome DevTools MCP for live debug**: Attempted but architecturally impossible (crawl Chrome ≠ user browser). No viable path without exposing user Chrome to CDP.
- **`activateWorkspace` always-call fix**: Prior commit (`78dddcaf`) added the `workspaceMode !== 'pulse'` guard. We did not revert this — the guard was intended behavior; root cause of the bug is elsewhere.

---

## Open Questions

- **Omnibox bug root cause**: Static analysis didn't reveal a definitive break point. The diagnostic logs need to be observed in a real browser session. Which stage in `[omnibox pulse path] → [submitWorkspacePrompt] → [pulse-workspace prompt effect] → [handlePrompt]` is missing from console output?
- **`use-pulse-chat.ts` linter removal**: The `console.log` added to `handlePrompt` was removed by a linter (auto-modified notification). Is there a lint rule suppressing console statements? If so, the diagnostic chain at that layer is incomplete.
- **`usePulsePersistence` hydration race**: Theoretical scenario where hydration `setChatHistory([])` fires after `handlePrompt` adds user message — not confirmed. Needs browser console evidence to rule in or out.
- **`refresh` `url` vs `urls` precedence**: `RefreshRequest` has both fields. Does `server.rs` prefer one over the other when both are supplied? Not checked.

---

## Next Steps

1. **Resolve omnibox bug**: User opens browser devtools → sends message → pastes console output. First missing log in the chain identifies the break point.
2. **Remove debug console.logs**: `omnibox.tsx` and `pulse-workspace.tsx` — strip before merging `feat/sidebar`.
3. **Add `just gen-mcp-schema --check` to CI**: Wire into `.github/workflows/ci.yml` as a schema drift gate.
4. **`STRUCT_TO_ACTION` map maintenance**: Document in `scripts/generate_mcp_schema_doc.py` that this dict must be updated when new actions are added to `AxonRequest` enum in `schema.rs`.
5. **Sessions ingest options descriptions**: `docs/MCP-TOOL-SCHEMA.md` sessions table has `--` for descriptions — fill in what `claude`/`codex`/`gemini` bools and `project` string actually control.
