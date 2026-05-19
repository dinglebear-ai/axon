---
date: 2026-05-07 08:54:17 EST
repo: git@github.com:jmagar/axon.git
branch: bd-teams/ask-perf-foundation
head: 03140366
agent: Claude (claude-sonnet-4-6)
session id: unknown
transcript: unavailable
working directory: /home/jmagar/workspace/axon_rust
pr: "#69 feat(ask): timing instrumentation + --server-url flag (nm9, vrn) — https://github.com/jmagar/axon/pull/69"
---

## User Request

Run the `plugin-dev:skill-reviewer` agent against all skills in `plugins/skills/`, then apply the review findings: fix the `dr` name mismatch and reorganize the `axon` skill to move verbose reference content into `references/`.

## Session Overview

Dispatched a specialized skill-reviewer agent across 16 skills in `plugins/skills/`. The agent identified one critical structural issue (directory/frontmatter name mismatch in `dr`), two major issues in `axon/SKILL.md` (oversized description, missing `allowed-tools`), and gave 14 other skills a clean pass. All identified issues were resolved.

## Sequence of Events

1. Dispatched `plugin-dev:skill-reviewer` agent against all 16 skills in `plugins/skills/`
2. Reviewed the full agent report — 14 pass, 2 need fixes (`dr`, `axon`)
3. Read `dr/SKILL.md` and `axon/SKILL.md` to understand current state
4. Fixed `dr/SKILL.md`: updated `name` field and body header to match directory name `dr`
5. Created `plugins/skills/axon/references/mcp-response-protocol.md` — extracted MCP envelope, artifact ops, and error codes
6. Created `plugins/skills/axon/references/async-job-lifecycle.md` — extracted full async job lifecycle JSON
7. Updated `axon/SKILL.md`: trimmed description, added `allowed-tools`, replaced two verbose sections with concise summaries + reference pointers

## Key Findings

- `plugins/skills/dr/SKILL.md:2` — `name: doctor` while directory is `dr/` (post-rename orphan; git status shows `D plugins/skills/doctor/SKILL.md` + `?? plugins/skills/dr/`)
- `plugins/skills/axon/SKILL.md:3` — description was ~1,100 chars; contains routing instructions ("Always prefer the MCP tool") that belong in the body, not the trigger description
- `plugins/skills/axon/SKILL.md` — missing `allowed-tools` field while all 14 subskills scope to `mcp__plugin_axon_axon__axon`
- 14 subskills (`ask`, `crawl`, `scrape`, `embed`, `query`, `search`, `ingest`, `extract`, `map`, `retrieve`, `sources`, `domains`, `stats`, `status`) rated pass — consistent template, good disambiguators, accurate trigger phrases

## Technical Decisions

- **`name: dr` over reverting to `doctor/`**: The directory was intentionally renamed `dr` (git history shows `D doctor/SKILL.md`). Updated frontmatter to match rather than reverting the rename.
- **Two reference files, not one**: `mcp-response-protocol.md` and `async-job-lifecycle.md` split on logical topic boundary (protocol spec vs job operations) rather than merging into a single large reference file.
- **Kept "Response handling" and "Async jobs" as stub sections in SKILL.md**: Preserving the section headers with 2-sentence summaries means the skill still surfaces these topics during retrieval, while the detail lives in `references/`.
- **Description trimmed to ~330 chars**: Removed all routing instructions (MCP-vs-CLI preference already covered in the body under "When to fall back to the CLI"). Kept all trigger phrases intact.

## Files Modified

| File | Change |
|------|--------|
| `plugins/skills/dr/SKILL.md` | `name: doctor` → `name: dr`; `# axon-doctor` → `# axon-dr` |
| `plugins/skills/axon/SKILL.md` | Trimmed description (~1,100→~330 chars); added `allowed-tools`; collapsed "Async jobs" and "Response handling"+"Errors" sections to stubs with ref pointers |
| `plugins/skills/axon/references/mcp-response-protocol.md` | Created — MCP response envelope, `response_mode` values, artifact subcommands (head/grep/search/read), cleanup ops, error codes |
| `plugins/skills/axon/references/async-job-lifecycle.md` | Created — full async lifecycle JSON for all four families, CLI mirror subcommands, `--wait true` guidance |

## Errors Encountered

None.

## Behavior Changes (Before/After)

- **`dr` skill**: Previously `name: doctor` would load a skill body that said "axon-doctor" — now directory name, `name` field, and body header all say `dr`. Skill activation via `/axon:dr` now correctly resolves.
- **`axon` skill description**: Trigger matching is now tighter — routing instruction prose removed from the description, so Claude won't activate the skill on "prefer MCP over CLI" type queries.
- **`axon` skill `allowed-tools`**: Now scoped to `mcp__plugin_axon_axon__axon`, consistent with all subskills.

## Risks and Rollback

- **`dr` rename**: If any external config, plugin.json, or user keybinding references the skill by name `doctor`, those references will break. Rollback: revert `name: dr` → `name: doctor` and `# axon-dr` → `# axon-doctor`.
- **`axon` description trim**: The removed routing instructions were accurate; they're now only in the body. If the body content is not loaded at trigger time (lazy loading), the routing guidance disappears. Rollback: restore original description from git.

## Decisions Not Taken

- **Revert `dr/` directory to `doctor/`**: Would require a `git mv` and conflict with the existing git state (`D plugins/skills/doctor/SKILL.md` already staged). Updating the frontmatter was cleaner.
- **Single large reference file**: Considered `references/reference.md` as a catch-all; rejected in favor of topically named files that are easier to link and update independently.
- **Move "Choosing parameters" table to references/**: Decided to keep it inline — it's a decision aid used during active skill execution, not a lookup reference.

## Next Steps

**Unfinished from this session:**
- None — all identified issues were resolved.

**Follow-on tasks:**
- Register `dr` as an alias or update plugin.json if it references the old `doctor` name
- Consider whether the `axon` parent skill's `allowed-tools` scoping to only the MCP tool is correct, or whether it also needs CLI (`Bash`) access for the "fall back to CLI" path
- Run `plugin-dev:plugin-validator` to confirm `plugin.json` references are consistent with the updated skill names
