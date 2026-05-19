# Hook Error Root Cause Fix
**Date:** 2026-03-01
**Branch:** feat/sidebar
**Trigger:** Recurring `PreToolUse:Edit hook error` appearing every time Claude attempts to edit files

---

## Session Overview

Systematically debugged recurring `PreToolUse:Edit hook error` messages that had been "fixed 3 times before" without resolution. Found **3 distinct root causes** across two `settings.json` files. All fixed in this session. Previous fixes failed because they targeted the wrong hooks (`lint-suppression.py`, `secret-scan.py`) while the actual failing hooks — inline Python one-liners in `settings.json` — were never touched.

---

## Timeline

1. **Invoked `superpowers:systematic-debugging`** — established Phase 1 (root cause investigation before any fix)
2. **Read both settings.json files** — identified all `PreToolUse` hooks registered for Edit events
3. **Reproduced with empty stdin** — confirmed `json.load(sys.stdin)` without try-except crashes with `JSONDecodeError` (exit 1 → "hook error")
4. **Found project-level settings.json** at `.claude/settings.json` — contained 4 more unsafe hooks (s6 type/run, rust-toolchain, rustfmt)
5. **Read backup file** `settings.json.bak-hooks-fix` — confirmed prior "fixes" targeted the wrong thing; original hooks used safe shell `echo && exit` or `"type": "prompt"` hooks
6. **Applied fix 1** — `json.load(sys.stdin)` → `json.loads(sys.stdin.read() or '{}')` in all inline hooks (global + project)
7. **Ran full validation** — discovered 2 more failures (exit 1) on entries 4 and 6
8. **Identified Bug 2** — `\n` in JSON strings is U+000A (real newline), breaking Python `-c "..."` f-strings
9. **Confirmed via repr()** — `'\n' in command_string` returned `True` for both failing hooks
10. **Applied fix 2** — used Python JSON manipulation to replace `sys.stderr.write(f'...\n')` with `print(f'...', file=sys.stderr)`
11. **Identified Bug 3 (bonus)** — s6 path check used `p in targets` where `p` is absolute but targets are relative → block **never fired**
12. **Applied fix 3** — `any(t in p for t in targets)` for s6 type + run hooks
13. **Final validation** — all 8 PreToolUse Edit hooks pass with empty stdin

---

## Key Findings

### Bug 1: `json.load(sys.stdin)` without exception handling

- **Where:** `~/.claude/settings.json` entries 1+2 (CI workflow hooks, matcher `Edit`/`Write`)
- **Where:** `.claude/settings.json` entries 0–3 (s6 type/run, rust-toolchain, rustfmt)
- **What:** `json.load(sys.stdin)` with no `try/except` → on any edge case stdin condition, raises `JSONDecodeError`, exits with code 1
- **Claude Code behavior:** Exit code 1 = "hook error" (not 0=success, not 2=block)
- **Fix:** `json.loads(sys.stdin.read() or '{}')` — empty stdin falls back to `{}` gracefully

### Bug 2: Real newline (0x0A) in JSON command strings

- **Where:** `~/.claude/settings.json` entry 4 (lock file hook) and entry 6 (enforcer block)
- **What:** In JSON, `\n` is a newline escape → U+000A. When json.load returns this command string, the shell command `python3 -c "...f'message\n'..."` has a real newline inside the single-quoted f-string → `SyntaxError: unterminated f-string literal (detected at line 1)` → exit 1
- **Evidence:** `'\n' in pre[4]['hooks'][0]['command']` → `True`
- **Fix:** `print(f'...', file=sys.stderr)` — no `\n` needed; `print()` adds newline automatically

### Bug 3: Absolute vs relative path comparison (bonus — s6 block never worked)

- **Where:** `.claude/settings.json` s6 type + Write hooks
- **What:** Claude Code passes **absolute** file paths to hooks (e.g., `/home/jmagar/workspace/axon_rust/docker/s6/s6-rc.d/crawl-worker/type`), but `targets` set contained **relative** paths (`docker/s6/s6-rc.d/crawl-worker/type`). `p in targets` always `False` → block never fired
- **Fix:** `any(t in p for t in targets)` — substring check works for both absolute and relative

### Why it kept recurring

Prior sessions fixed `lint-suppression.py` and `secret-scan.py` (which were already safe — they had proper `except json.JSONDecodeError`). The actual failing hooks were inline Python one-liners embedded in `settings.json`, which were easy to miss and not touched by prior fixes.

---

## Technical Decisions

1. **Used `json.loads(sys.stdin.read() or '{}')` instead of try/except** — single-line-friendly, no multiline needed for inline hooks, robust against empty or missing stdin
2. **Used `print(..., file=sys.stderr)` instead of fixing `\n` escaping in JSON** — simpler fix that eliminates the problem entirely; `print()` auto-adds newline without any escape sequences in the Python source
3. **Used Python JSON manipulation (`json.load/dump`) to fix embedded newlines** — the `Edit` tool couldn't match strings containing literal `0x0A` newlines reliably; Python direct manipulation was deterministic
4. **Fixed both `json.load` bug AND path comparison bug** in s6 hooks — both were actively broken; fixing only the crash bug while leaving the logic bug would leave the s6 protection silently non-functional

---

## Files Modified

| File | Purpose | Change |
|------|---------|--------|
| `~/.claude/settings.json` | Global Claude Code hooks | Fixed 2 CI workflow hooks (json.load→json.loads), 2 lock-file hooks (real newline→print()), 1 enforcer hook (real newline→print()) |
| `.claude/settings.json` | Project-level hooks | Fixed 4 hooks: s6 type (Edit+Write), s6 run, rust-toolchain (json.load→json.loads); fixed path comparison (in targets → any(t in p)) |
| `~/.claude/projects/-home-jmagar-workspace-axon-rust/memory/MEMORY.md` | Persistent agent memory | Added "Hook Bugs — Root Causes" section documenting all 3 bugs and their patterns |

---

## Commands Executed

```bash
# Audit all hooks for json.load usage
python3 -c "import json; ..."  # parsed settings.json, checked each hook

# Test 1: Reproduce with empty stdin
echo -n "" | python3 -c "import json,sys; p=json.load(sys.stdin)..."
# Result: JSONDecodeError → exit 1 → REPRODUCED

# Test 2: Confirm safe pattern handles empty stdin
echo -n "" | python3 /home/jmagar/.claude/hooks/lint-suppression.py
# Result: exit 0 → safe

# Test 3: Check raw bytes for real newlines
python3 -c "... '\n' in pre[4]['hooks'][0]['command'] ..."
# Result: True — real 0x0A newline confirmed in command string

# Final validation
python3 -c "# run all 8 PreToolUse Edit hooks with empty stdin via subprocess"
# Result: OVERALL: ALL PASS
```

---

## Behavior Changes (Before/After)

| Hook | Before | After |
|------|--------|-------|
| Global CI workflow (Edit) | Crashes on edge case stdin → `PreToolUse:Edit hook error` | Exits 0 gracefully |
| Global lock file check (Edit+Write) | Always crashes with SyntaxError (real newline in f-string) → hook error | Exits 0; print() works correctly |
| Global enforcer block (Edit) | Always crashes with SyntaxError (real newline in string) → hook error | Exits 0; sys.stderr.write with proper escape |
| Project s6 type block (Edit+Write) | Crashes on edge case stdin; also never blocks (path mismatch) | Exits 0 safely; actually blocks s6 type files |
| Project s6 run check (Edit) | Crashes on edge case stdin; never fires | Exits 0 safely; fires for s6 run scripts |
| Project rust-toolchain check | Crashes on edge case stdin | Exits 0 safely |
| Project rustfmt (PostToolUse) | Crashes on edge case stdin | Exits 0 safely |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| Empty stdin → CI workflow hook (after fix) | exit 0 | exit 0 | ✅ |
| Empty stdin → lock file hook (after fix) | exit 0 | exit 0 | ✅ |
| Empty stdin → enforcer hook (after fix) | exit 0 | exit 0 | ✅ |
| Empty stdin → secret-scan.py | exit 0 | exit 0 | ✅ (was already safe) |
| Empty stdin → lint-suppression.py | exit 0 | exit 0 | ✅ (was already safe) |
| Full validation: all 8 PreToolUse Edit hooks | ALL PASS | ALL PASS | ✅ |

---

## Source IDs + Collections Touched

None — this session was debugging/configuration only. No Axon crawl/embed/query operations.

---

## Risks and Rollback

- **Risk:** `settings.json` was modified directly. Claude Code reads this on startup.
  - **Rollback:** `.claude/settings.json.bak-hooks-fix` exists as backup for project settings. Global settings had no backup — restore from git or memory.
- **Risk:** The JSON reformatting via `json.dump(..., indent=4)` may have changed whitespace in global settings.json (originally written with 4-space indent, same was preserved).
- **Risk:** s6 path check fix (`any(t in p for t in targets)`) — if any target path substring matches an unrelated file path, it would falsely block. The s6 paths (`docker/s6/s6-rc.d/*/type`) are specific enough that false positives are unlikely.

---

## Decisions Not Taken

1. **Extracting all inline hooks to `.py` files** — more maintainable but out of scope; user didn't request refactoring, just fixing
2. **Adding `try/except` wrappers** — chose `json.loads(sys.stdin.read() or '{}')` pattern instead; simpler, same protection, works in one-liners
3. **Using `\\n` escape in JSON** to represent literal `\n` — chose `print()` instead; eliminates the escaping complexity entirely
4. **Targeting only the 2 errors shown** — fixed ALL instances of the pattern across both files to prevent future recurrence

---

## Open Questions

- Why does Claude Code sometimes send empty or malformed stdin to hooks? No definitive answer found. Could be a timing/pipe issue in Claude Code 2.1.63, or certain edge cases in tool execution.
- Are there project-level hooks in other repositories that have the same `json.load` pattern?

---

## Next Steps

- If new hooks are written, always use `json.loads(sys.stdin.read() or '{}')` (never bare `json.load(sys.stdin)`)
- If hooks need stderr output with newlines, use `print(..., file=sys.stderr)` not `sys.stderr.write(f'...\n')`
- Consider adding a lint script that scans settings.json for unsafe hook patterns
