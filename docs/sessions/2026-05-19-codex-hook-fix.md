---
date: 2026-05-19 14:43:42 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 161001d9
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 161001d9 [main]
---

# Codex Plugin Hook Failure Fix

## User Request

The user reported Codex lifecycle hook failures:

- `UserPromptSubmit hook (failed) error: hook exited with code 1`
- `SessionStart hook (failed) error: hook exited with code 1`

The requested goal was to fix these hook failures.

## Session Overview

The session traced the failing hooks to installed Codex plugin hook definitions rather than repo code. The Beads plugin hook invoked a missing `bd codex-hook` subcommand, and the Superpowers plugin hook depended on an unset `CLAUDE_PLUGIN_ROOT`. Both hook definitions were patched under `~/.codex/plugins/cache/...` and verified with direct command reproductions.

## Sequence of Events

1. Loaded the systematic-debugging workflow and checked prior memory for host-level hook failure patterns.
2. Validated the user-level Codex hook config at `/home/jmagar/.codex/hooks.json`; it only contained the already-fixed `Stop` hook.
3. Located active plugin hook state in `/home/jmagar/.codex/config.toml`, pointing at Beads and Superpowers plugin hooks.
4. Reproduced the failing commands directly:
   - `bd codex-hook SessionStart` failed because the installed `bd` had no `codex-hook` command.
   - `"${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd" session-start` failed when `CLAUDE_PLUGIN_ROOT` was unset.
5. Patched the installed plugin hook JSON files and verified the patched commands.

## Key Findings

- `/home/jmagar/.codex/hooks.json` parsed as valid JSON and only defined a `Stop` hook.
- `/home/jmagar/.codex/config.toml` had trusted active hooks for:
  - `beads@labby-marketplace:hooks/hooks.json:session_start:0:0`
  - `beads@labby-marketplace:hooks/hooks.json:user_prompt_submit:0:0`
  - `superpowers@labby-marketplace:hooks/hooks.json:session_start:0:0`
- `bd --version` reported `bd version 1.0.3 (1b2dd2cb)`.
- `bd codex-hook SessionStart` and `bd codex-hook UserPromptSubmit` failed with `unknown command "codex-hook" for "bd"`.
- The Superpowers hook worked when run with an absolute plugin root path.

## Technical Decisions

- The Beads hooks were made non-fatal rather than disabled, preserving hook registration while avoiding lifecycle failure until the installed `bd` and plugin hook contract are aligned.
- Beads hook stderr was redirected to `/tmp/beads-codex-hook.err` so failures remain inspectable without breaking the hook event.
- The Superpowers hook was changed to an absolute `run-hook.cmd` path because Codex did not provide `CLAUDE_PLUGIN_ROOT` in the reproduced command path.
- The unsupported `async` field was removed from the Superpowers hook entry, matching prior Codex hook config behavior.

## Files Modified

- `/home/jmagar/.codex/plugins/cache/labby-marketplace/beads/1.0.4/hooks/hooks.json`: appended `2>/tmp/beads-codex-hook.err || true` to Beads lifecycle hook commands at lines 9, 21, 33, and 44.
- `/home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/hooks.json`: replaced the env-dependent `CLAUDE_PLUGIN_ROOT` command with an absolute `run-hook.cmd` path at line 9 and removed `async`.
- `/home/jmagar/.codex/plugins/cache/labby-marketplace/beads/1.0.4/hooks/hooks.json.bak-20260519T182552Z`: backup created before patching.
- `/home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/hooks.json.bak-20260519T182552Z`: backup created before patching.
- `/tmp/fix-codex-plugin-hooks.py`: one-off patch script used to update host-level plugin hook JSON.
- `docs/sessions/2026-05-19-codex-hook-fix.md`: this session note.

## Commands Executed

- `python3 -m json.tool /home/jmagar/.codex/hooks.json`: confirmed the user-level hook JSON was valid.
- `which bd && bd --version && bd status --json`: confirmed the installed `bd` path/version and showed Dolt connectivity was blocked inside the sandbox.
- `printf ... | bd codex-hook SessionStart`: reproduced the Beads hook failure with `unknown command "codex-hook"`.
- `env -u CLAUDE_PLUGIN_ROOT ... "${CLAUDE_PLUGIN_ROOT}/hooks/run-hook.cmd" session-start`: reproduced the Superpowers empty-root path failure.
- `python3 /tmp/fix-codex-plugin-hooks.py`: patched installed plugin hook JSON files after approval.
- `python3 -m json.tool .../hooks.json`: validated both patched hook files.
- `codex doctor`: checked broader Codex state; it reported unrelated sandbox/network reachability warnings.

## Errors Encountered

- `bd codex-hook ...` failed because the installed `bd` binary did not include the `codex-hook` subcommand expected by the Beads plugin hook file.
- The Superpowers hook failed as `/hooks/run-hook.cmd: not found` when `CLAUDE_PLUGIN_ROOT` expanded to an empty string.
- A broad `rg` scan across `~/.claude` and `~/.codex` produced excessive cached-session noise; the useful evidence came from the active hook config and exact command reproduction.

## Behavior Changes (Before/After)

| Before | After |
| --- | --- |
| Beads `SessionStart` and `UserPromptSubmit` hooks exited non-zero when `bd codex-hook ...` was invoked. | Beads hook commands exit `0` even when the current `bd` lacks `codex-hook`, with stderr captured in `/tmp/beads-codex-hook.err`. |
| Superpowers `SessionStart` depended on `CLAUDE_PLUGIN_ROOT` being set. | Superpowers `SessionStart` runs through the absolute cached plugin path. |
| The Superpowers hook JSON included `async: false`. | The unsupported `async` field was removed. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `python3 -m json.tool /home/jmagar/.codex/plugins/cache/labby-marketplace/beads/1.0.4/hooks/hooks.json` | Valid JSON | Parsed and printed formatted JSON | pass |
| `python3 -m json.tool /home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/hooks.json` | Valid JSON | Parsed and printed formatted JSON | pass |
| `printf ... | sh -c 'bd codex-hook SessionStart 2>/tmp/beads-codex-hook.err || true'` | Exit 0 | `exit=0` | pass |
| `printf ... | sh -c 'bd codex-hook UserPromptSubmit 2>/tmp/beads-codex-hook.err || true'` | Exit 0 | `exit=0 stdout_bytes=0` | pass |
| `"/home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/run-hook.cmd" session-start` | Exit 0 and valid JSON | `exit=0`, `json=ok` | pass |

## Risks and Rollback

- Risk: The Beads hook no longer injects Beads context until a `bd` binary with `codex-hook` support is installed or the plugin hook is updated to a supported command.
- Rollback: restore the backup files:
  - `/home/jmagar/.codex/plugins/cache/labby-marketplace/beads/1.0.4/hooks/hooks.json.bak-20260519T182552Z`
  - `/home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/hooks.json.bak-20260519T182552Z`
- Risk: Plugin cache updates may overwrite these local hook patches.

## Decisions Not Taken

- Did not edit repo code because the failures were in host-level Codex plugin hook configuration.
- Did not disable the Beads plugin or Beads hooks completely because the non-fatal command preserves future compatibility if `bd codex-hook` becomes available.
- Did not attempt a `bd` upgrade because the immediate request was to stop hook failures, and network/runtime state was restricted.

## References

- `/home/jmagar/.codex/hooks.json`
- `/home/jmagar/.codex/config.toml`
- `/home/jmagar/.codex/plugins/cache/labby-marketplace/beads/1.0.4/hooks/hooks.json`
- `/home/jmagar/.codex/plugins/cache/labby-marketplace/superpowers/5.1.0/hooks/hooks.json`
- Prior memory entry: host-level Codex hook runtime failures and writable-log fixes.

## Open Questions

- Which `bd` release introduced `bd codex-hook`, and should the local `bd` binary be upgraded instead of keeping the temporary non-fatal wrapper?
- Will the Lab marketplace plugin cache overwrite the local patched hook files on the next plugin refresh?
- Does Codex intentionally omit `CLAUDE_PLUGIN_ROOT` for plugin hook execution, or is this specific to the reproduced sandbox context?

## Next Steps

Unfinished work from this session:

- None for the reported hook failures; direct hook command verification passed.

Follow-on tasks not yet started:

- Upgrade or patch Beads so its hook uses a supported installed command and can inject context again.
- Reopen a fresh Codex session to confirm the UI no longer reports `SessionStart` or `UserPromptSubmit` hook failures.
