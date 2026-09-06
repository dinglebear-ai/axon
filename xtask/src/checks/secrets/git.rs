use super::*;

pub(super) fn staged_files(root: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=AMR"])
        .current_dir(root)
        .output()
        .context("failed to run git diff --cached")?;
    if !out.status.success() {
        bail!(
            "git diff --cached failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8(out.stdout)
        .context("git diff output is not UTF-8")?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

pub(super) fn staged_content(root: &Path, path: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["show", &format!(":{path}")])
        .current_dir(root)
        .output()
        .context("failed to run git show")?;
    if !out.status.success() {
        bail!("git show :{path} failed");
    }
    if out.stdout.contains(&0u8) {
        bail!("binary file");
    }
    String::from_utf8(out.stdout).context("file is not UTF-8")
}

pub(super) fn should_skip_path(path: &str) -> bool {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if SKIP_EXTENSIONS.contains(&ext.as_str()) {
        return true;
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    SKIP_NAME_PATTERNS.contains(&name)
}
