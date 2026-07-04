//! Authority and alias records used during source resolution.

use axon_api::{AuthorityEvidence, AuthorityLevel, SourceKind, SourceScope};
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct AuthorityRecord {
    pub authority_id: String,
    pub canonical_uri: String,
    pub source_kind: SourceKind,
    pub authority: AuthorityLevel,
    pub aliases: Vec<String>,
    pub entrypoints: Vec<(SourceScope, String)>,
    pub adapter_hint: Option<String>,
    pub confidence: f32,
    pub evidence: Vec<AuthorityEvidence>,
}

impl AuthorityRecord {
    pub fn new(
        authority_id: impl Into<String>,
        canonical_uri: impl Into<String>,
        source_kind: SourceKind,
        authority: AuthorityLevel,
    ) -> Self {
        let canonical_uri = canonical_uri.into();
        Self {
            authority_id: authority_id.into(),
            adapter_hint: adapter_hint_for_uri(&canonical_uri, source_kind),
            canonical_uri,
            source_kind,
            authority,
            aliases: Vec::new(),
            entrypoints: Vec::new(),
            confidence: 1.0,
            evidence: Vec::new(),
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(normalize_alias(&alias.into()));
        self
    }

    pub fn with_entrypoint(mut self, scope: SourceScope, uri: impl Into<String>) -> Self {
        self.entrypoints.push((scope, uri.into()));
        self
    }

    pub fn with_adapter_hint(mut self, adapter: impl Into<String>) -> Self {
        self.adapter_hint = Some(adapter.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_evidence(
        mut self,
        evidence_kind: impl Into<String>,
        value: impl Into<String>,
        confidence: f32,
    ) -> Self {
        self.evidence.push(AuthorityEvidence {
            evidence_kind: evidence_kind.into(),
            value: value.into(),
            confidence: confidence.clamp(0.0, 1.0),
        });
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryAuthorityRegistry {
    records: Vec<AuthorityRecord>,
}

fn adapter_hint_for_uri(uri: &str, source_kind: SourceKind) -> Option<String> {
    let hint = if uri.starts_with("github://") {
        "github"
    } else if uri.starts_with("gitlab://") {
        "gitlab"
    } else if uri.starts_with("gitea://") {
        "gitea"
    } else if uri.starts_with("git+https://") {
        "git"
    } else if let Some(rest) = uri.strip_prefix("pkg://") {
        rest.split('/').next()?
    } else if uri.starts_with("docker://") {
        "docker"
    } else if uri.starts_with("reddit://") {
        "reddit"
    } else if uri.starts_with("youtube://") {
        "youtube"
    } else if uri.starts_with("feed://") {
        "feed"
    } else if uri.starts_with("session://") {
        "session"
    } else if uri.starts_with("local://") {
        "local"
    } else if uri.starts_with("upload://") {
        "upload"
    } else if uri.starts_with("cli://") {
        "cli"
    } else if uri.starts_with("mcp://") {
        "mcp"
    } else if source_kind == SourceKind::Git
        && (uri.starts_with("http://") || uri.starts_with("https://"))
    {
        adapter_hint_for_http_uri(uri)?
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        "web"
    } else {
        return None;
    };
    Some(hint.to_string())
}

impl InMemoryAuthorityRegistry {
    pub fn from_records(records: Vec<AuthorityRecord>) -> Self {
        Self { records }
    }

    pub fn find(&self, raw_source: &str) -> Option<&AuthorityRecord> {
        self.matches(raw_source).into_iter().next()
    }

    pub fn matches(&self, raw_source: &str) -> Vec<&AuthorityRecord> {
        let alias = normalize_alias(raw_source);
        self.records
            .iter()
            .filter(|record| {
                normalize_alias(&record.canonical_uri) == alias
                    || record.aliases.iter().any(|candidate| candidate == &alias)
            })
            .collect()
    }
}

fn normalize_alias(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    lower
        .as_str()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

fn adapter_hint_for_http_uri(uri: &str) -> Option<&'static str> {
    let url = Url::parse(uri).ok()?;
    let host = url.host_str()?.trim_start_matches("www.");
    if host == "github.com" || host.ends_with(".github.com") {
        Some("github")
    } else if host == "gitlab.com" || host.ends_with(".gitlab.com") || host.starts_with("gitlab.") {
        Some("gitlab")
    } else if host == "codeberg.org"
        || host.ends_with(".codeberg.org")
        || host == "gitea.com"
        || host.ends_with(".gitea.com")
        || host == "forgejo.org"
        || host.ends_with(".forgejo.org")
        || host.starts_with("gitea.")
        || host.starts_with("forgejo.")
    {
        Some("gitea")
    } else if url.path().trim_end_matches('/').ends_with(".git") {
        Some("git")
    } else {
        Some("web")
    }
}
