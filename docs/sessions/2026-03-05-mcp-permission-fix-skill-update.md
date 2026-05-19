# MCP Permission Fix + Skill/Command Update to MCP-First

**Date:** 2026-03-05
**Branch:** `feat/services-layer-refactor`

## Session Overview

Debugged "Permission denied (os error 13)" errors on all MCP tool calls inside the axon-workers Docker container, then updated all axon skills and commands to use the `mcp__axon__axon` MCP tool directly instead of the CLI wrapper script, with CLI fallback.

## Timeline

1. **MCP Permission Debugging** — Traced "Permission denied" from MCP tool responses through Rust code to bind mount ownership
2. **Root Cause Identified** — `/home/jmagar/appdata/axon/artifacts` bind-mounted as `root:root` (755), `axon` user (UID 1001) can't write
3. **Three-Layer Fix Applied** — Immediate host chown, code-level writability probe, s6 cont-init.d permission fixup
4. **Docker Rebuild + Verify** — Rebuilt image, restarted containers, confirmed all MCP calls working
5. **Skill + Command Rewrite** — Rewrote `SKILL.md` and all 19 `.claude/commands/axon/*.md` files to use MCP tool directly
6. **CLI Fallback Added** — Added `Bash` to `allowed-tools` and fallback instructions in SKILL.md

## Key Findings

- `fs::create_dir_all()` succeeds on existing directories regardless of write permission — writability must be probed separately (`common.rs`)
- Bind mount ownership from host persists into containers, overriding Dockerfile `chown`
- s6-overlay `cont-init.d/` scripts run as root before services — ideal for permission fixup
- `ensure_artifact_root()` needed a write probe, not just existence check

## Technical Decisions

- **Write probe over stat check**: Used temp file write+delete instead of checking Unix permissions bits — more portable and handles ACLs/SELinux
- **Fallback artifact root**: `/tmp/axon-mcp` fallback if primary dir not writable
- **MCP-first, CLI-fallback**: Skill uses MCP tool by default, falls back to `./scripts/axon` CLI only when MCP unavailable
- **Safe prefix validation**: cont-init.d script validates `AXON_MCP_ARTIFACT_DIR` resolves under `/app/`, `/home/`, `/data/`, or `/tmp/`

## Files Modified

| File | Purpose |
|------|---------|
| `crates/mcp/server/common.rs` | Added `is_writable()` probe, rewrote `ensure_artifact_root()` |
| `docker/s6/cont-init.d/10-load-axon-env` | Added MCP artifact dir permission fixup block (lines 76-89) |
| `.claude/skills/axon/SKILL.md` | Rewritten: MCP-first with CLI fallback, execution order, full CLI mapping table |
| `.claude/commands/axon/*.md` (19 files) | All rewritten with exact `mcp__axon__axon` JSON call examples + `Bash` in allowed-tools |

## Commands Executed

| Command | Purpose |
|---------|---------|
| `cat /proc/<pid>/cgroup` | Identify container for MCP server process |
| `docker inspect axon-workers --format '{{json .Mounts}}'` | Find bind mount paths and permissions |
| `sudo chown -R 1001:1001 /home/jmagar/appdata/axon/{artifacts,worker}` | Immediate permission fix |
| `docker compose build axon-workers` | Rebuild with code fixes |
| `docker compose up -d axon-workers` | Deploy fixed container |
| MCP tool calls (doctor, query, stats, sources, embed list) | End-to-end verification |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| MCP tool calls | All fail with "Permission denied (os error 13)" | All succeed |
| `ensure_artifact_root()` | Only checks dir existence | Probes writability, falls back to `/tmp/axon-mcp` |
| Container startup | No artifact dir fixup | `10-load-axon-env` chowns artifact dir to `axon:axon` |
| Axon commands | Use `./scripts/axon` CLI wrapper | Use `mcp__axon__axon` MCP tool, CLI as fallback |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| MCP `doctor` | `ok: true` | `ok: true`, all services connected | PASS |
| MCP `query` | Results returned | 10 results from cortex collection | PASS |
| MCP `stats` | Collection stats | 2.57M points, 1536 dims | PASS |
| MCP `sources` | Source list | Sources with chunk counts | PASS |
| MCP `embed list` | Job list | Recent embed jobs listed | PASS |

## Risks and Rollback

- **Low risk**: Code change is additive (probe function + fallback path). Original behavior preserved when dir is writable.
- **Rollback**: Revert `common.rs` changes, remove lines 76-89 from `10-load-axon-env`
- **Skill/command changes**: Pure documentation — no runtime impact, easily reverted via git

## Decisions Not Taken

- **Unix permission bit checking** — Rejected: doesn't account for ACLs, SELinux, or bind mount quirks
- **Making artifact dir configurable via MCP params** — Overkill for current needs
- **Removing CLI fallback entirely** — MCP may not always be available (e.g., local dev without workers)

## Open Questions

- None — all fixes verified end-to-end

## Next Steps

- Monitor MCP stability across container restarts
- Consider adding artifact dir health check to `axon doctor`
