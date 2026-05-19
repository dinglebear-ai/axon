# taplo + xtask Design

**Date:** 2026-05-04
**Branch:** bd-1d2.1/config-system-cleanup (to be implemented on a new branch)

## Goal

1. Add taplo for TOML formatting enforcement.
2. Introduce an `xtask` crate that replaces the 5 bash enforcement scripts with portable Rust, enabling the `axon` binary to support Windows developers.

## taplo

### Config
`.taplo.toml` at repo root:
- 2-space indent
- `reorder_keys = false` (preserve intentional ordering in Cargo.toml)
- `align_entries = false`

### Install
- Local: `cargo binstall taplo-cli` (documented in `scripts/dev-setup.sh`)
- CI: `taiki-e/install-action` — same pattern as `cargo-audit` and `cargo-deny`

### just tasks
```
taplo-check   # taplo fmt --check (CI + pre-commit)
taplo-fmt     # taplo fmt (fix)
```

### lefthook
New `taplo` pre-commit command, glob `**/*.toml`, runs `taplo fmt --check`.

### CI
New `toml-fmt` job: install taplo via `taiki-e/install-action`, run `taplo fmt --check`.

---

## xtask Crate

### Workspace conversion
Root `Cargo.toml` gains a `[workspace]` section:
```toml
[workspace]
members = ["xtask"]
resolver = "2"
```
The root package (`axon`) remains unchanged — it becomes the implicit workspace root member.

### xtask crate layout
```
xtask/
├── Cargo.toml       # anyhow, clap, walkdir
└── src/
    ├── main.rs      # clap dispatch
    ├── checks.rs    # mod declarations
    └── checks/
        ├── no_mod_rs.rs
        ├── mcp_http.rs
        ├── env_staged.rs
        ├── unwraps.rs
        └── claude_symlinks.rs
```

Follows the project's no-mod-rs rule (module root in `checks.rs`, submodules in `checks/`).

### Subcommands

| Subcommand | Replaces | Exit behavior |
|---|---|---|
| `cargo xtask check-no-mod-rs` | `check_no_mod_rs.sh` | fail (exit 1) on any `mod.rs` found |
| `cargo xtask check-mcp-http` | `check_mcp_http_only.sh` | fail if MCP transport patterns missing |
| `cargo xtask check-env-staged` | `check_env_staged.sh` | fail if `.env` files are staged |
| `cargo xtask check-unwraps` | `warn_new_unwraps.sh` | warn-only (always exit 0) |
| `cargo xtask check-claude-symlinks` | `check_claude_symlinks.sh` | fail on missing/broken symlinks |
| `cargo xtask check` | — | runs all five in sequence |

### Implementation notes

- `check-no-mod-rs`: walkdir from repo root, skip `.git`/`target`/`node_modules`, fail on any file named `mod.rs`.
- `check-mcp-http`: `std::fs::read_to_string` the three target files, check for required string patterns.
- `check-env-staged`: `git diff --cached --name-only` via `std::process::Command`, match filenames against the same exemption rules as the shell script (`.env.example` allowed, everything else blocked).
- `check-unwraps`: `git diff --cached` via `std::process::Command`, parse `+` lines, count `.unwrap()` / `.expect(` in non-test Rust files, print summary (warn-only).
- `check-claude-symlinks`: walkdir for `CLAUDE.md` files (skip `.git`/`target`/`node_modules`), verify `AGENTS.md` and `GEMINI.md` siblings are symlinks pointing to `CLAUDE.md` via `std::fs::read_link`.

**Windows symlink caveat:** `check-claude-symlinks` requires Developer Mode or admin privileges on Windows (same limitation the shell script would have). Document in xtask README and `dev-setup.sh`.

---

## Integration Changes

### Justfile
Replace shell script invocations with `cargo xtask` calls where applicable. `verify` / `fix` / `precommit` recipes stay structurally the same.

### lefthook
Replace the 5 `run: ./scripts/check_*.sh` entries with `cargo xtask <subcommand>`. `validate_skills_ref` stays as a shell script (calls external `skills-ref` CLI — no logic to migrate).

### CI
Each of the 5 affected jobs gets `run: cargo xtask <subcommand>` instead of the shell script. No changes to structure or triggers.

### Cleanup
Delete the 5 migrated `.sh` files after all integration points are updated. The following stay:
- `validate_skills_ref.sh`
- `check_shell_completions.sh`
- `test-mcp-oauth-protection.sh`
- `test-mcp-tools-mcporter.sh`
- `dev-setup.sh`
- `install-git-hooks.sh`

---

## Windows CI Job

New `windows-check` CI job on `windows-latest`:
- `dtolnay/rust-toolchain@stable` (1.94.0)
- `Swatinem/rust-cache@v2`
- `cargo check --locked`
- `cargo xtask check-no-mod-rs` + `cargo xtask check-mcp-http` (the two checks that don't need git staged state or symlinks — suitable for CI without a commit context)

The git-diff-dependent checks (`check-env-staged`, `check-unwraps`) and `check-claude-symlinks` are lefthook/pre-commit concerns, not CI jobs, so they're excluded from the Windows CI job.

---

## Out of Scope

- Python scripts (`enforce_monoliths.py`, `generate_mcp_schema_doc.py`, etc.) — not migrated in this pass
- `validate_skills_ref.sh` — calls external CLI, no logic to port
- Integration test scripts — Linux CI only, not worth porting
