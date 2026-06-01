# Shared brief — comprehensive stale-docs refresh (Phase 1: verify + fix in place)

**Date:** 2026-06-01
**Worktree (your ONLY workspace):** `/home/jmagar/workspace/axon/.worktrees/docs-refresh`
**Branch:** `docs/comprehensive-refresh`

## Mission

You are one of 8 parallel agents each refreshing a disjoint set of Axon's living/reference docs.
Your job is to make your assigned docs **accurate against the current source code**, fixing them
**in place**. A separate later phase handles reorganization — so you must NOT move, rename, or
delete any file, and must NOT run any git command.

## Absolute rules

1. **Operate only inside the worktree** `/home/jmagar/workspace/axon/.worktrees/docs-refresh`.
   Never touch `/home/jmagar/workspace/axon` (the main checkout) or any other `.worktrees/*`.
   Use absolute paths under the worktree for every read and edit.
2. **Edit only the files in your assignment.** No other doc. Files are partitioned so agents never
   collide — staying in your lane is what keeps this safe.
3. **Fix in place only.** Do NOT `git mv`, rename, move, delete, or create new doc files in `docs/`
   (except your one report file, see below). Reorg is a later serial phase.
4. **No git operations.** Do not commit, stage, branch, or push. Just edit files on disk.
5. **AGENTS.md and GEMINI.md are symlinks to CLAUDE.md** — edit the `CLAUDE.md` target; never edit
   the symlinks directly.

## Method — ground truth is the code, the doc is suspect

Treat the existing doc as a hypothesis to verify, not a source of truth. For every factual claim
(flags, defaults, command names, env vars, file paths, module names, ports, behaviors):

- **Verify against current source.** Read the relevant `src/**`, `Cargo.toml`, `docker-compose*.yaml`,
  `config.example.toml`, `.env.example`, `justfile`, `scripts/**`, and `src/*/CLAUDE.md`.
- **CLI command docs:** the canonical CLI help is pre-dumped from the working v4.16.0 binary at
  `docs/reports/2026-06-01-stale-docs-refresh/ground-truth/axon-<command>--help.txt`. Cross-check
  every flag/default/arg against that file AND against the clap definition in
  `src/cli/commands/<command>.rs` / `src/core/config/cli.rs`.
- **MCP docs:** `docs/MCP-TOOL-SCHEMA.md` is the declared wire-contract source of truth; the schema
  itself lives in `src/mcp/`. Derive from code/schema and reconcile prose to it.
- **Prior audit (2026-05-06):** `docs/reports/2026-05-06-stale-docs-audit/` has the last methodology
  and findings (A-root, B-commands, C-mcp, D-per-crate, E-plugin-skills, F-misc). Read the section(s)
  relevant to your lane for known problem areas — but re-verify; that audit is ~1 month old.

When a claim contradicts the code: fix the doc to match the code. When the doc describes something
that no longer exists: remove/correct it. When the code has behavior the doc omits and it's in scope:
add it. Keep the doc's existing voice, structure, and formatting conventions. Do not rewrite for
style — change what is wrong or missing.

## Watch for these common staleness signatures in Axon

- Commands that exist in the binary but not in docs (e.g. `endpoints`, `train`, `monitor`, `sync`,
  `preflight`, `smoke`, `compose`) or vice versa.
- `axon_rust` vs `axon` naming (repo was renamed; data dir is `~/.axon`, not `~/.local/share/axon`).
- Version numbers — current is **4.16.0** (`Cargo.toml`). Flag stale version strings.
- "lite mode" / Postgres / Redis / RabbitMQ / AMQP references — that path was **removed**; runtime is
  SQLite + in-process workers only.
- Default collection name (`cortex` vs `axon` — verify against `src/core/config` and `--help`).
- Removed env vars (`OPENAI_*`, `--openai-*`) — replaced by Gemini headless path.
- Ports, service names in compose, TEI/Qdrant URLs.

## Deliverable — write ONE report file

Write your findings to:
`docs/reports/2026-06-01-stale-docs-refresh/agent-reports/<AGENT-LETTER>-<short-name>.md`

Structure it as:

```
# Agent <LETTER> — <domain> report

## Files reviewed
- path — verdict (accurate | minor fixes | major fixes | stale-removed)

## Fixes made
Concrete list: file, what was wrong, what it now says (cite source you verified against).

## Gaps / missing docs (for Phase 2)
Docs that SHOULD exist but don't (e.g. no doc for `axon diff`, `axon brand`). Describe what each
missing doc should cover. Do NOT create them — just report.

## Reorg observations (for Phase 2)
Misfiled docs, overlaps, redundancy, naming inconsistencies, things that belong in a different
section. The orchestrator will design the new hierarchy from these.

## Cross-reference notes
Any links FROM your docs TO other docs (so the reorg phase can fix them), and any code→doc path
references you noticed.
```

Then return a SHORT summary (≤200 words) of: # files reviewed, # fixed, biggest accuracy problems
found, and the top gaps/reorg notes. Your edits persist on disk; the summary is for the orchestrator.
