# Session: CLAUDE.md Quality Audit and Updates
**Date:** 2026-03-10 01:53
**Branch:** `refactor/acp-performance-modern-rust`

---

## Session Overview

Ran the `claude-md-improver` skill to audit all CLAUDE.md files in the axon_rust repo. Discovered 11 files (excluding node_modules copy), assessed quality against the standard rubric, identified 3 files with stale or missing content due to recent branch changes, and applied targeted updates.

---

## Timeline

1. **Discovery** — Found 11 CLAUDE.md files via `find` across `./`, `crates/*/`, `apps/web/`, `docker/`, `docs/`
2. **Assessment** — Read all 11 files in parallel; cross-referenced against `git diff HEAD` to identify what changed in this branch
3. **Report** — Generated quality scores (A–F scale) and identified 3 files needing updates
4. **Updates** — Applied all 3 targeted edits, user confirmed and approved

---

## Key Findings

- `crates/cli/CLAUDE.md:38` — `screenshot/spider_capture.rs` listed but deleted in this branch; `screenshot_migration_tests.rs` added but absent from layout
- `crates/web/CLAUDE.md` — `ssh_auth.rs` (new ~300-line module) not documented at all; `DownloadAuthState` struct missing; SSH challenge-response auth flow completely absent
- `CLAUDE.md:317-336` — `AXON_REQUIRE_DUAL_AUTH` and `AXON_SSH_AUTHORIZED_KEYS` new env vars missing from Web App Security section
- All other CLAUDE.md files (vector, ingest, crawl, mcp, core, jobs, docker, docs) scored 87–94 and required no changes

---

## Technical Decisions

- **Added full Auth Stack table to `crates/web/CLAUDE.md`** rather than just listing the module — the dual-auth mode default change (`AXON_REQUIRE_DUAL_AUTH=true`) is a security posture change operators need to understand
- **Documented SSH challenge-response flow as a numbered sequence** — the 4-step nonce/sign/verify cycle is non-obvious; step 2 shows the exact `ssh-keygen` command
- **Added `DownloadAuthState` gotcha** — explained why it exists (lighter than AppState, no WS/stats overhead) so future contributors don't wonder if it's a refactor candidate
- **Root CLAUDE.md env table additions** — kept comments tight (2 lines per var) matching the existing style in that section

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/cli/CLAUDE.md` | Replaced `spider_capture.rs` with `screenshot_migration_tests.rs` in module layout | Remove stale deleted file; add new test file |
| `crates/web/CLAUDE.md` | Added `ssh_auth.rs` + `tailscale_auth.rs` to directory intent; added Auth Stack section with table, SSH flow, `DownloadAuthState` | Document new SSH key auth layer |
| `CLAUDE.md` | Added `AXON_REQUIRE_DUAL_AUTH` and `AXON_SSH_AUTHORIZED_KEYS` to Web App Security Env | Document new auth env vars |

---

## Commands Executed

```bash
find /home/jmagar/workspace/axon_rust -name "CLAUDE.md" | head -50
# → 12 results (11 relevant, 1 in node_modules — excluded)

git diff --stat HEAD
# → 27 files changed, 1008 insertions, 757 deletions

git diff HEAD -- crates/cli/CLAUDE.md
# → confirms run_ask_native → run_ask already updated; spider_capture.rs deletion visible

git diff HEAD -- crates/cli/commands/screenshot.rs
# → spider_capture mod removed; now calls crates/services::screenshot::screenshot_capture

ls crates/cli/commands/screenshot/
# → screenshot_migration_tests.rs  util.rs  (spider_capture.rs gone)

cat crates/web/ssh_auth.rs | head -40
# → SSH challenge-response auth module; nonce store, subprocess verification

git diff HEAD -- crates/web/tailscale_auth.rs | head -80
# → SshKeyIdentity added, DualAuth AuthOutcome added, require_dual_auth field added (default: true)
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `crates/cli/CLAUDE.md` module layout | Listed deleted `spider_capture.rs` | Lists actual `screenshot_migration_tests.rs` |
| `crates/web/CLAUDE.md` security docs | No SSH auth documentation | Full Auth Stack table + 4-step SSH flow + DownloadAuthState |
| Root `CLAUDE.md` env vars | Missing `AXON_REQUIRE_DUAL_AUTH`, `AXON_SSH_AUTHORIZED_KEYS` | Both vars documented with comments |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `spider_capture.rs` removed from cli CLAUDE.md | Not present | Not present | ✓ |
| `screenshot_migration_tests.rs` in cli CLAUDE.md | Present | Present | ✓ |
| `ssh_auth.rs` in web CLAUDE.md directory intent | Present | Present | ✓ |
| Auth Stack section in web CLAUDE.md | Present | Present | ✓ |
| `AXON_REQUIRE_DUAL_AUTH` in root CLAUDE.md | Present | Present | ✓ |
| `AXON_SSH_AUTHORIZED_KEYS` in root CLAUDE.md | Present | Present | ✓ |

---

## Source IDs + Collections Touched

*(Populated after Axon embed step below)*

---

## Risks and Rollback

- All changes are documentation only — no code modified
- Rollback: `git checkout crates/cli/CLAUDE.md crates/web/CLAUDE.md CLAUDE.md`

---

## Decisions Not Taken

- **Did not update `apps/web/CLAUDE.md`** for new Pulse pane components (`pulse-logs-pane.tsx`, `pulse-mcp-pane.tsx`, `pulse-terminal-pane.tsx`) — these are untracked new files with thin implementations (wrappers over existing components). Worth adding when the panes stabilize and get wired into the workspace layout.
- **Did not bump "Last Modified" dates** in changed CLAUDE.md files — dates track substantive codebase changes, not doc-only updates

---

## Open Questions

- `apps/web/CLAUDE.md` — Pulse workspace pane architecture (logs/mcp/terminal panes) should be documented once the pane-switcher integration is complete
- `crates/web/CLAUDE.md` — Does `AXON_REQUIRE_DUAL_AUTH` apply to download routes as well, or only `/ws`? `DownloadAuthState` carries the same fields but actual enforcement depends on handler code

---

## Next Steps

- When Pulse pane work completes on this branch, update `apps/web/CLAUDE.md` with the 3 new pane components and their roles
- Verify `AXON_REQUIRE_DUAL_AUTH` enforcement scope (WS-only vs download routes) and update `crates/web/CLAUDE.md` accordingly if different
