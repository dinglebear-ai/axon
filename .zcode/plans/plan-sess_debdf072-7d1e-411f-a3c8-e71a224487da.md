## Summary of changes

Seven edits across three scopes: (A) the global cargo front-door + config, (B) axon's repo-local config + docs, (C) a new xtask check. All address root causes surfaced in the review.

---

### A. Global: `~/.local/bin/cargo` (front-door shim) + `~/.cargo/config.toml`

**A1. Pin `CARGO_HOME` in the front-door shim (root-cause fix for `.cargo` cache leak)**

In `~/.local/bin/cargo`, near the top (after `set -euo pipefail`, before any soldr invocation), add:

```bash
# Pin CARGO_HOME to the global home. soldr otherwise redirects CARGO_HOME to
# <repo>/.cargo per workspace, which re-fetches the registry per repo and
# writes .package-cache/.global-cache files into the repo tree (git pollution).
# soldr honors a pre-exported CARGO_HOME, so pinning here collapses Cargo's
# registry + package caches back to ~/.cargo while leaving zccache's
# compilation cache under ~/.soldr/ (a separate, legitimate isolation layer).
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
```

This is the actual fix. The `.gitignore` entries below are belt-and-suspenders for stale leftovers.

**A2. Set `CARGO_BIN_ARTIFACT_WRAPPER_NO_CACHE=1` before `soldr cargo` (prevent double-soldr)**

In the front-door shim, in the non-bypass branch where it runs `soldr cargo`, export the env var so `cargo-bin-artifact-wrapper` (invoked as rustc-wrapper) skips its own soldr delegation and goes straight to rustc. The outer `soldr cargo` already handles zccache caching:

```bash
if [[ "${SOLDR_BYPASS:-0}" != "1" ]]; then
    SOLDR_BIN="$(command -v soldr 2>/dev/null || true)"
    # ... existing soldr resolution ...
    export SOLDR_REAL_CARGO="$REAL_CARGO"
    # Prevent double-soldr: cargo-bin-artifact-wrapper (the rustc-wrapper) would
    # otherwise invoke `soldr $rustc` per compile on top of the outer `soldr cargo`.
    # The outer soldr owns zccache; the inner wrapper should go straight to rustc.
    export CARGO_BIN_ARTIFACT_WRAPPER_NO_CACHE=1
    # ... existing daemon start + systemd-run ...
fi
```

Leave it **unset** in the `SOLDR_BYPASS=1` branch so the wrapper's own soldr delegation provides per-compile caching during recovery.

**A3. Add global `rustc-wrapper` to `~/.cargo/config.toml`**

Add to the `[build]` section (replacing the stale "Do not set rustc-wrapper here" comment):

```toml
[build]
jobs = 12
# rustc-wrapper is global so every repo gets bin-artifact installation without
# per-repo config. The front-door shim exports CARGO_BIN_ARTIFACT_WRAPPER_NO_CACHE=1
# when running inside soldr cargo, preventing a double-soldr; in SOLDR_BYPASS mode
# the wrapper's own soldr delegation provides per-compile caching.
rustc-wrapper = "cargo-bin-artifact-wrapper"
```

Rewrite the stale comment that claims "the cargo front door owns wrapper injection" — it's now actually true (the shim owns the NO_CACHE injection; the config owns the wrapper selection).

**A4. Add Cranelift backend to `~/.cargo/config.toml`**

Add to `[profile.dev]` (unstable feature — requires `RUSTFLAGS="-Zcodegen-backend=cranelift"` and the `rustc-codegen-cranelift` component on the toolchain):

```toml
# Cranelift codegen backend for faster dev builds. Requires the
# rustc-codegen-cranelift component: `rustup component add rustc-codegen-cranelift`.
# If the component is unavailable on the pinned toolchain, remove this block.
[profile.dev]
codegen-backend = "cranelift"
debug = 0
codegen-units = 256
split-debuginfo = "off"
incremental = false
opt-level = 0
```

Also need `[unstable]` section in `~/.cargo/config.toml`:
```toml
[unstable]
codegen-backend = true
```

**Verification step (before committing):** Run `rustup component list --toolchain 1.96.0 | grep cranelift`. If the component is not available on stable 1.96.0, remove the Cranelift block from config and update `contributing.md` to drop the mention instead. Cranelift may require nightly — verify before merging.

---

### B. axon repo-local changes

**B1. Remove `rustc-wrapper` from `.cargo/config.toml`, move config to `[env]`**

Current `.cargo/config.toml` sets `rustc-wrapper = "scripts/cargo-rustc-wrapper"`. Remove that line. Move the per-repo bin-artifact config (currently hardcoded in `scripts/cargo-rustc-wrapper`) into `[env]`:

```toml
[env]
AXON_ALLOW_FALLBACK_WEB_ASSETS = { value = "1", force = false }

# Per-repo config for the global cargo-bin-artifact-wrapper (rustc-wrapper is
# now set globally in ~/.cargo/config.toml). These env vars are inherited by
# the rustc-wrapper subprocess.
CARGO_BIN_ARTIFACT_LEGACY_PREFIX = "AXON_RUSTC_WRAPPER"
CARGO_BIN_ARTIFACT_BIN_DIR = { value = "bin", relative = true }
CARGO_BIN_ARTIFACT_ALLOWED_CRATES = "axon,axon-palette-tauri,axon_palette_tauri"
CARGO_BIN_ARTIFACT_NAME_MAP = "axon-palette-tauri=axon-palette,axon_palette_tauri=axon-palette"
CARGO_BIN_ARTIFACT_INSTALL_LATEST = "0"
CARGO_BIN_ARTIFACT_LOCAL_CRATE = "axon"
CARGO_BIN_ARTIFACT_LOCAL_BIN = { value = "/home/jmagar/.local/bin/axon" }
```

Note: `CARGO_BIN_ARTIFACT_REPO` is omitted — `cargo-bin-artifact-wrapper` defaults it to `git rev-parse --show-toplevel || pwd`, which is correct.

**B2. Delete `scripts/cargo-rustc-wrapper`**

The wrapper logic now lives entirely in the global `cargo-bin-artifact-wrapper` binary + `.cargo/config.toml [env]`. Deleting this script also removes the dead sccache fallback (review item #5). Check for references first and update them.

**B3. Add `.gitignore` entries for stale `.cargo` cache files**

Belt-and-suspenders for any leftover cache files from before the `CARGO_HOME` pin:

```
# Cargo/soldr-managed cache files (should not appear now that the front-door
# shim pins CARGO_HOME=~/.cargo, but ignore any stragglers).
.cargo/.global-cache
.cargo/.package-cache
.cargo/.package-cache-mutate
.cargo/registry/
```

Do NOT ignore `.cargo/config.toml` or `.cargo/audit.toml` — those are tracked.

**B4. Update `contributing.md`**

- Section "Global Cargo config" (line 28-31): update the description to match the new reality (soldr/zccache, global rustc-wrapper, Cranelift). The reference to `rmcp-template/docs/contributing/rust.md` can stay as the family canonical.
- Section "Local `.cargo/config.toml`" (line 33-48): update the example — it no longer shows just the xtask alias + Windows linker; it now also has the `[env]` block for bin-artifact config. Remove the line "All other settings (profile tuning, mold linker for Linux) are inherited from the global config" and replace with accurate text about rustc-wrapper being global and per-repo config living in `[env]`.

---

### C. New xtask check: `check-audit-ignore-sync`

**C1. Create `xtask/src/checks/audit_ignore_sync.rs`**

Parse both `.cargo/audit.toml` and `deny.toml`, extract the `ignore` lists under `[advisories]`, compare them as sets, fail with a clear diff if they drift. Canonical source is `deny.toml`.

Following the existing pattern (smallest check is `version_sync.rs` at 10 lines; `env_staged.rs` at 83 lines is a good complexity reference):

```rust
use anyhow::{Context, Result, bail};
use std::path::Path;

/// Verify .cargo/audit.toml and deny.toml advisory ignore lists are in sync.
/// deny.toml is canonical; .cargo/audit.toml must match it exactly (as a set).
pub fn check(root: &Path) -> Result<()> {
    let audit_path = root.join(".cargo/audit.toml");
    let deny_path = root.join("deny.toml");

    let audit_ignores = parse_ignore_list(&audit_path)
        .with_context(|| format!("failed to parse {}", audit_path.display()))?;
    let deny_ignores = parse_ignore_list(&deny_path)
        .with_context(|| format!("failed to parse {}", deny_path.display()))?;

    let audit_set: std::collections::BTreeSet<&str> = audit_ignores.iter().map(|s| s.as_str()).collect();
    let deny_set: std::collections::BTreeSet<&str> = deny_ignores.iter().map(|s| s.as_str()).collect();

    if audit_set == deny_set {
        return Ok(());
    }

    let only_audit: Vec<_> = audit_set.difference(&deny_set).collect();
    let only_deny: Vec<_> = deny_set.difference(&audit_set).collect();

    eprintln!("[audit-ignore-sync] advisory ignore lists drifted between deny.toml and .cargo/audit.toml");
    eprintln!("[audit-ignore-sync] deny.toml is canonical; .cargo/audit.toml must match.");
    if !only_audit.is_empty() {
        eprintln!("[audit-ignore-sync] in .cargo/audit.toml but not deny.toml: {}", only_audit.join(", "));
    }
    if !only_deny.is_empty() {
        eprintln!("[audit-ignore-sync] in deny.toml but not .cargo/audit.toml: {}", only_deny.join(", "));
    }
    bail!("advisory ignore list drift detected");
}

/// Extract RUSTSEC-xxx IDs from the `ignore = [...]` array under `[advisories]`.
/// Minimal parser: no toml dependency, just scan for quoted IDs.
fn parse_ignore_list(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let mut ids = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        for token in trimmed.split(',') {
            let token = token.trim().trim_start_matches('[').trim_end_matches(']').trim();
            if token.starts_with("RUSTSEC-") {
                let id = token.trim_matches('"');
                if id.starts_with("RUSTSEC-") {
                    ids.push(id.to_string());
                }
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
#[path = "audit_ignore_sync_tests.rs"]
mod tests;
```

**C2. Create `xtask/src/checks/audit_ignore_sync_tests.rs`**

Test the parser against both files' formats; test the set comparison. Following the sidecar convention.

**C3. Wire into `xtask/src/checks.rs` + `main.rs`**

- Add `pub mod audit_ignore_sync;` to `checks.rs`.
- Add `audit_ignore_sync::check(root)?;` to the `check()` aggregate function.
- Add `/// Verify .cargo/audit.toml and deny.toml advisory ignore lists match.` + `CheckAuditIgnoreSync,` to the `Command` enum in `main.rs`.
- Add `Command::CheckAuditIgnoreSync => checks::audit_ignore_sync::check(&root),` to the match.

---

### Verification

1. **Cranelift availability** — `rustup component list --toolchain 1.96.0 | grep cranelift`. If missing on stable, remove Cranelift from config + contributing.md and note it.
2. **Cache leak fixed** — `rm -rf .cargo/.global-cache .cargo/.package-cache .cargo/.package-cache-mutate .cargo/registry` then `cargo check --bin axon --quiet` then `git status .cargo/` should show no untracked cache files (because `CARGO_HOME` is now pinned).
3. **No double-soldr** — `cargo check --bin axon --quiet 2>&1 | grep -c soldr` or `soldr daemon status` before/after to confirm hits populate (entries should grow, not stay at 0).
4. **Wrapper still works** — `cargo build --bin axon` then `ls -la ~/.local/bin/axon` confirms the binary is still installed.
5. **xtask check** — `cargo xtask check-audit-ignore-sync` passes (lists are currently in sync).
6. **xtask aggregate** — `cargo xtask check` passes (includes the new check).
7. **Existing tests** — `cargo test --package xtask` passes.
8. **sccache gone** — `grep -rn sccache scripts/` returns nothing (script deleted).

### Out of scope (flagging for awareness)

- **Sibling repos `lab` and `cortex`** have the same `scripts/cargo-rustc-wrapper` pattern. They will benefit from the global `rustc-wrapper` once their `.cargo/config.toml` files are updated to use `[env]` (same as B1 for axon). This plan only touches axon. Updating lab/cortex is a follow-up — the global config change (A3) is backward-compatible because repos without `[env]` config simply don't install bins (the wrapper no-ops when `CARGO_BIN_ARTIFACT_LOCAL_CRATE` is unset and... actually it still copies to `$repo/bin/` by default). Need to verify: does the global wrapper affect repos that don't opt in? If so, may need `rustc-wrapper = ""` override in repos that don't want it, or change the wrapper default.
- **Cranelift on stable** — may require nightly toolchain. If so, this is a separate decision.
