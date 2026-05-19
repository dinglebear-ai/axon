# Session: Fix Missing Sonnet/Haiku Models for Claude ACP Adapter
**Date:** 2026-03-22
**Branch:** feat/pulse-shell-and-hybrid-search
**Duration:** ~45 minutes

---

## Session Overview

Investigated and fixed a bug where the Pulse Chat UI only showed "Server default" and "Opus 4.6" as Claude model options — Sonnet 4.6 and Haiku 4.5 were missing. Codex and Gemini model lists loaded correctly. Root cause was a stale native `claude-agent-acp` binary (March 6) that predated the Claude 4.6 Sonnet/Haiku releases. Fixed by installing `@zed-industries/claude-agent-acp@0.22.2` and pointing `AXON_ACP_CLAUDE_ADAPTER_CMD` to it.

---

## Timeline

1. **Symptom reported** — Pulse Chat → Claude agent → model selector shows only "Server default" + "Opus 4.6". Codex and Gemini models load fine.
2. **Code path traced** — WS `config_options_update` events → `axon-shell-state.ts:321-325` → `getAcpModelConfigOption()` → model options derived from live ACP probe, not hardcoded.
3. **ACP probe path traced** — `/api/pulse/config` → `pulse_chat_probe` WS mode → `establish_acp_session()` → `apply_config_and_model()` in `crates/services/acp/session.rs`.
4. **Asymmetry found** — Codex has `read_codex_cached_model_options()` fallback from `~/.codex/models_cache.json`; Gemini uses `~/.gemini/settings.json`; **Claude has no fallback at all** — entirely dependent on live ACP probe response.
5. **Binary version mismatch confirmed** — `/usr/local/bin/claude-agent-acp` from March 6 (MD5: `bb997edf9b922a8d264ac5a81ac4308b`) vs `claude` CLI 2.1.81 from March 20 (MD5: `0e044662e3f2edc8fe199aa8b5f97fdc`). Two separate executables.
6. **Newer version found** — `@zed-industries/claude-agent-acp@0.22.2` available on npm (released March 18). Zed's agents dir had 0.21.0 (March 9).
7. **Fix applied** — Installed 0.22.2, created wrapper at `~/.local/bin/claude-agent-acp`, updated `.env`.

---

## Key Findings

- **Root cause**: `AXON_ACP_CLAUDE_ADAPTER_CMD=/usr/local/bin/claude-agent-acp` pointed to a native binary from March 6, 2026. Claude 4.6 Sonnet and Haiku were not in that binary's model list.
- **Architecture gap**: `crates/services/acp/session.rs:375-401` — Codex and Gemini have local model cache fallbacks; Claude does not. When the ACP adapter reports a truncated model list, there is no recovery path.
- **`map_config_options()` validation** (`crates/services/acp/mapping.rs:84-142`) — silently drops config options where `current_value` is absent from the options list. This is correct behavior but means a stale adapter that returns partial options produces a silently truncated UI.
- **Model options are dynamic** — the ACP adapter calls the underlying claude CLI at runtime to get the model list. An old adapter binary = old model list, even with a new `claude` CLI installed.
- **npm package locations**:
  - Stale native binary: `/usr/local/bin/claude-agent-acp` (120MB, root-owned, March 6)
  - Zed agents: `/home/jmagar/.local/share/zed/external_agents/claude-agent-acp/0.21.0/` (March 9)
  - Newly installed: `/home/jmagar/.local/share/fnm/node-versions/v24.13.0/installation/lib/node_modules/@zed-industries/claude-agent-acp/` (0.22.2, March 18)

---

## Technical Decisions

- **Wrapper script over direct env var** — The fnm node binary path (`/run/user/1000/fnm_multishells/.../bin/node`) is ephemeral per shell session. Created a stable wrapper at `~/.local/bin/claude-agent-acp` that uses the absolute fnm node path (`/home/jmagar/.local/share/fnm/node-versions/v24.13.0/installation/bin/node`).
- **Did not replace `/usr/local/bin/claude-agent-acp`** — Root-owned, 120MB native binary. Replacing it would require sudo and mixing a JS wrapper where a native binary was expected. Safer to redirect via `.env`.
- **Did not implement Claude model fallback in Rust** — Would require discovering Claude's model list format (not documented as a local cache file). The npm package fix addresses the root cause directly.
- **Used `@zed-industries/claude-agent-acp` not `@zed-industries/claude-code-acp`** — `claude-code-acp` (0.16.2) is a different package; `claude-agent-acp` (0.22.2) is the correct ACP server adapter for Claude.

---

## Files Modified

| File | Purpose |
|------|---------|
| `/home/jmagar/workspace/axon_rust/.env` (line 118) | Changed `AXON_ACP_CLAUDE_ADAPTER_CMD` from `/usr/local/bin/claude-agent-acp` to `/home/jmagar/.local/bin/claude-agent-acp` |
| `/home/jmagar/.local/bin/claude-agent-acp` | Created wrapper script (new file) that invokes `@zed-industries/claude-agent-acp@0.22.2` via pinned node path |

---

## Commands Executed

```bash
# Confirmed stale binary
ls -la /usr/local/bin/claude-agent-acp
# → -rwxr-xr-x root root 120878417 Mar 6 06:59

# Confirmed binary mismatch
md5sum /usr/local/bin/claude-agent-acp → bb997edf9b922a8d264ac5a81ac4308b
md5sum $(which claude)                  → 0e044662e3f2edc8fe199aa8b5f97fdc

# Found newer version
npm view @zed-industries/claude-agent-acp time
# → 0.22.2 released 2026-03-18T14:31:42.547Z

# Installed latest
npm install -g @zed-industries/claude-agent-acp@0.22.2
# → added 6 packages in 1s

# Verified install path
ls -la /home/jmagar/.local/share/fnm/node-versions/v24.13.0/installation/bin/claude-agent-acp
# → symlink to ../lib/node_modules/@zed-industries/claude-agent-acp/dist/index.js

# Tested wrapper
~/.local/bin/claude-agent-acp --version → exit 0
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| Pulse Chat → Claude → Model selector | "Server default", "Opus 4.6" only | Should show "Server default", "Opus 4.6", "Sonnet 4.6", "Haiku 4.5" |
| `AXON_ACP_CLAUDE_ADAPTER_CMD` | `/usr/local/bin/claude-agent-acp` (March 6, native binary, stale model list) | `/home/jmagar/.local/bin/claude-agent-acp` (wrapper → `@zed-industries/claude-agent-acp@0.22.2`, March 18) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `ls -la /home/jmagar/.local/bin/claude-agent-acp` | executable wrapper | `-rwxrwxr-x 233 bytes` | ✅ |
| `~/.local/bin/claude-agent-acp --version` | exit 0 | exit 0 | ✅ |
| `cat /home/jmagar/.local/share/fnm/.../claude-agent-acp/package.json \| python3 -c ...` | version 0.22.2 | `version: 0.22.2` | ✅ |
| `.env` line 118 | new path | `/home/jmagar/.local/bin/claude-agent-acp` | ✅ |
| Pulse Chat Claude model list after restart | Sonnet + Haiku visible | **NOT YET VERIFIED** — `axon serve` was not running during this session | ⏳ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session (debugging only).

---

## Risks and Rollback

**Risk**: The wrapper uses a pinned node version path (`v24.13.0`). If fnm switches to a different node version as default, the wrapper will still invoke v24.13.0 (pinned, not dynamic). This is intentional stability — update the wrapper path if the node version changes.

**Risk**: `/usr/local/bin/claude-agent-acp` (the stale native binary) is still present. If something restores `AXON_ACP_CLAUDE_ADAPTER_CMD` to the old value, the bug returns.

**Rollback**: Revert `.env` line 118 to `AXON_ACP_CLAUDE_ADAPTER_CMD=/usr/local/bin/claude-agent-acp`. The wrapper at `~/.local/bin/claude-agent-acp` can remain without harm.

---

## Decisions Not Taken

- **Implement Claude local model fallback in Rust** (`crates/services/acp/session.rs` + `crates/services/acp/config.rs`) — Would require knowing where Claude Code stores its model cache on disk. Not documented. The binary fix addresses root cause; this would be a defense-in-depth enhancement for the future.
- **Replace `/usr/local/bin/claude-agent-acp`** — Root-owned, 120MB. Using `.env` override is safer and doesn't require sudo.
- **Use `claude-code-acp` package** — Different package (`@zed-industries/claude-code-acp@0.16.2`); its binary is named `claude-code-acp`, not `claude-agent-acp`. Wrong package for this adapter slot.
- **Point directly at claude CLI** — `claude` binary supports ACP via `--output-format stream-json --input-format stream-json` flags but the handshake is managed by the `claude-agent-acp` npm wrapper. Bypassing the wrapper would require duplicating its protocol negotiation.

---

## Open Questions

- **Is `axon serve` expected to pick up `.env` changes automatically, or does it require a restart?** — Assumed restart required; `.env` is read at process startup.
- **Why was `/usr/local/bin/claude-agent-acp` installed as a native 120MB binary?** — Likely a pre-built binary from an older Claude Code release. The current npm packages are pure JS wrappers (~1KB). May have been installed manually or via a prior `npm install -g` that produced a native binary.
- **Does `@zed-industries/claude-agent-acp@0.22.2` report the full Claude 4.6 model list?** — Not yet verified end-to-end. Requires restarting `axon serve` and triggering a `pulse_chat_probe` for Claude.

---

## Next Steps

1. **Restart `axon serve`** to reload `.env` with the new adapter path.
2. **Verify Sonnet/Haiku appear** in Pulse Chat → Claude model selector.
3. **Consider adding a Claude model fallback** in `crates/services/acp/session.rs` analogous to `read_codex_cached_model_options()` — eliminates dependency on live probe for model list rendering.
4. **Document the adapter binary locations** in `crates/web/CLAUDE.md` or `docs/ACP.md` so future maintainers know where to look if the model list is stale again.
