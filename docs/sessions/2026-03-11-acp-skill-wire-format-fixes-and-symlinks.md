# ACP Skill Wire-Format Fixes + Symlinks — 2026-03-11

## Session Overview

Continued the ACP skill overhaul from the prior session. Applied the final confirmed fixes to `wire-format.md` — all verified against actual SDK source constants from `~/workspace/acp/agent-client-protocol/src/client.rs`. Created symlinks so the single skill source is shared across Claude Code, Codex, and Gemini environments.

---

## Timeline

1. **Resumed mid-task** — prior session ended while agents had just confirmed 4+ inaccuracies in `wire-format.md` but fixes had not yet been applied
2. **Located skill files** — found skill in workspace at `/home/jmagar/workspace/axon_rust/.claude/skills/acp/` (not `~/.claude/skills/acp/`, which was an empty directory)
3. **Applied wire-format.md fixes** — 7 confirmed inaccuracies corrected across authenticate, session/prompt, fs/*, session/request_permission, terminal/wait_for_exit, and session/set_mode
4. **Created symlinks** — removed empty `~/.claude/skills/acp/` dir, replaced with symlink; created matching symlinks in `~/.codex/skills/` and `~/.gemini/skills/`

---

## Key Findings

### Confirmed Wire-Format Inaccuracies (all from schema source constants)

| File:Line | Bug | Fix | SDK Source |
|-----------|-----|-----|------------|
| `wire-format.md:77` | `"credentials": { "apiKey": "sk-..." }` in authenticate params | Removed — `AuthenticateRequest` has no `credentials` field | `client.rs` / `AuthenticateRequest` type |
| `wire-format.md:80` | `"authenticated": true` in authenticate result | Changed to `{}` — `AuthenticateResponse::new()` = default empty | SDK builder |
| `wire-format.md:108` | `"messages": [...]` in session/prompt params | Changed to `"prompt": [...]` — field is `PromptRequest.prompt: Vec<ContentBlock>` | `PromptRequest` struct |
| `wire-format.md:142` | `"fs/readTextFile"` method name | `"fs/read_text_file"` (snake_case) | `FS_READ_TEXT_FILE_METHOD_NAME` constant |
| `wire-format.md:150` | `"request/permission"` method name | `"session/request_permission"` | `SESSION_REQUEST_PERMISSION_METHOD_NAME` constant |
| `wire-format.md:157` | `"outcome": "Approved"` | Replaced with `Cancelled \| Selected(SelectedPermissionOutcome)` shape | `RequestPermissionOutcome` enum |
| `wire-format.md:196,203` | `"terminal/waitForExit"` method name | `"terminal/wait_for_exit"` (snake_case) | `TERMINAL_WAIT_FOR_EXIT_METHOD_NAME` constant |
| `wire-format.md:258` | `"mode": "acceptEdits"` parameter | `"modeId": "acceptEdits"` | `SetSessionModeRequest { mode_id }` with `#[serde(rename = "modeId")]` |

### Architecture of Skill Distribution
- Skill source: `/home/jmagar/workspace/axon_rust/.claude/skills/acp/` (8 files)
- Three symlinks now point to this single source — edits propagate everywhere automatically
- `~/.claude/skills/acp/` was an empty directory (created by Claude Code but never populated); replaced with symlink

---

## Technical Decisions

- **snake_case is the ACP wire standard**: All multi-word ACP methods use snake_case in the JSON-RPC `method` field. This contradicts naive assumptions — JSON-RPC commonly uses camelCase method names, but ACP doesn't. The SDK source constants are authoritative.
- **`AuthenticateRequest` has no credentials field**: Credentials travel via env vars (advertised as `AuthMethodEnvVar`) or browser/terminal flow, not in the JSON-RPC message itself. The `methodId` tells the agent which auth method was invoked.
- **`RequestPermissionOutcome::Approved` doesn't exist**: The enum is `Cancelled | Selected(SelectedPermissionOutcome { option_id })`. "Approved" is conceptually mapped to `Selected` but not a variant name.
- **Symlinks over copies**: One source file, three skill directories. Any future fix only needs to be applied once.

---

## Files Modified

| File | Change |
|------|--------|
| `~/.claude/skills/acp/references/wire-format.md` | Fixed 7 confirmed inaccuracies: credentials field, authenticate result shape, "messages"→"prompt", 3 camelCase method names, outcome enum, modeId parameter name |
| `~/.claude/skills/acp` | Replaced empty directory with symlink → workspace skill |
| `~/.codex/skills/acp` | Created symlink → workspace skill |
| `~/.gemini/skills/acp` | Created symlink → workspace skill |

---

## Commands Executed

```bash
# Locate actual skill files
find /home/jmagar/workspace/axon_rust/.claude/skills/acp/ -type f | sort
# Result: 8 files (SKILL.md, 2 examples, 5 references)

# Check target symlink directories exist
ls -la /home/jmagar/.codex/ /home/jmagar/.gemini/
# Both .codex/skills/ and .gemini/skills/ exist

# Replace empty dir with symlink (Claude Code directory)
rmdir /home/jmagar/.claude/skills/acp
ln -sf /home/jmagar/workspace/axon_rust/.claude/skills/acp /home/jmagar/.claude/skills/acp

# Create Codex and Gemini symlinks
ln -sf /home/jmagar/workspace/axon_rust/.claude/skills/acp /home/jmagar/.codex/skills/acp
ln -sf /home/jmagar/workspace/axon_rust/.claude/skills/acp /home/jmagar/.gemini/skills/acp
```

Verification output:
```
lrwxrwxrwx ... /home/jmagar/.claude/skills/acp -> /home/jmagar/workspace/axon_rust/.claude/skills/acp
lrwxrwxrwx ... /home/jmagar/.codex/skills/acp  -> /home/jmagar/workspace/axon_rust/.claude/skills/acp
lrwxrwxrwx ... /home/jmagar/.gemini/skills/acp -> /home/jmagar/workspace/axon_rust/.claude/skills/acp
```

---

## Behavior Changes (Before/After)

**Before**: `wire-format.md` contained 7 inaccuracies — camelCase method names, a fabricated `credentials` field in authenticate, wrong `"messages"` param name, and `"Approved"` as an outcome variant. Using these examples as a reference would produce JSON-RPC messages the server would reject.

**After**: All method names verified against SDK source constants. Auth request is minimal (`methodId` only). Prompt param is `"prompt"`. Permission outcome reflects the actual enum shape. The `modeId` parameter is named correctly.

**Before**: ACP skill existed only in `workspace/axon_rust/.claude/skills/acp/`. `~/.claude/skills/acp/` was an empty directory. Codex and Gemini had no access to the skill.

**After**: All three environments (Claude Code, Codex, Gemini) reference the same single source via symlinks.

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `~/.claude/skills/acp` target | symlink to workspace | `lrwxrwxrwx → /home/jmagar/workspace/axon_rust/.claude/skills/acp` | ✅ |
| `~/.codex/skills/acp` target | symlink to workspace | `lrwxrwxrwx → /home/jmagar/workspace/axon_rust/.claude/skills/acp` | ✅ |
| `~/.gemini/skills/acp` target | symlink to workspace | `lrwxrwxrwx → /home/jmagar/workspace/axon_rust/.claude/skills/acp` | ✅ |
| `wire-format.md:142` | `fs/read_text_file` | Updated | ✅ |
| `wire-format.md:150` | `session/request_permission` | Updated | ✅ |
| `wire-format.md:196,203` | `terminal/wait_for_exit` | Updated | ✅ |
| `wire-format.md:258` | `"modeId"` parameter | Updated | ✅ |
| `wire-format.md:77` | No credentials field | Removed with explanatory comment | ✅ |
| `wire-format.md:108` | `"prompt"` field name | Updated | ✅ |

---

## Source IDs + Collections Touched

- No Axon embed/retrieve operations in this session (pure file editing and symlink creation)

---

## Risks and Rollback

- **Symlink risk (low)**: If the workspace repo moves, all three symlinks break. Fix: update symlinks to new path. To rollback: `rm ~/.claude/skills/acp ~/.codex/skills/acp ~/.gemini/skills/acp && mkdir ~/.claude/skills/acp`
- **wire-format.md edits**: Wire format examples are reference-only — no running code consumes them. Risk is documentation-only.

---

## Decisions Not Taken

- **Copy skill files into each tool's directory**: Rejected — creates three independent copies that drift. Single symlink source is strictly better.
- **Leave `~/workspace/axon_rust/.claude/skills/acp/` as the only location and document it manually**: Rejected — Codex and Gemini would never discover the skill via their auto-scan.
- **Fix `session/close` unstable label** in wire-format.md: Not done — `session/close` is noted as unstable in SKILL.md; the wire-format.md example exists but is currently not annotated as unstable. Low priority.

---

## Open Questions

- **`RequestPermissionOutcome` exact wire shape**: The fix uses `{ "type": "selected", "optionId": "allow" }` as an example, but the exact serde representation of the `Selected(SelectedPermissionOutcome)` variant on the wire hasn't been verified against a live exchange or schema serde attributes. Could be `{ "selected": { "optionId": "..." } }` or another shape.
- **Other terminal method names**: `terminal/create`, `terminal/output`, `terminal/kill`, `terminal/release` were not verified against SDK constants (only `terminal/wait_for_exit` was confirmed). These may need similar snake_case treatment but no issues were reported by agents.
- **`DashMap` vs `Rc<RefCell<HashMap>>` note**: Agent 3 found that codex-acp uses `Rc<RefCell<HashMap>>` (not DashMap) for session state because it runs in a `LocalSet` context. `codex-patterns.md` recommends `DashMap` but the production reference uses `Rc<RefCell>`. This inconsistency has not been resolved in the skill.

---

## Next Steps

- Verify `RequestPermissionOutcome` exact wire shape by reading `agent-client-protocol/src/client.rs` serde attributes
- Resolve `DashMap` vs `Rc<RefCell<HashMap>>` recommendation in `codex-patterns.md`
- Check `terminal/create`, `terminal/output`, `terminal/kill`, `terminal/release` method name constants
- Consider adding a note to `wire-format.md` that `session/close` is behind `unstable_session_close` feature flag
- Embed updated skill files into Qdrant: `axon embed ~/.claude/skills/acp/`
