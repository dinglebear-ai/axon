use anyhow::{Result, bail};

use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};

use super::super::GitHubCommonFields;
use crate::core::logging::log_warn;

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
        Some(false) | None => stderr_has_auth_or_permission_failure(stderr),
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
///
/// When a token is provided, authentication is passed via the git
/// `http.extraHeader` config option as a base64-encoded `Authorization: Basic`
/// header — this avoids embedding the token in the clone URL where it would be
/// visible in process lists, git reflog, and shell history.
/// GIT_TERMINAL_PROMPT=0 ensures git fails fast rather than blocking on
/// credential prompts.
pub(super) async fn clone_repo(
    common: &GitHubCommonFields,
    branch: &str,
    token: Option<&str>,
) -> Result<tempfile::TempDir> {
    use crate::core::http::validate_url;
    use base64::Engine as _;

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();

    // Validate the public URL for SSRF before issuing any network request.
    let public_url = format!("https://github.com/{}/{}.git", common.owner, common.name);
    validate_url(&public_url)?;

    let ctx = format!("git clone {}", common.repo_slug);
    let mut command = tokio::process::Command::new("git");

    // Pass the token via http.extraHeader so it never appears in the clone URL
    // or process list. The header value is `Authorization: Basic
    // base64("x-oauth-basic:TOKEN")` which is the format GitHub accepts for
    // PAT / fine-grained token auth over HTTPS.
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("x-oauth-basic:{t}"));
        let header = format!("Authorization: Basic {encoded}");
        command.arg("-c").arg(format!("http.extraHeader={header}"));
    }

    command
        .args([
            "clone",
            "--depth=1",
            "--branch",
            branch,
            "--single-branch",
            "--",
            &public_url,
            &tmp_path,
        ])
        .env("GIT_TERMINAL_PROMPT", "0");

    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, &ctx).await?;

    if output.status.success() {
        return Ok(tmp);
    }

    let stderr = sanitized_git_stderr(&output.stderr, token);

    if token.is_some() && should_retry_unauthenticated_clone(common, &stderr) {
        log_warn(&format!(
            "command=ingest_github auth_clone_failed repo={}/{} retrying_unauthenticated",
            common.owner, common.name
        ));
        let _ = tokio::fs::remove_dir_all(tmp.path()).await;
        tokio::fs::create_dir_all(tmp.path()).await.map_err(|e| {
            anyhow::anyhow!("failed to recreate tmp dir for unauthenticated retry: {e}")
        })?;

        let mut fallback = tokio::process::Command::new("git");
        fallback
            .args([
                "clone",
                "--depth=1",
                "--branch",
                branch,
                "--single-branch",
                "--",
                &public_url,
                &tmp_path,
            ])
            .env("GIT_TERMINAL_PROMPT", "0");
        let fallback_output = run_command_with_timeout(fallback, SUBPROCESS_TIMEOUT, &ctx).await?;
        if fallback_output.status.success() {
            return Ok(tmp);
        }
        let fallback_stderr = sanitized_git_stderr(&fallback_output.stderr, token);
        bail!(
            "git clone failed (exit {}): {}",
            fallback_output.status,
            fallback_stderr
        );
    }

    bail!(
        "git clone failed for {} (exit {}): {}",
        common.repo_slug,
        output.status,
        stderr
    );
}
