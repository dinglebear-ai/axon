# Chat + Tool Calling Render Audit (apps/web)
Date: 2026-03-12
Environment: production `https://axon.tootie.tv` + source review in `apps/web`
Reviewer: Codex

## Scope
Audit of conversation rendering for:
- agent messages
- thinking/chain-of-thought
- tool calls (terminal, MCP, skills, file ops)
- ordering, persistence, and replay traceability
- badge/icon/chip semantics
- density, clarity, and visual quality

## Method
1. Runtime inspection with Chrome DevTools on `https://axon.tootie.tv`.
2. Session list and session payload inspection via network panel (`/api/sessions/*`).
3. Source audit for event ingestion and message rendering:
- `apps/web/components/reboot/axon-message-list.tsx`
- `apps/web/components/ai-elements/tool.tsx`
- `apps/web/hooks/use-axon-acp.ts`
- `apps/web/hooks/pulse-chat-helpers.ts`
- `apps/web/components/reboot/live-message-sync.ts`
- `apps/web/hooks/use-axon-session.ts`
- `apps/web/lib/sessions/claude-jsonl-parser.ts`
- `apps/web/lib/sessions/codex-jsonl-parser.ts`
- `apps/web/components/ai-elements/chain-of-thought.tsx`

## Runtime Evidence (axon.tootie.tv)
- App loaded and session list populated.
- Multiple sessions render only user turns or minimal assistant output.
- Example API response with both roles present:
  - `/api/sessions/4a6b08d481fd` returns user + assistant (`"Not logged in · Please run /login"`).
- Example API response with only user messages (large Codex transcript):
  - `/api/sessions/5e57fa05359e` response body is dominated by user content and lacks replayed tool/thinking structures.
- Screenshot captured:
  - `docs/reports/2026-03-12-axon-tootie-chat-audit.png`

## Findings

### F1 (High): Tool icon semantics are incorrect (terminal icon for everything)
Tool header always renders `TerminalSquareIcon`, regardless of tool class.
- File: `apps/web/components/ai-elements/tool.tsx`
- Impact: MCP tools, skills, file operations, shell commands all look identical; users cannot distinguish action type at a glance.

### F2 (High): Session replay is lossy for tool/thinking traceability
Historical sessions are reconstructed from parsers that keep only `{role, content}`.
- Files:
  - `apps/web/lib/sessions/claude-jsonl-parser.ts`
  - `apps/web/lib/sessions/codex-jsonl-parser.ts`
  - `apps/web/hooks/use-axon-session.ts`
- Impact:
  - Tool-use cards, statuses, and outputs are not reliably recoverable after reload.
  - Chain-of-thought/tool event chronology cannot be fully audited post hoc.

### F3 (High): Live/historical merge is fragile
Merge logic requires index + exact role/content alignment.
- File: `apps/web/components/reboot/live-message-sync.ts`
- Impact: if content normalization/drift occurs, rich live metadata can be dropped during sync.

### F4 (Medium): Tool cards have low information density for operations work
Current tool cards show name + status + raw JSON input/output only.
- File: `apps/web/components/reboot/axon-message-list.tsx`
- Missing fields:
  - tool class (terminal/mcp/skill/fs)
  - namespace/server (`mcp__server__tool` decomposition)
  - start/end/duration
  - sequence index in turn
  - compact status progression

### F5 (Medium): Chain-of-thought presentation is visually clean but semantically generic
CoT UI exists and is collapsible, but steps are mostly inferred from chunked text.
- Files:
  - `apps/web/components/reboot/axon-message-list.tsx`
  - `apps/web/components/ai-elements/chain-of-thought.tsx`
- Impact: hard to distinguish substantive reasoning milestones from streamed fragments.

### F6 (Medium): Conversation replay quality appears inconsistent across session types
Observed sessions where conversation history is heavily skewed toward user content.
- Runtime evidence: `/api/sessions/5e57fa05359e`
- Likely contributor: parser assumptions per provider format do not preserve full structured events.

### F7 (Low): Minor UX quality noise remains in production
Console issues observed:
- form fields missing `id`/`name`
- permissions-policy warning for `interest-cohort`
- non-blocking; but polish debt in a high-end UI.

## Answers to Key Product Questions
- Is everything in order? No.
- Are badges/icons/chips correct for current actions? No, tool icon semantics are incorrect and too coarse.
- Can you trace exact tool call order in replayed conversations? Not reliably after reload.
- Is chain-of-thought properly used for all 3 agents? Pipeline exists, but rendered semantics are too generic and persistence is weak.
- Are all agent messages preserved and ordered? Not consistently across session replay paths.

## UX/IA Assessment

### What is good
- Clean visual baseline and modern component styling.
- Live rendering pipeline supports streaming deltas and tool updates.
- CoT is collapsible and does not overwhelm default view.

### What is bad
- Tool identity collapse (everything looks like terminal usage).
- Event replay model is text-centric, not operation-centric.
- Hard to audit “what happened, in what order, and why.”

### What is worse
- For operational/debug use, historical transcript cannot be trusted as a faithful execution trace.

### Does it look vibe coded?
- Partially. Surface polish is strong, but semantics and observability density lag behind operator needs.

### Would I use it?
- For lightweight chat: yes.
- For serious agent execution review/debugging: not yet, until tool/event trace fidelity improves.

## Recommended Design Direction

### 1. Replace single tool icon with typed badge system (highest ROI)
Derive `toolKind` from tool name/namespace and render distinct icon+chip:
- `terminal`
- `mcp`
- `skill`
- `file_read`
- `file_write`
- `search` / `web`

### 2. Introduce compact event timeline rows per assistant turn
Format:
`[time] [kind chip] [tool chip] [status chip] [summary] [duration]`
- Keep JSON payload collapsed by default.
- Preserve readable at-a-glance chronology.

### 3. Persist structured blocks in session API contract
Store/replay:
- `blocks[]` (`text|thinking|tool_use`)
- `toolUses[]` (id, status, content chunks)
- CoT steps
This is prerequisite for trustworthy replay.

### 4. Stabilize live-historical reconciliation
Use stable IDs (message id / event id / tool call id), not index+content matching.

### 5. Improve density without clutter
- Tighten card paddings.
- Inline compact metadata row above expanded details.
- Default collapsed for completed tools; auto-open active/errored tools.

## Build-Upon Areas
- Existing `Tool`, `ChainOfThought`, and stream handler architecture are good foundations.
- Current visual language can carry richer semantics with modest component extension.

## Suggested Implementation Order
1. Typed tool icon/chip mapping in `ToolHeader` + message list.
2. Structured replay contract (API + parsers + session hook).
3. Stable event-ID-based merge strategy.
4. Compact timeline UX pass.
5. CoT semantic enrichment and cross-agent consistency checks.

## Resolution Status (Implemented)

### R1: Typed tool badges/icons (F1) — Addressed
- Implemented tool-kind inference + chip/icon rendering:
  - `MCP`, `TERMINAL`, `FILE`, `SKILL`, `SEARCH`, fallback `TOOL`
- File:
  - `apps/web/components/ai-elements/tool.tsx`
- Tests:
  - `apps/web/__tests__/tool-kind.test.tsx` (7 passing)
- DevTools evidence:
  - Replay row labels now include `mcp__chrome-dev-tools__click MCP Running` and
    `exec_command TERMINAL Running`.

### R2: Replay fidelity for tools/thinking (F2/F6) — Addressed
- Extended session parsing to preserve structured metadata:
  - Claude parser now captures `tool_use` + `thinking` blocks.
  - Codex parser now handles:
    - legacy `type:\"message\"` records
    - modern `response_item` messages
    - `function_call` + `function_call_output` as replayable tool entries
    - reasoning summaries into chain-of-thought blocks.
- Files:
  - `apps/web/lib/sessions/claude-jsonl-parser.ts`
  - `apps/web/lib/sessions/codex-jsonl-parser.ts`
  - `apps/web/hooks/use-axon-session.ts`
- Tests:
  - `apps/web/__tests__/sessions/parser.test.ts`
  - `apps/web/__tests__/sessions/codex-parser.test.ts`
  - all passing in targeted run.
- DevTools evidence:
  - Session replay now renders assistant text + tool cards + CoT for synthetic replay session.

### R3: Historical/live merge robustness (F3) — Addressed
- Improved merge to preserve metadata when content differs by whitespace normalization
  and to fallback-match by role/content semantic equivalence instead of strict index-only equality.
- File:
  - `apps/web/components/reboot/live-message-sync.ts`
- Tests:
  - `apps/web/__tests__/live-message-sync.test.ts` includes new metadata-preservation case.

### R4: Information density improvements (F4/F5 partial) — Addressed
- Tool rows now carry compact semantic chips in-header (higher scan density).
- CoT/Tool replay metadata now persists from session parsers, improving usefulness of
  historical transcript review.
- Note:
  - Full timeline compaction and duration telemetry chips remain a future enhancement.

### R5: Form field warnings (F7) — Addressed
- Added explicit `id/name` defaults for prompt textarea and composer inputs.
- Files:
  - `apps/web/components/ai-elements/prompt-input.tsx`
  - `apps/web/components/reboot/axon-prompt-composer.tsx`
- DevTools evidence:
  - On reload, console warning about unnamed form fields no longer appears in the
    observed page; only non-blocking PWA info remains.
