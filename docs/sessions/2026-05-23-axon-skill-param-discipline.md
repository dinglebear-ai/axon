---
date: 2026-05-23 22:38:34 EST
repo: git@github.com:jmagar/axon.git
branch: feat/palette-tauri-and-dev-to-body
head: 6dbb46a7
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
---

## User Request

Review the axon skill (`plugins/skills/axon/SKILL.md`) and confirm whether it tells the agent not to use extra params/options/flags unless the user explicitly asked. If not, add it.

## Session Overview

Audited `plugins/skills/axon/SKILL.md`. Confirmed no parameter-discipline rule existed in the skill itself (the equivalent rule lives only in user-global `MEMORY.md` as a CLI-only preference). Added a new "Parameter discipline (hard rule)" section near the top of the skill covering both MCP and CLI surfaces.

## Sequence of Events

1. Located the active axon skill at `/home/jmagar/workspace/axon_rust/plugins/skills/axon/SKILL.md` (other copies under `~/.claude/agents/axon/`, `~/.agents/src/agents/axon/`, and the plugin cache are not the editable source).
2. Read the full SKILL.md (204 lines).
3. Reported finding: no minimal-params rule present; only the global `MEMORY.md` "Never add CLI flags not explicitly requested" preference.
4. With user approval, inserted a "Parameter discipline (hard rule)" section between the intro paragraph and "When to fall back to the CLI".

## Key Findings

- `plugins/skills/axon/SKILL.md` examples liberally show optional params (`max_pages`, `max_depth`, `include_subdomains`, `root_selector`, `exclude_selector`, `since`, `before`, `diagnostics`, `hybrid_search`, `format`, `limit`, `collection`) without telling the agent these are illustrative, not defaults.
- "Choosing parameters — quick guide" table (line ~180) suggests params per situation but does not say "omit by default".
- Global `~/.claude/CLAUDE.md` / `MEMORY.md` has the rule "Never add CLI flags not explicitly requested" but it is (a) CLI-only in wording and (b) not visible to agents invoking the skill without that memory loaded.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `plugins/skills/axon/SKILL.md` | — | Added "Parameter discipline (hard rule)" section covering both MCP and CLI surfaces; enumerates common knobs not to add by default | Edit tool applied, file state confirmed current |

## Beads Activity

No bead activity observed. The change is a small skill-doc edit; no tracker work was required or performed.

## Repository Maintenance

- **Plans**: Not reviewed — out of scope for this single-edit session.
- **Beads**: No bead reads or writes performed; the edit did not relate to any tracked work item.
- **Worktrees/branches**: Inspected via injected context only. `git worktree list` shows the main checkout plus `.worktrees/axon-status-trim` (active branch `feat/axon-status-trim`, has upstream). No cleanup performed — both worktrees have unmerged or in-flight work.
- **Stale docs**: Not reviewed broadly. The axon skill itself was the doc updated; no other docs are made stale by this change.
- **Skipped items**: Working tree has substantial unrelated dirty state (palette-tauri scaffold, web handlers, server_mode tests, search_crawl edits). Not touched — out of scope.

## Tools and Skills Used

- **Shell (Bash)**: `find` to locate skill copies; `ls` + existence check before writing the session note. No failures.
- **Read**: Read the full SKILL.md to audit for the rule.
- **Edit**: Single insertion in SKILL.md. Succeeded on first attempt.
- **Write**: This session note.
- No MCP tools, agents, subagents, browser tools, or external CLIs were invoked.

## Behavior Changes (Before/After)

- **Before**: Agents using the axon skill saw many example JSON blocks with optional knobs and no instruction to omit them by default. The global "never add unrequested flags" preference was not mirrored into the skill, so MCP usage in particular could over-specify params.
- **After**: A "Parameter discipline (hard rule)" section appears immediately after the intro, telling agents to send bare requests (e.g. `{ "action": "scrape", "url": "…" }`) unless the user named a specific knob. Rule explicitly applies to both MCP and CLI surfaces.

## Risks and Rollback

- Risk: minimal. Doc-only change inside a skill file; no code or runtime behavior altered.
- Rollback: `git checkout HEAD -- plugins/skills/axon/SKILL.md` (or revert the single Edit hunk).

## Next Steps

- If desired, mirror the same rule into the other axon skill copies (`~/.claude/agents/axon/`, `~/.agents/src/agents/axon/`, plugin cache) — but those are likely regenerated from the in-repo source, so editing only the in-repo skill is probably sufficient. Verify by checking the agents-marketplace publishing flow before duplicating.
- Unrelated: the working tree carries large uncommitted scaffolding (`apps/palette-tauri/**`) and several modified server/handler files. Decide whether those belong on this branch or should be split out before any push.
