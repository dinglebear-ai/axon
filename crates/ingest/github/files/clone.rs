use anyhow::{Result, bail};

use crate::crates::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};

use super::super::GitHubCommonFields;
use crate::crates::core::logging::log_warn;

pub(super) fn stderr_has_auth_or_permission_failure(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "authentication failed",
        "permission denied",
        "denied to",
        "access denied",
        "repository not found",
        "not found",
        "could not read username",
        "invalid username or token",
        "support for password authentication was removed",
    ]
    .iter()
    .any(|needle| stderr.contains(needle))
}

pub fn should_retry_unauthenticated_clone(common: &GitHubCommonFields, stderr: &str) -> bool {
    match common.is_private {
        Some(true) => false,
        Some(false) | None => !stderr_has_auth_or_permission_failure(stderr),
    }
}

pub fn sanitized_git_stderr(stderr: &[u8], token: Option<&str>) -> String {
    let mut stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if let Some(token) = token
        && !token.is_empty()
    {
        stderr = stderr.replace(token, "[redacted]");
    }
    stderr
}

/// Run `git clone --depth=1` into a temp directory with SSRF validation and timeout.
pub(super) async fn clone_repo(
    common: &GitHubCommonFields,
    branch: &str,
    token: Option<&str>,
) -> Result<tempfile::TempDir> {
    use crate::crates::core::http::validate_url;

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();
    let clone_url = format!("https://github.com/{}/{}.git", common.owner, common.name);

    validate_url(&clone_url)?;

    let base_args = [
        "clone",
        "--depth=1",
        "--branch",
        branch,
        "--single-branch",
        "--",
        &clone_url,
        &tmp_path,
    ];

    let ctx = format!("git clone {}", common.repo_slug);

    if let Some(t) = token {
        let mut command = tokio::process::Command::new("git");
        command
            .args(base_args)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env("GIT_CONFIG_VALUE_0", format!("Authorization: token {t}"));
        let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, &ctx).await?;

        if output.status.success() {
            return Ok(tmp);
        }

        let stderr = sanitized_git_stderr(&output.stderr, Some(t));
        if !should_retry_unauthenticated_clone(common, &stderr) {
            bail!(
                "authenticated git clone failed for {} (exit {}): {}",
                common.repo_slug,
                output.status,
                stderr
            );
        }

        log_warn(&format!(
            "command=ingest_github auth_clone_failed repo={}/{} retrying_unauthenticated",
            common.owner, common.name
        ));
        let _ = tokio::fs::remove_dir_all(tmp.path()).await;
        tokio::fs::create_dir_all(tmp.path()).await.map_err(|e| {
            anyhow::anyhow!("failed to recreate tmp dir for unauthenticated retry: {e}")
        })?;
    }

    let mut command = tokio::process::Command::new("git");
    command.args(base_args);
    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, &ctx).await?;

    if !output.status.success() {
        let stderr = sanitized_git_stderr(&output.stderr, token);
        bail!("git clone failed (exit {}): {}", output.status, stderr);
    }

    Ok(tmp)
}
