use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Returns true if the given basename is a staged-file violation.
///
/// Rules:
/// - `.env.example` → allowed (template file, committed by design)
/// - basename equals `.env` or starts with `.env.` → blocked
///   (`.env`, `.env.local`, `.env.production`, ...)
/// - basename equals `services.env` → blocked
/// - basename ends with `.env` (e.g., `prod.env`) → blocked
/// - everything else → allowed
fn is_violation(filename: &str) -> bool {
    if filename == ".env.example" {
        return false;
    }
    if filename == ".env" || filename.starts_with(".env.") {
        return true;
    }
    if filename == "services.env" {
        return true;
    }
    if filename.ends_with(".env") {
        return true;
    }
    false
}

pub fn check(root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(root)
        .output()
        .context("failed to invoke `git diff --cached --name-only`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`git diff --cached --name-only` failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("`git diff --cached --name-only` returned non-UTF-8 output")?;

    let mut offenders: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        let basename = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path);
        if is_violation(basename) {
            offenders.push(path.to_string());
        }
    }

    if offenders.is_empty() {
        return Ok(());
    }

    println!("[env-guard] BLOCKED — staged file(s) may contain secrets:");
    for f in &offenders {
        println!("  {f}");
    }
    println!("[env-guard] Unstage with: git restore --staged <file>");
    println!("[env-guard] Only .env.example should ever be committed.");
    bail!("env-guard blocked {} staged file(s)", offenders.len());
}

#[cfg(test)]
mod tests {
    use super::is_violation;

    #[test]
    fn is_violation_blocks_dot_env() {
        assert!(is_violation(".env"));
    }

    #[test]
    fn is_violation_allows_dot_env_example() {
        assert!(!is_violation(".env.example"));
    }

    #[test]
    fn is_violation_blocks_dot_env_local() {
        assert!(is_violation(".env.local"));
    }

    #[test]
    fn is_violation_blocks_dot_env_production() {
        assert!(is_violation(".env.production"));
    }

    #[test]
    fn is_violation_blocks_services_env() {
        assert!(is_violation("services.env"));
    }

    #[test]
    fn is_violation_blocks_arbitrary_dot_env_suffix() {
        assert!(is_violation("prod.env"));
        assert!(is_violation("staging.env"));
    }

    #[test]
    fn is_violation_allows_unrelated_files() {
        assert!(!is_violation("Cargo.toml"));
        assert!(!is_violation("src/main.rs"));
        assert!(!is_violation("README.md"));
        assert!(!is_violation("envoy.yaml"));
    }
}
