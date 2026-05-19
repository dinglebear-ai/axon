# Fix save-to-md Embed Job ID Bug

**Date:** 2026-02-25
**Branch:** feat/crawl-download-pack

---

## Session Overview

Fixed a consistent failure in the `/save-to-md` command across all three install locations. The command instructed Claude to read `data.url` and `data.collection` from the initial `axon embed --json` response, but those fields are only present in `axon embed status <job_id> --json` after the job completes. The initial response only contains `data.job_id`.

---

## Timeline

1. User reported `/save-to-md` fails 100% of the time with a specific error
2. Context note from previous session identified the root cause: initial embed JSON lacks `data.url`/`data.collection`
3. Read all three command files — confirmed they were identical and all had the same bug
4. Fixed all three files to: capture `data.job_id` from embed → poll status → extract `data.url`/`data.collection` from status output → retrieve verify

---

## Key Findings

- `axon embed "<path>" --json` returns `{ data: { job_id: "...", status: "queued" } }` — no `data.url` or `data.collection`
- `axon embed status "<job_id>" --json` returns the completed job with `data.url` and `data.collection`
- All three command files were byte-for-byte identical before the fix
- `~/.claude/commands/save-to-md.md` was already updated automatically when `~/claude-homelab/commands/save-to-md.md` was edited (likely a symlink or sync)

---

## Technical Decisions

- Added an explicit step 5 for polling `axon embed status <job_id> --json` rather than inlining the status call — keeps the two operations distinct and makes the "retry if queued/running" instruction clear
- Renumbered downstream steps (6, 7, 8) rather than collapsing — preserves granularity for error reporting
- Applied identical fix logic to the Codex variant (`~/.codex/prompts/save-to-md.md`) which uses a more condensed prose style vs the Claude version's bash code blocks

---

## Files Modified

| File | Change |
|------|--------|
| `/home/jmagar/claude-homelab/commands/save-to-md.md` | Fixed embed steps 3–7 to use job_id → status poll flow |
| `/home/jmagar/.claude/commands/save-to-md.md` | Same fix (auto-synced) |
| `/home/jmagar/.codex/prompts/save-to-md.md` | Same fix, adapted to Codex prose style |

---

## Behavior Changes (Before/After)

| Before | After |
|--------|-------|
| Step 3: read `data.url`/`data.collection` from initial embed response (field never present) | Step 3: capture `data.job_id` from initial embed response |
| Steps 4–7 unreachable due to missing fields | Step 5: `axon embed status <job_id> --json` → read `data.url`/`data.collection` |
| Command consistently failed at embed verification | Command now follows the actual two-phase async embed API |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| Read all 3 command files | Identical content | Confirmed identical | ✅ |
| Edit `~/claude-homelab/commands/save-to-md.md` | Steps updated | Applied | ✅ |
| Read `~/.claude/commands/save-to-md.md` after edit | Already updated | Confirmed | ✅ |
| Edit `~/.codex/prompts/save-to-md.md` | Steps updated | Applied | ✅ |

---

## Source IDs + Collections Touched

None — this session modified command/prompt files only, no Axon embed/retrieve operations prior to this save.

---

## Risks and Rollback

- Low risk — prompt files only, no code changes
- Rollback: revert the three files to their pre-fix state (re-add `data.url`/`data.collection` to step 3, remove the status poll step)

---

## Decisions Not Taken

- Did not add `--wait true` to `axon embed` call — the poll-status approach is correct; `--wait` blocks the terminal which is worse UX for long embeds
- Did not merge the Codex and Claude command files into one canonical source — they serve different runtimes with different formatting conventions

---

## Open Questions

- Is `~/claude-homelab/commands/save-to-md.md` a symlink to `~/.claude/commands/save-to-md.md`, or did the hook sync them? (Both showed identical content after one edit.)
- Does `axon embed status` require the job to be `completed` before `data.url`/`data.collection` appear, or do they appear while still `running`?

---

## Next Steps

- Verify the fix works end-to-end on the next `/save-to-md` invocation
- If the Gemini variant exists (`~/.gemini/` or similar), check and apply the same fix
