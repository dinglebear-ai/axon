# Session: MCP Config Update & Rust Build Optimizations
**Date:** 2026-03-02
**Duration:** Short configuration session

---

## Session Overview

Updated all three AI tool MCP configs to use the new `axon mcp` subcommand (post-refactor from separate binary), cleaned up stale symlinks, and installed + configured system-wide Rust build optimizations (mold linker, sccache, split-debuginfo).

---

## Timeline

1. Activated Serena project for `axon_rust`
2. Read current MCP configs across all three AI tools — all pointed to old `axon-mcp` binary
3. Updated `~/.claude.json`, `~/.codex/config.toml`, `~/.gemini/settings.json` to `axon mcp` subcommand
4. Simplified command from full path → bare `axon` (in PATH) since env loading is now deterministic in the binary
5. Deleted stale `~/.local/bin/axon-mcp` symlink (pointed to non-existent `scripts/axon-mcp`)
6. Updated `~/.local/bin/axon` symlink: wrapper script → `target/release/axon` → `target/debug/axon` (active dev)
7. Audited existing build optimizations; identified mold + split-debuginfo as missing wins
8. Installed mold 2.37.1, created `~/.cargo/config.toml` with all optimizations globally
9. Removed now-redundant per-project `.cargo/config.toml`

---

## Key Findings

- All three AI tool configs were pointing to the old `axon-mcp` standalone binary path, not the new subcommand
- `~/.local/bin/axon-mcp` symlink was stale — `scripts/axon-mcp` no longer exists
- `~/.local/bin/axon` was pointing to `scripts/axon` wrapper (which sourced `.env`) — unnecessary since binary now loads env deterministically
- Project `.cargo/config.toml` only contained `rustc-wrapper = "sccache"` — safe to promote to global and delete
- `clang` 20.1.8 already installed — required as linker driver for mold
- `codegen-units` and `incremental` already default to optimal values for dev; no explicit config needed
- mold has most impact on **debug builds** (link-time dominant); limited impact on release due to `lto = "thin"` + `codegen-units = 1`
- `split-debuginfo = "unpacked"` is dev-only — no effect on release (which already runs `strip = true`)

---

## Technical Decisions

- **Bare `axon` command over full path**: Binary is in PATH via `~/.local/bin/axon` symlink; full path creates maintenance burden when binary location changes.
- **`target/debug/axon` over `target/release/axon` in symlink**: Active development benefits from faster debug builds and better error messages; release symlink was premature.
- **Global `~/.cargo/config.toml` over per-project**: Build optimizations (linker, sccache, split-debuginfo) are machine-level concerns, not project-level. Avoids duplication across every Rust project.
- **`clang` as linker driver with `-fuse-ld=mold`** over `mold` directly: More compatible approach; clang handles the linker invocation and delegates to mold.
- **Rejected**: Keeping `scripts/axon-mcp` wrapper — it no longer exists and the binary handles env loading natively now.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `~/.claude.json` | Modified | Updated `mcpServers.axon.command` → `"axon"`, `args` → `["mcp"]` |
| `~/.codex/config.toml` | Modified | Updated `[mcp_servers.axon]` command → `"axon"`, args → `["mcp"]` |
| `~/.gemini/settings.json` | Modified | Updated `mcpServers.axon.command` → `"axon"`, `args` → `["mcp"]` |
| `~/.local/bin/axon` | Symlink updated | Points to `target/debug/axon` (was `scripts/axon` wrapper) |
| `~/.local/bin/axon-mcp` | Deleted | Stale symlink to non-existent `scripts/axon-mcp` |
| `~/.cargo/config.toml` | Created | Global Rust build config: sccache + mold + split-debuginfo |
| `/home/jmagar/workspace/axon_rust/.cargo/config.toml` | Deleted | Promoted to global; no longer needed per-project |

---

## Commands Executed

```bash
# Verify binary locations
ls -la /home/jmagar/.local/bin/axon*
ls -la /home/jmagar/workspace/axon_rust/target/release/axon
ls -la /home/jmagar/workspace/axon_rust/target/debug/axon

# Delete stale symlink
rm /home/jmagar/.local/bin/axon-mcp

# Update symlink to debug binary
ln -sf /home/jmagar/workspace/axon_rust/target/debug/axon /home/jmagar/.local/bin/axon

# Install mold
sudo apt install -y mold
# Result: mold 2.37.1 installed

# Remove per-project cargo config
rm /home/jmagar/workspace/axon_rust/.cargo/config.toml
rmdir /home/jmagar/workspace/axon_rust/.cargo
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| MCP server command (all 3 tools) | `/home/jmagar/.local/bin/axon-mcp` (or `target/debug/axon-mcp`), `args: []` | `axon`, `args: ["mcp"]` |
| `~/.local/bin/axon` target | `scripts/axon` wrapper (sources `.env`) | `target/debug/axon` directly |
| `~/.local/bin/axon-mcp` | Stale symlink to non-existent script | Deleted |
| Rust linker (all projects) | Default `ld` | `mold` via clang driver |
| Debug build debug info | Monolithic `.dwp` merge | Unpacked (faster, no merge step) |
| sccache scope | Per-project (axon_rust only) | System-wide (all Rust projects) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `ls -la ~/.local/bin/axon` | → `target/debug/axon` | `→ /home/jmagar/workspace/axon_rust/target/debug/axon` | ✅ |
| `ls ~/.local/bin/axon-mcp` | Not found | Removed (no output) | ✅ |
| `mold --version` | Installed | `mold 2.37.1 (compatible with GNU ld)` | ✅ |
| `cat ~/.cargo/config.toml` | sccache + mold + split-debuginfo | All three present | ✅ |
| `ls ~/.cargo/config.toml` (per-project) | Deleted | Directory removed | ✅ |

---

## Risks and Rollback

- **mold compatibility**: mold is broadly compatible but rare edge cases exist with unusual linker flags. Rollback: remove `[target.x86_64-unknown-linux-gnu]` section from `~/.cargo/config.toml`.
- **Debug binary in PATH**: If `cargo build` hasn't run since last change, `axon` in PATH will be stale. No auto-rebuild. Mitigation: run `cargo build --bin axon` after changes.
- **sccache going global**: If sccache isn't running/installed on another machine sharing this config, builds will error. Mitigation: `sccache` is already installed and the binary exists at expected path.

---

## Decisions Not Taken

- **`codegen-units = 256` explicitly in `[profile.dev]`**: Already the Cargo default for dev; redundant.
- **`incremental = true` explicitly**: Already the Cargo default for dev; redundant.
- **Cranelift backend**: Experimental, not worth stability risk for this project.
- **Keep release binary in PATH**: Overridden in favor of debug for active development cycle.
- **Keep per-project `.cargo/config.toml`**: Redundant once global config was created; deleted to avoid confusion.

---

## Open Questions

- When switching to production deploy, symlink should be updated back to `target/release/axon` — no automation exists for this yet.
- `scripts/axon` wrapper still exists (only `scripts/axon-mcp` was removed) — may want to delete it if truly unused.

---

## Next Steps

- Run `cargo build --bin axon` to confirm mold is wired correctly and build succeeds
- Consider deleting `scripts/axon` wrapper if nothing else references it
- When ready for production, update symlink: `ln -sf .../target/release/axon ~/.local/bin/axon`
