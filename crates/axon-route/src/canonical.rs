//! Lexical source normalization and source-family detection.

use crate::provider_host::{is_gitea_host, is_github_host, is_gitlab_host, is_youtube_host};
use crate::query::{normalized_query, sensitive_query_warnings};
use axon_api::{Severity, SourceKind, SourceScope, SourceWarning};
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalSource {
    pub canonical_uri: String,
    pub source_kind: SourceKind,
    pub default_scope: SourceScope,
    pub adapter_hint: Option<String>,
    pub display_name: String,
    pub reason: String,
    pub warnings: Vec<SourceWarning>,
}

pub fn canonicalize(raw: &str, requested_scope: Option<SourceScope>) -> Option<CanonicalSource> {
    let source = raw.trim();
    canonical_local(source, requested_scope)
        .or_else(|| canonical_mcp(source))
        .or_else(|| canonical_cli(source))
        .or_else(|| canonical_session(source))
        .or_else(|| canonical_memory(source))
        .or_else(|| canonical_upload(source))
        .or_else(|| canonical_feed(source))
        .or_else(|| canonical_reddit(source))
        .or_else(|| canonical_youtube(source))
        .or_else(|| canonical_registry(source))
        .or_else(|| canonical_gitlab(source))
        .or_else(|| canonical_gitea(source))
        .or_else(|| crate::github::canonical_github(source))
        .or_else(|| canonical_generic_git(source))
        .or_else(|| canonical_web(source))
}

pub fn is_lexically_local_path(raw: &str) -> bool {
    raw.starts_with('/') || raw.starts_with("./") || raw.starts_with("../") || raw.starts_with('~')
}

fn canonical_local(raw: &str, requested_scope: Option<SourceScope>) -> Option<CanonicalSource> {
    if !is_lexically_local_path(raw) {
        return None;
    }
    let normalized = crate::local_path::normalize_local_path(raw);
    let key = crate::source_id::local_project_key(&normalized);
    let display_name = normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("local")
        .to_string();
    Some(CanonicalSource {
        canonical_uri: format!("local://{key}"),
        source_kind: SourceKind::Local,
        default_scope: requested_scope.unwrap_or(SourceScope::Directory),
        adapter_hint: Some("local".to_string()),
        display_name,
        reason: "resolved as local filesystem source".to_string(),
        warnings: Vec::new(),
    })
}

fn canonical_mcp(raw: &str) -> Option<CanonicalSource> {
    let rest = raw.strip_prefix("mcp:")?;
    let (server, tool) = rest.split_once('/')?;
    if server.trim().is_empty() || tool.trim().is_empty() {
        return None;
    }
    Some(basic(
        format!("mcp://{server}/tools/{tool}"),
        SourceKind::McpTool,
        SourceScope::Tool,
        "mcp",
        tool,
        "resolved as MCP tool source",
    ))
}

fn canonical_cli(raw: &str) -> Option<CanonicalSource> {
    let rest = raw.strip_prefix("cli:")?.trim();
    let tool = rest.split_whitespace().next()?;
    Some(basic(
        format!("cli://{tool}"),
        SourceKind::CliTool,
        SourceScope::Tool,
        "cli",
        tool,
        "resolved as CLI tool source",
    ))
}

fn canonical_session(raw: &str) -> Option<CanonicalSource> {
    let rest = raw.strip_prefix("session:")?;
    let (provider, session_id) = rest.split_once(':')?;
    if provider.trim().is_empty() || session_id.trim().is_empty() {
        return None;
    }
    Some(basic(
        format!("session://{provider}/{session_id}"),
        SourceKind::Session,
        SourceScope::Thread,
        "session",
        session_id,
        "resolved as AI session source",
    ))
}

fn canonical_upload(raw: &str) -> Option<CanonicalSource> {
    let artifact = raw
        .strip_prefix("upload://")
        .or_else(|| raw.strip_prefix("upload:"))
        .or_else(|| raw.strip_prefix("artifact://"))?
        .trim();
    if artifact.is_empty()
        || (!artifact.starts_with("upl_") && !artifact.starts_with("art_"))
        || artifact
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-' | b'.'))
    {
        return None;
    }
    Some(basic(
        format!("upload://{artifact}"),
        SourceKind::Upload,
        SourceScope::File,
        "upload",
        artifact,
        "resolved as upload/artifact source",
    ))
}

fn canonical_memory(raw: &str) -> Option<CanonicalSource> {
    let memory_id = raw.strip_prefix("memory://")?;
    if memory_id.is_empty()
        || !memory_id.starts_with("mem_")
        || memory_id
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-' | b'.'))
    {
        return None;
    }
    Some(basic(
        format!("memory://{memory_id}"),
        SourceKind::Memory,
        SourceScope::Api,
        "memory",
        memory_id,
        "resolved as durable memory source",
    ))
}

fn canonical_feed(raw: &str) -> Option<CanonicalSource> {
    let explicit_feed_url = raw
        .strip_prefix("rss:")
        .or_else(|| raw.strip_prefix("feed:"))
        .or_else(|| raw.strip_prefix("atom:"));
    let feed_url = match explicit_feed_url {
        Some(url) => url,
        None if looks_like_feed_url(raw) => raw,
        None => return None,
    };
    let url = normalized_url(feed_url)?;
    if !is_http_url(&url) {
        return None;
    }
    let mut source = basic(
        format!("feed://{}{}", host_port(&url)?, clean_path(&url)),
        SourceKind::Feed,
        SourceScope::Feed,
        "feed",
        url.host_str()?,
        "resolved as feed source",
    );
    source.warnings.extend(sensitive_query_warnings(&url));
    Some(source)
}

fn looks_like_feed_url(raw: &str) -> bool {
    let Some(url) = normalized_url(raw) else {
        return false;
    };
    if !is_http_url(&url) {
        return false;
    }
    let path = url.path().to_ascii_lowercase();
    let last_segment = path.rsplit('/').next().unwrap_or("");
    path.ends_with(".rss")
        || path.ends_with(".atom")
        || path.ends_with(".rdf")
        || matches!(
            last_segment,
            "feed.xml" | "rss.xml" | "atom.xml" | "index.xml" | "feed" | "rss" | "atom"
        )
        || path
            .split('/')
            .any(|segment| matches!(segment, "feed" | "feeds" | "rss" | "atom"))
        || url.query_pairs().any(|(key, value)| {
            key == "feed"
                || (matches!(key.as_ref(), "format" | "output" | "type")
                    && matches!(value.as_ref(), "rss" | "rss2" | "atom" | "rdf"))
        })
}

fn canonical_reddit(raw: &str) -> Option<CanonicalSource> {
    let subreddit = raw
        .strip_prefix("r/")
        .or_else(|| raw.strip_prefix("reddit.com/r/"))
        .or_else(|| raw.strip_prefix("https://reddit.com/r/"))
        .or_else(|| raw.strip_prefix("https://www.reddit.com/r/"))?;
    let name = subreddit.split('/').next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(basic(
        format!("reddit://r/{name}"),
        SourceKind::Reddit,
        SourceScope::Subreddit,
        "reddit",
        name,
        "resolved as Reddit source",
    ))
}

fn canonical_youtube(raw: &str) -> Option<CanonicalSource> {
    let url = normalized_url(raw)?;
    let host = url.host_str()?.trim_start_matches("www.");
    if host == "youtu.be" {
        let id = url.path().trim_start_matches('/');
        if id.is_empty() {
            return None;
        }
        return Some(youtube_video(id));
    }
    if is_youtube_host(host) && url.path() == "/watch" {
        let id = url.query_pairs().find(|(key, _)| key == "v")?.1;
        if id.trim().is_empty() {
            return None;
        }
        return Some(youtube_video(&id));
    }
    None
}

fn youtube_video(id: &str) -> CanonicalSource {
    basic(
        format!("youtube://video/{id}"),
        SourceKind::Youtube,
        SourceScope::Video,
        "youtube",
        id,
        "resolved as YouTube video source",
    )
}

fn canonical_registry(raw: &str) -> Option<CanonicalSource> {
    let (registry, package) = raw.split_once(':')?;
    let adapter = match registry {
        "crates" | "npm" | "pypi" | "docker" => registry,
        _ => return None,
    };
    let package = package.trim();
    if package.is_empty() {
        return None;
    }
    let package = if adapter == "pypi" {
        package.to_ascii_lowercase()
    } else {
        package.to_string()
    };
    let canonical_uri = if adapter == "docker" {
        format!("docker://docker.io/{package}")
    } else {
        format!("pkg://{adapter}/{package}")
    };
    Some(basic(
        canonical_uri,
        SourceKind::Registry,
        SourceScope::Package,
        adapter,
        &package,
        "resolved as registry package source",
    ))
}

fn canonical_gitlab(raw: &str) -> Option<CanonicalSource> {
    let url = normalized_url(raw)?;
    let host = url.host_str()?;
    if !is_gitlab_host(host) {
        return None;
    }
    let path = repo_path(&url)?;
    let mut source = basic(
        format!("gitlab://{}/{path}", host_port(&url)?),
        SourceKind::Git,
        SourceScope::Repo,
        "gitlab",
        path.rsplit('/').next().unwrap_or(&path),
        "resolved as GitLab repository source",
    );
    source.warnings.extend(sensitive_query_warnings(&url));
    Some(source)
}

fn canonical_gitea(raw: &str) -> Option<CanonicalSource> {
    let url = normalized_url(raw)?;
    let host = url.host_str()?;
    if !is_gitea_host(host) {
        return None;
    }
    let path = repo_path(&url)?;
    let mut source = basic(
        format!("gitea://{}/{path}", host_port(&url)?),
        SourceKind::Git,
        SourceScope::Repo,
        "gitea",
        path.rsplit('/').next().unwrap_or(&path),
        "resolved as Gitea/Forgejo repository source",
    );
    source.warnings.extend(sensitive_query_warnings(&url));
    Some(source)
}

fn canonical_generic_git(raw: &str) -> Option<CanonicalSource> {
    let url = normalized_url(raw)?;
    if !is_http_url(&url) {
        return None;
    }
    let path = repo_path(&url)?;
    if !path.ends_with(".git") {
        return None;
    }
    let repo = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".git"))
        .unwrap_or_default();
    if repo.is_empty() {
        return None;
    }
    let mut source = basic(
        format!(
            "git+{}://{}{}",
            url.scheme(),
            host_port(&url)?,
            clean_path(&url)
        ),
        SourceKind::Git,
        SourceScope::Repo,
        "git",
        repo,
        "resolved as generic Git repository source",
    );
    source.warnings.extend(sensitive_query_warnings(&url));
    Some(source)
}

fn canonical_web(raw: &str) -> Option<CanonicalSource> {
    let url = normalized_url(raw)?;
    if !is_http_url(&url) {
        return None;
    }
    if is_malformed_provider_url(&url) {
        return None;
    }
    let query = normalized_query(&url);
    let mut source = basic(
        format!(
            "{}://{}{}{}",
            url.scheme(),
            host_port(&url)?,
            clean_path(&url),
            query.query
        ),
        SourceKind::Web,
        SourceScope::Site,
        "web",
        url.host_str()?,
        "resolved as web source",
    );
    source.warnings.extend(query.warnings);
    if !raw.contains("://") {
        source.warnings.push(warning(
            "source.inferred.scheme",
            "source did not include a URL scheme; https was inferred",
        ));
    }
    Some(source)
}

fn normalized_url(raw: &str) -> Option<Url> {
    let candidate = if raw.contains("://") {
        raw.to_string()
    } else if raw.contains('.') {
        format!("https://{raw}")
    } else {
        return None;
    };
    let mut url = Url::parse(&candidate).ok()?;
    url.set_scheme(&url.scheme().to_ascii_lowercase()).ok()?;
    Some(url)
}

fn is_http_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn clean_path(url: &Url) -> String {
    let path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };
    let path = collapse_duplicate_slashes(path);
    if path == "/" {
        path
    } else {
        path.trim_end_matches('/').to_string()
    }
}

fn host_port(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn repo_path(url: &Url) -> Option<String> {
    let path = clean_path(url).trim_start_matches('/').to_string();
    let path = path.split("/-/").next().unwrap_or(&path);
    if path.split('/').count() < 2 {
        None
    } else {
        Some(path.to_string())
    }
}

fn collapse_duplicate_slashes(path: &str) -> String {
    let mut collapsed = String::with_capacity(path.len());
    let mut previous_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if !previous_slash {
                collapsed.push(ch);
            }
            previous_slash = true;
        } else {
            collapsed.push(ch);
            previous_slash = false;
        }
    }
    collapsed
}

fn is_malformed_provider_url(url: &Url) -> bool {
    let Some(host) = url.host_str().map(|host| host.trim_start_matches("www.")) else {
        return false;
    };
    malformed_youtube_url(host, url) || malformed_github_url(host, url)
}

fn malformed_youtube_url(host: &str, url: &Url) -> bool {
    (host == "youtu.be" && url.path().trim_start_matches('/').is_empty())
        || (is_youtube_host(host)
            && url.path() == "/watch"
            && url
                .query_pairs()
                .find(|(key, _)| key == "v")
                .is_some_and(|(_, value)| value.trim().is_empty()))
}

fn malformed_github_url(host: &str, url: &Url) -> bool {
    if !is_github_host(host) {
        return false;
    }
    let parts = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts.get(1).is_some_and(|repo| *repo == ".git")
}

fn basic(
    canonical_uri: String,
    source_kind: SourceKind,
    default_scope: SourceScope,
    adapter: &str,
    display_name: &str,
    reason: &str,
) -> CanonicalSource {
    CanonicalSource {
        canonical_uri,
        source_kind,
        default_scope,
        adapter_hint: Some(adapter.to_string()),
        display_name: display_name.to_string(),
        reason: reason.to_string(),
        warnings: Vec::new(),
    }
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
