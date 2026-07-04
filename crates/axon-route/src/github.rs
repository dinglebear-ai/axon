//! GitHub source canonicalization.

use axon_api::{Severity, SourceKind, SourceScope, SourceWarning};

use crate::canonical::CanonicalSource;

pub(crate) fn canonical_github(raw: &str) -> Option<CanonicalSource> {
    let path = raw
        .strip_prefix("https://github.com/")
        .or_else(|| raw.strip_prefix("http://github.com/"))
        .or_else(|| raw.strip_prefix("github.com/"))
        .or_else(|| {
            if raw.contains("://") || raw.contains('.') || raw.contains(':') {
                None
            } else {
                Some(raw)
            }
        })?;
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].contains('.') || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    let owner = parts[0];
    let repo = trim_git_suffix(parts[1]);
    if repo.is_empty() {
        return None;
    }
    let repo_uri = format!("github://{owner}/{repo}");
    let (canonical_uri, scope, reason) = github_subpath(&repo_uri, &parts[2..]).unwrap_or((
        repo_uri,
        SourceScope::Repo,
        "resolved as GitHub repository source",
    ));
    let mut source = CanonicalSource {
        canonical_uri,
        source_kind: SourceKind::Git,
        default_scope: scope,
        adapter_hint: Some("github".to_string()),
        display_name: repo.to_string(),
        reason: reason.to_string(),
        warnings: Vec::new(),
    };
    if path == raw {
        source.warnings.push(warning(
            "source.inferred.github_shorthand",
            "source interpreted as GitHub owner/repo shorthand",
        ));
    }
    if let Ok(url) = url::Url::parse(raw) {
        source
            .warnings
            .extend(crate::query::sensitive_query_warnings(&url));
    }
    Some(source)
}

fn github_subpath(repo_uri: &str, rest: &[&str]) -> Option<(String, SourceScope, &'static str)> {
    match rest {
        ["issues", id, ..] => Some((
            format!("{repo_uri}/issues/{id}"),
            SourceScope::Issue,
            "resolved as GitHub issue source",
        )),
        ["pull", id, ..] | ["pulls", id, ..] => Some((
            format!("{repo_uri}/pulls/{id}"),
            SourceScope::PullRequest,
            "resolved as GitHub pull request source",
        )),
        ["releases", "tag", tag, ..] => Some((
            format!("{repo_uri}/releases/tag/{tag}"),
            SourceScope::Release,
            "resolved as GitHub release source",
        )),
        ["tree", branch @ ..] if !branch.is_empty() => Some((
            format!("{repo_uri}/tree/{}", branch.join("/")),
            SourceScope::Branch,
            "resolved as GitHub branch source",
        )),
        _ => None,
    }
}

fn trim_git_suffix(value: &str) -> &str {
    value.strip_suffix(".git").unwrap_or(value)
}

fn warning(code: &str, message: &str) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Info,
        message: message.to_string(),
        source_item_key: None,
        retryable: false,
    }
}
