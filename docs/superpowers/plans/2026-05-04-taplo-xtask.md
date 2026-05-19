# taplo + xtask Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add taplo for TOML formatting enforcement and introduce an `xtask` crate that replaces 5 bash enforcement scripts with portable Rust, enabling Windows-safe dev tooling.

**Architecture:** A new `xtask` workspace member exposes `cargo xtask <subcommand>` for all enforcement checks. taplo is wired into lefthook/CI as an external binary. Both integrate with the existing Justfile and lefthook pipeline without replacing them.

**Tech Stack:** Rust (anyhow, clap 4 derive, walkdir 2), taplo-cli, lefthook, GitHub Actions

---

## File Map

**Create:**
- `xtask/Cargo.toml`
- `xtask/src/main.rs`
- `xtask/src/checks.rs`
- `xtask/src/checks/no_mod_rs.rs`
- `xtask/src/checks/mcp_http.rs`
- `xtask/src/checks/env_staged.rs`
- `xtask/src/checks/unwraps.rs`
- `xtask/src/checks/claude_symlinks.rs`
- `.taplo.toml`

**Modify:**
- `Cargo.toml` — add `[workspace]` section
- `.cargo/config.toml` — add `[alias]` for `xtask`
- `Justfile` — add `taplo-check` and `taplo-fmt` tasks
- `lefthook.yml` — replace 5 shell script commands + add taplo
- `.github/workflows/ci.yml` — update 2 jobs + add `toml-fmt` + add `windows-check`

**Delete:**
- `scripts/check_no_mod_rs.sh`
- `scripts/check_mcp_http_only.sh`
- `scripts/check_env_staged.sh`
- `scripts/warn_new_unwraps.sh`
- `scripts/check_claude_symlinks.sh`

---

## Task 1: Workspace conversion + xtask scaffold

**Files:**
- Modify: `Cargo.toml`
- Modify: `.cargo/config.toml`
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Create: `xtask/src/checks.rs`

- [ ] **Step 1: Add `[workspace]` to root `Cargo.toml`**

Insert this block at the very top of `Cargo.toml`, before `[package]`:

```toml
[workspace]
members = ["xtask"]
resolver = "2"

```

- [ ] **Step 2: Add cargo alias to `.cargo/config.toml`**

Append to the existing `.cargo/config.toml`:

```toml

[alias]
xtask = "run -p xtask --"
```

- [ ] **Step 3: Create `xtask/Cargo.toml`**

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "xtask"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
walkdir = "2"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create `xtask/src/checks.rs`** (module root, no logic yet)

```rust
pub mod claude_symlinks;
pub mod env_staged;
pub mod mcp_http;
pub mod no_mod_rs;
pub mod unwraps;
```

- [ ] **Step 5: Create `xtask/src/main.rs`** (skeleton — full dispatch added in Task 8)

```rust
mod checks;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "xtask", about = "Axon build and enforcement tasks")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Fail on any mod.rs file
    CheckNoModRs,
    /// Verify MCP transport modes are all supported
    CheckMcpHttp,
    /// Block staged .env files that may contain secrets
    CheckEnvStaged,
    /// Warn on new .unwrap()/.expect( in staged non-test Rust (warn-only)
    CheckUnwraps,
    /// Verify AGENTS.md + GEMINI.md symlinks exist alongside every CLAUDE.md
    CheckClaudeSymlinks,
    /// Run all checks
    Check,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be inside the workspace")
        .to_path_buf()
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = workspace_root();

    match args.cmd {
        Cmd::CheckNoModRs => checks::no_mod_rs::check(&root),
        Cmd::CheckMcpHttp => checks::mcp_http::check(&root),
        Cmd::CheckEnvStaged => checks::env_staged::run(),
        Cmd::CheckUnwraps => checks::unwraps::run(),
        Cmd::CheckClaudeSymlinks => checks::claude_symlinks::check(&root),
        Cmd::Check => {
            checks::no_mod_rs::check(&root)?;
            checks::mcp_http::check(&root)?;
            checks::env_staged::run()?;
            checks::unwraps::run()?;
            checks::claude_symlinks::check(&root)?;
            println!("All checks passed.");
            Ok(())
        }
    }
}
```

- [ ] **Step 6: Create stub implementations for each check module**

Create `xtask/src/checks/no_mod_rs.rs`:
```rust
use anyhow::Result;
use std::path::Path;

pub fn check(_root: &Path) -> Result<()> {
    todo!()
}
```

Create `xtask/src/checks/mcp_http.rs`:
```rust
use anyhow::Result;
use std::path::Path;

pub fn check(_root: &Path) -> Result<()> {
    todo!()
}
```

Create `xtask/src/checks/env_staged.rs`:
```rust
use anyhow::Result;

pub fn check(_staged_files: &[&str]) -> Result<()> {
    todo!()
}

pub fn run() -> Result<()> {
    todo!()
}
```

Create `xtask/src/checks/unwraps.rs`:
```rust
use anyhow::Result;

pub fn count(_diff_output: &str) -> usize {
    todo!()
}

pub fn run() -> Result<()> {
    todo!()
}
```

Create `xtask/src/checks/claude_symlinks.rs`:
```rust
use anyhow::Result;
use std::path::Path;

pub fn check(_root: &Path) -> Result<()> {
    todo!()
}
```

- [ ] **Step 7: Verify workspace builds**

```bash
cargo check -p xtask --locked
```

Expected: compiles (stubs will compile; `todo!()` is valid Rust).

- [ ] **Step 8: Commit scaffold**

```bash
git add Cargo.toml .cargo/config.toml xtask/
git commit -m "chore: scaffold xtask workspace crate"
```

---

## Task 2: taplo config + just tasks

**Files:**
- Create: `.taplo.toml`
- Modify: `Justfile`

- [ ] **Step 1: Create `.taplo.toml`**

```toml
[formatting]
indent_string = "  "
reorder_keys = false
align_entries = false
column_width = 100
```

- [ ] **Step 2: Run taplo on existing TOML files to see if any need reformatting**

```bash
taplo fmt --check
```

If it exits non-zero, run `taplo fmt` to fix, then verify with `taplo fmt --check` again.

- [ ] **Step 3: Add taplo tasks to `Justfile`**

Append after the `fix-all` recipe:

```just
# TOML formatting
taplo-check:
    taplo fmt --check

taplo-fmt:
    taplo fmt
```

- [ ] **Step 4: Verify just tasks work**

```bash
just taplo-check
```

Expected: exits 0 (TOML files are already formatted after Step 2).

- [ ] **Step 5: Commit**

```bash
git add .taplo.toml Justfile
git add -u  # picks up any TOML files reformatted in Step 2
git commit -m "chore: add taplo TOML formatter config and just tasks"
```

---

## Task 3: check-no-mod-rs

**Files:**
- Modify: `xtask/src/checks/no_mod_rs.rs`

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/checks/no_mod_rs.rs` entirely:

```rust
use anyhow::Result;
use std::path::Path;
use walkdir::WalkDir;

pub fn check(root: &Path) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn passes_with_no_mod_rs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("lib.rs"), "").unwrap();
        assert!(check(dir.path()).is_ok());
    }

    #[test]
    fn fails_on_mod_rs_in_subdir() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("foo");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("mod.rs"), "").unwrap();
        assert!(check(dir.path()).is_err());
    }

    #[test]
    fn ignores_target_dir() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target").join("debug");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("mod.rs"), "").unwrap();
        assert!(check(dir.path()).is_ok());
    }

    #[test]
    fn ignores_dot_git() {
        let dir = TempDir::new().unwrap();
        let git = dir.path().join(".git").join("hooks");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("mod.rs"), "").unwrap();
        assert!(check(dir.path()).is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask no_mod_rs -- --nocapture
```

Expected: all 4 tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `check`**

Replace the `todo!()` body only:

```rust
pub fn check(root: &Path) -> Result<()> {
    let offenders: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !matches!(name, ".git" | "target" | "node_modules" | ".cache")
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "mod.rs")
        .map(|e| e.path().to_path_buf())
        .collect();

    if offenders.is_empty() {
        println!("OK: no mod.rs files found.");
        return Ok(());
    }

    eprintln!("ERROR: legacy Rust module roots detected (mod.rs is disallowed):");
    for path in &offenders {
        eprintln!("  {}", path.display());
    }
    eprintln!("\nUse modern module style: foo.rs + foo/*.rs");
    anyhow::bail!("{} mod.rs file(s) found", offenders.len())
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p xtask no_mod_rs -- --nocapture
```

Expected: all 4 tests pass.

- [ ] **Step 5: Smoke-test against the actual repo**

```bash
cargo xtask check-no-mod-rs
```

Expected: `OK: no mod.rs files found.`

- [ ] **Step 6: Commit**

```bash
git add xtask/src/checks/no_mod_rs.rs
git commit -m "feat(xtask): implement check-no-mod-rs"
```

---

## Task 4: check-mcp-http

**Files:**
- Modify: `xtask/src/checks/mcp_http.rs`

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/checks/mcp_http.rs` entirely:

```rust
use anyhow::{Context, Result};
use std::path::Path;

const MCP_CMD: &str = "crates/cli/commands/mcp.rs";
const CLI_CONFIG: &str = "crates/core/config/cli.rs";
const BUILD_CONFIG: &str = "crates/core/config/parse.rs";

pub fn check(root: &Path) -> Result<()> {
    todo!()
}

fn check_file(root: &Path, rel: &str, patterns: &[(&str, &str)]) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_root(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, content).unwrap();
        }
        dir
    }

    #[test]
    fn passes_with_all_patterns() {
        let dir = make_root(&[
            (MCP_CMD, "run_http_server()\nrun_stdio_server()\nBoth"),
            (CLI_CONFIG, "transport: Option<McpTransport>"),
            (BUILD_CONFIG, "AXON_MCP_TRANSPORT"),
        ]);
        assert!(check(dir.path()).is_ok());
    }

    #[test]
    fn fails_missing_http_transport() {
        let dir = make_root(&[
            (MCP_CMD, "run_stdio_server()\nBoth"),
            (CLI_CONFIG, "transport: Option<McpTransport>"),
            (BUILD_CONFIG, "AXON_MCP_TRANSPORT"),
        ]);
        let err = check(dir.path()).unwrap_err();
        assert!(err.to_string().contains("HTTP transport"));
    }

    #[test]
    fn fails_missing_stdio_transport() {
        let dir = make_root(&[
            (MCP_CMD, "run_http_server()\nBoth"),
            (CLI_CONFIG, "transport: Option<McpTransport>"),
            (BUILD_CONFIG, "AXON_MCP_TRANSPORT"),
        ]);
        assert!(check(dir.path()).is_err());
    }

    #[test]
    fn fails_missing_file() {
        let dir = TempDir::new().unwrap();
        assert!(check(dir.path()).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask mcp_http -- --nocapture
```

Expected: all 4 tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `check_file` and `check`**

Replace both `todo!()` bodies:

```rust
pub fn check(root: &Path) -> Result<()> {
    check_file(root, MCP_CMD, &[
        ("run_http_server(", "MCP CLI must support HTTP transport"),
        ("run_stdio_server(", "MCP CLI must support stdio transport"),
        ("Both", "MCP CLI must support both transports concurrently"),
    ])?;
    check_file(root, CLI_CONFIG, &[
        ("transport: Option<McpTransport>", "MCP CLI must expose --transport"),
    ])?;
    check_file(root, BUILD_CONFIG, &[
        ("AXON_MCP_TRANSPORT", "MCP transport env override missing"),
    ])?;
    println!("OK: MCP CLI supports stdio, http, and both transport modes.");
    Ok(())
}

fn check_file(root: &Path, rel: &str, patterns: &[(&str, &str)]) -> Result<()> {
    let path = root.join(rel);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("missing {rel}"))?;
    for (pattern, msg) in patterns {
        if !content.contains(pattern) {
            anyhow::bail!("ERROR: {msg} in {rel}");
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p xtask mcp_http -- --nocapture
```

Expected: all 4 tests pass.

- [ ] **Step 5: Smoke-test against the actual repo**

```bash
cargo xtask check-mcp-http
```

Expected: `OK: MCP CLI supports stdio, http, and both transport modes.`

- [ ] **Step 6: Commit**

```bash
git add xtask/src/checks/mcp_http.rs
git commit -m "feat(xtask): implement check-mcp-http"
```

---

## Task 5: check-env-staged

**Files:**
- Modify: `xtask/src/checks/env_staged.rs`

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/checks/env_staged.rs` entirely:

```rust
use anyhow::{Context, Result};
use std::process::Command;

/// Pure logic — takes the list of staged filenames as input.
/// Called by `run()` with real git output; called directly by tests.
pub fn check(staged_files: &[&str]) -> Result<()> {
    todo!()
}

pub fn run() -> Result<()> {
    todo!()
}

fn is_blocked(basename: &str) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_env_example() {
        assert!(check(&[".env.example"]).is_ok());
    }

    #[test]
    fn blocks_dot_env() {
        assert!(check(&[".env"]).is_err());
    }

    #[test]
    fn blocks_dot_env_local() {
        assert!(check(&[".env.local"]).is_err());
    }

    #[test]
    fn blocks_dot_env_production() {
        assert!(check(&[".env.production"]).is_err());
    }

    #[test]
    fn blocks_services_env() {
        assert!(check(&["services.env"]).is_err());
    }

    #[test]
    fn blocks_nested_dot_env() {
        assert!(check(&["config/.env"]).is_err());
    }

    #[test]
    fn allows_unrelated_files() {
        assert!(check(&["src/main.rs", "Cargo.toml", "README.md"]).is_ok());
    }

    #[test]
    fn empty_staged_list_passes() {
        assert!(check(&[]).is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask env_staged -- --nocapture
```

Expected: all 8 tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `is_blocked`, `check`, and `run`**

Replace all `todo!()` bodies:

```rust
pub fn check(staged_files: &[&str]) -> Result<()> {
    let violations: Vec<_> = staged_files
        .iter()
        .filter(|f| {
            let base = std::path::Path::new(f)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            is_blocked(base)
        })
        .collect();

    if violations.is_empty() {
        return Ok(());
    }

    eprintln!("[env-guard] BLOCKED — staged file(s) may contain secrets:");
    for v in &violations {
        eprintln!("  {v}");
    }
    eprintln!("[env-guard] Unstage with: git restore --staged <file>");
    eprintln!("[env-guard] Only .env.example should ever be committed.");
    anyhow::bail!("{} secret file(s) staged", violations.len())
}

pub fn run() -> Result<()> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .context("failed to run git diff --cached")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<&str> = stdout.lines().collect();
    check(&files)
}

fn is_blocked(basename: &str) -> bool {
    match basename {
        ".env.example" => false,
        name if name == ".env" || name.starts_with(".env.") => true,
        name if name == "services.env" || (name.ends_with(".env") && name != ".env.example") => true,
        _ => false,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p xtask env_staged -- --nocapture
```

Expected: all 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add xtask/src/checks/env_staged.rs
git commit -m "feat(xtask): implement check-env-staged"
```

---

## Task 6: check-unwraps

**Files:**
- Modify: `xtask/src/checks/unwraps.rs`

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/checks/unwraps.rs` entirely:

```rust
use anyhow::{Context, Result};
use std::process::Command;

/// Count new .unwrap()/.expect( calls in the added lines of a diff.
/// Pure function — takes diff text as input for testability.
pub fn count(diff_output: &str) -> usize {
    todo!()
}

pub fn run() -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unwrap_on_added_line() {
        let diff = "+    let x = foo.unwrap();\n";
        assert_eq!(count(diff), 1);
    }

    #[test]
    fn counts_expect_on_added_line() {
        let diff = "+    let x = foo.expect(\"msg\");\n";
        assert_eq!(count(diff), 1);
    }

    #[test]
    fn ignores_removed_lines() {
        let diff = "-    let x = foo.unwrap();\n";
        assert_eq!(count(diff), 0);
    }

    #[test]
    fn ignores_context_lines() {
        let diff = "     let x = foo.unwrap();\n";
        assert_eq!(count(diff), 0);
    }

    #[test]
    fn ignores_diff_header() {
        let diff = "+++ b/src/lib.rs\n+    foo.unwrap()\n";
        assert_eq!(count(diff), 1);
    }

    #[test]
    fn counts_multiple() {
        let diff = "+    a.unwrap()\n+    b.expect(\"x\")\n+    c.unwrap()\n";
        assert_eq!(count(diff), 3);
    }

    #[test]
    fn empty_diff_returns_zero() {
        assert_eq!(count(""), 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask unwraps -- --nocapture
```

Expected: all 7 tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `count` and `run`**

Replace both `todo!()` bodies:

```rust
pub fn count(diff_output: &str) -> usize {
    diff_output
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .filter(|l| l.contains(".unwrap()") || l.contains(".expect("))
        .count()
}

pub fn run() -> Result<()> {
    let staged = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR", "--", "*.rs"])
        .output()
        .context("failed to list staged Rust files")?;
    let staged_str = String::from_utf8_lossy(&staged.stdout);

    let non_test_files: Vec<&str> = staged_str
        .lines()
        .filter(|f| {
            !f.contains("/test") && !f.contains("/tests")
                && !f.ends_with("_test.rs")
                && !f.ends_with("tests.rs")
        })
        .collect();

    if non_test_files.is_empty() {
        return Ok(());
    }

    let mut total = 0usize;
    let mut detail: Vec<String> = Vec::new();

    for file in &non_test_files {
        let diff = Command::new("git")
            .args(["diff", "--cached", "--", file])
            .output()
            .context("failed to run git diff --cached")?;
        let n = count(&String::from_utf8_lossy(&diff.stdout));
        if n > 0 {
            total += n;
            detail.push(format!("  +{n}  {file}"));
        }
    }

    if total > 0 {
        eprintln!();
        eprintln!("[unwrap-warn] WARNING: {total} new .unwrap()/.expect() call(s) in staged non-test Rust");
        for line in &detail {
            eprintln!("{line}");
        }
        eprintln!("[unwrap-warn] Prefer '?' propagation or explicit error handling in production code.");
        eprintln!("[unwrap-warn] (Warning only — commit proceeds)");
    }

    Ok(()) // always succeeds — warn-only
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p xtask unwraps -- --nocapture
```

Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add xtask/src/checks/unwraps.rs
git commit -m "feat(xtask): implement check-unwraps (warn-only)"
```

---

## Task 7: check-claude-symlinks

**Files:**
- Modify: `xtask/src/checks/claude_symlinks.rs`

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/checks/claude_symlinks.rs` entirely:

```rust
use anyhow::Result;
use std::path::Path;
use walkdir::WalkDir;

pub fn check(root: &Path) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    #[cfg(unix)]
    fn passes_with_valid_symlinks() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "").unwrap();
        symlink("CLAUDE.md", dir.path().join("AGENTS.md")).unwrap();
        symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
        assert!(check(dir.path()).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn fails_missing_agents_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "").unwrap();
        symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
        assert!(check(dir.path()).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn fails_wrong_symlink_target() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "").unwrap();
        fs::write(dir.path().join("OTHER.md"), "").unwrap();
        symlink("OTHER.md", dir.path().join("AGENTS.md")).unwrap();
        symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
        assert!(check(dir.path()).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn fails_agents_md_is_regular_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "").unwrap();
        fs::write(dir.path().join("AGENTS.md"), "").unwrap();
        symlink("CLAUDE.md", dir.path().join("GEMINI.md")).unwrap();
        assert!(check(dir.path()).is_err());
    }

    #[test]
    fn passes_when_no_claude_md_exists() {
        let dir = TempDir::new().unwrap();
        assert!(check(dir.path()).is_ok());
    }

    #[test]
    fn ignores_target_dir() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target").join("debug");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("CLAUDE.md"), "").unwrap();
        // No symlinks in target/ — should still pass (target is excluded)
        assert!(check(dir.path()).is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask claude_symlinks -- --nocapture
```

Expected: all applicable tests panic with `not yet implemented`.

- [ ] **Step 3: Implement `check`**

Replace the `todo!()` body:

```rust
pub fn check(root: &Path) -> Result<()> {
    let mut failures: Vec<String> = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !matches!(name, ".git" | "target" | "node_modules" | ".cache" | ".next")
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name() == "CLAUDE.md")
    {
        let dir = entry.path().parent().unwrap();
        for sibling in ["AGENTS.md", "GEMINI.md"] {
            let link_path = dir.join(sibling);
            match std::fs::read_link(&link_path) {
                Ok(dest) if dest == Path::new("CLAUDE.md") => {}
                Ok(dest) => failures.push(format!(
                    "WRONG TARGET: {} -> {} (expected -> CLAUDE.md)",
                    link_path.display(),
                    dest.display()
                )),
                Err(_) if link_path.exists() => failures.push(format!(
                    "NOT A SYMLINK: {} (must be: ln -sf CLAUDE.md {sibling})",
                    link_path.display()
                )),
                Err(_) => failures.push(format!(
                    "MISSING: {} (should be a symlink to CLAUDE.md)",
                    link_path.display()
                )),
            }
        }
    }

    if failures.is_empty() {
        println!("[claude-symlinks] OK — all CLAUDE.md files have valid AGENTS.md + GEMINI.md symlinks");
        return Ok(());
    }
    for f in &failures {
        eprintln!("[claude-symlinks] {f}");
    }
    eprintln!("\n[claude-symlinks] Fix with: ln -sf CLAUDE.md AGENTS.md && ln -sf CLAUDE.md GEMINI.md");
    anyhow::bail!("{} symlink issue(s) found", failures.len())
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p xtask claude_symlinks -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 5: Smoke-test against the actual repo**

```bash
cargo xtask check-claude-symlinks
```

Expected: `[claude-symlinks] OK — all CLAUDE.md files have valid AGENTS.md + GEMINI.md symlinks`

- [ ] **Step 6: Run the full xtask check suite**

```bash
cargo xtask check
```

Expected: all checks pass, `All checks passed.`

- [ ] **Step 7: Run all xtask tests**

```bash
cargo test -p xtask
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add xtask/src/checks/claude_symlinks.rs
git commit -m "feat(xtask): implement check-claude-symlinks"
```

---

## Task 8: lefthook integration + shell script removal

**Files:**
- Modify: `lefthook.yml`
- Delete: `scripts/check_no_mod_rs.sh`, `scripts/check_mcp_http_only.sh`, `scripts/check_env_staged.sh`, `scripts/warn_new_unwraps.sh`, `scripts/check_claude_symlinks.sh`

- [ ] **Step 1: Update `lefthook.yml`**

Replace the five shell-script commands with xtask equivalents and add the taplo command. The updated `pre-commit` block:

```yaml
pre-commit:
  parallel: true
  commands:
    env-guard:
      run: cargo xtask check-env-staged
    claude-symlinks:
      run: cargo xtask check-claude-symlinks
    mcp-http-only:
      glob: "**/*.rs"
      run: cargo xtask check-mcp-http
    no-mod-rs:
      glob: "**/*.rs"
      run: cargo xtask check-no-mod-rs
    unwrap-warn:
      glob: "**/*.rs"
      run: cargo xtask check-unwraps
    skills-ref:
      glob: "skills/**"
      run: ./scripts/validate_skills_ref.sh
    mcp-schema-doc:
      glob: "crates/mcp/schema.rs"
      run: python3 scripts/generate_mcp_schema_doc.py && git add docs/MCP-TOOL-SCHEMA.md
    monolith:
      run: >
        if [ -f "scripts/enforce_monoliths.py" ]; then
          python3 "scripts/enforce_monoliths.py" --staged;
        elif [ -f "$HOME/.claude/hooks/enforce_monoliths.py" ]; then
          python3 "$HOME/.claude/hooks/enforce_monoliths.py" --staged;
        else
          echo "ERROR enforce_monoliths.py not found in scripts/ or ~/.claude/hooks/" >&2; exit 1;
        fi
    taplo:
      glob: "**/*.toml"
      run: taplo fmt --check
    rustfmt:
      glob: "**/*.rs"
      run: cargo fmt -- --check
    clippy:
      glob: "**/*.{rs,toml}"
      run: cargo clippy --all-targets --locked -- -D warnings
    check:
      glob: "**/*.{rs,toml}"
      run: cargo check --all-targets --locked
    test:
      glob: "**/*.{rs,toml}"
      run: cargo test --all --locked
```

- [ ] **Step 2: Delete the five shell scripts**

```bash
git rm scripts/check_no_mod_rs.sh \
       scripts/check_mcp_http_only.sh \
       scripts/check_env_staged.sh \
       scripts/warn_new_unwraps.sh \
       scripts/check_claude_symlinks.sh
```

- [ ] **Step 3: Verify lefthook runs cleanly (dry run)**

```bash
lefthook run pre-commit --no-stdin
```

Expected: all commands exit 0 (or warnings for unwrap-warn, which is non-fatal).

- [ ] **Step 4: Commit**

```bash
git add lefthook.yml
git commit -m "chore: replace bash enforcement scripts with cargo xtask in lefthook"
```

---

## Task 9: CI integration

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Update the `no-mod-rs` job**

Find this step in the `no-mod-rs` job:
```yaml
      - name: Enforce modern Rust module style (no mod.rs)
        run: ./scripts/check_no_mod_rs.sh
```

Replace with:
```yaml
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.94.0
      - uses: Swatinem/rust-cache@v2
      - name: Enforce modern Rust module style (no mod.rs)
        run: cargo xtask check-no-mod-rs
```

- [ ] **Step 2: Update the `mcp-transport-modes` job**

Find this step in the `mcp-transport-modes` job:
```yaml
      - name: Enforce MCP CLI transport mode support
        run: ./scripts/check_mcp_http_only.sh
```

Replace with:
```yaml
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.94.0
      - uses: Swatinem/rust-cache@v2
      - name: Enforce MCP CLI transport mode support
        run: cargo xtask check-mcp-http
```

- [ ] **Step 3: Add `toml-fmt` job**

Add this new job after the `fmt` job:

```yaml
  toml-fmt:
    name: toml-fmt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: taiki-e/install-action@7bc99eee1f1b8902a125006cf790a1f4c8461e63  # v2
        with:
          tool: taplo-cli
      - name: taplo fmt check
        run: taplo fmt --check
```

- [ ] **Step 4: Add `windows-check` job**

Add this new job after the `toml-fmt` job:

```yaml
  windows-check:
    name: windows-check
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.94.0
      - uses: Swatinem/rust-cache@v2
      # Scope to xtask only: spider's Chrome native deps require additional
      # Windows setup. Full axon Windows build tracked separately.
      - name: cargo check xtask (Windows)
        run: cargo check -p xtask --locked
      - name: xtask check-no-mod-rs (Windows)
        run: cargo xtask check-no-mod-rs
      - name: xtask check-mcp-http (Windows)
        run: cargo xtask check-mcp-http
```

- [ ] **Step 5: Verify CI YAML is valid**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: migrate enforcement checks to cargo xtask, add toml-fmt and windows-check jobs"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run the full workspace test suite**

```bash
cargo test --workspace --locked
```

Expected: all tests pass including xtask unit tests.

- [ ] **Step 2: Run `cargo clippy` across the workspace**

```bash
cargo clippy --all-targets --locked -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Run taplo check**

```bash
just taplo-check
```

Expected: exits 0.

- [ ] **Step 4: Run `cargo xtask check` end-to-end**

```bash
cargo xtask check
```

Expected: `All checks passed.`

- [ ] **Step 5: Bump patch version**

In `Cargo.toml`, increment the patch version (e.g. `1.2.0` → `1.2.1`). Then add a CHANGELOG entry.

- [ ] **Step 6: Final commit and push**

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump version for taplo + xtask tooling"
rtk git push
```
