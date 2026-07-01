//! Source resolver boundary.

use axon_api::{
    AdapterCandidate, AuthorityHint, AuthorityLevel, ResolvedSource, Severity, SourceKind,
    SourceRequest, SourceScope, SourceWarning,
};
use axon_error::{ApiError, ErrorStage};

use crate::authority::InMemoryAuthorityRegistry;
use crate::canonical;
use crate::capability::AdapterRegistry;
use crate::source_id::source_id;

#[derive(Debug, Clone)]
pub struct SourceResolver {
    authorities: InMemoryAuthorityRegistry,
    adapters: AdapterRegistry,
}

impl SourceResolver {
    pub fn new(authorities: InMemoryAuthorityRegistry, adapters: AdapterRegistry) -> Self {
        Self {
            authorities,
            adapters,
        }
    }

    pub fn resolve(&self, request: &SourceRequest) -> Result<ResolvedSource, ApiError> {
        let mut warnings = Vec::new();
        let authority_matches = self.authorities.matches(&request.source);
        if authority_matches.len() > 1 {
            return Err(ApiError::new(
                "source.resolve.ambiguous",
                ErrorStage::Resolving,
                "source matched multiple authority records",
            )
            .with_context("source", request.source.clone())
            .with_context("matches", authority_matches.len().to_string()));
        }
        let authority_record = authority_matches.first().copied();
        let canonical = self.resolve_canonical(request, authority_record, &mut warnings)?;
        warnings.extend(canonical.warnings.clone());
        let candidates = self.candidates_for(&canonical);
        let authority = authority_record
            .map(|record| record.authority)
            .unwrap_or(AuthorityLevel::Inferred);
        let confidence = authority_record
            .map(|record| record.confidence.clamp(0.0, 1.0))
            .unwrap_or(0.75);
        let requested_uri = public_requested_uri(request, &canonical);
        let authority_hint = authority_record.map(|record| AuthorityHint {
            canonical_uri: Some(canonical.canonical_uri.clone()),
            authority: record.authority,
            evidence: record.evidence.clone(),
        });

        Ok(ResolvedSource {
            requested_uri,
            canonical_uri: canonical.canonical_uri.clone(),
            source_id: source_id(canonical.source_kind, &canonical.canonical_uri),
            source_kind: canonical.source_kind,
            display_name: canonical.display_name,
            candidate_adapters: candidates.clone(),
            default_scope: request.scope.unwrap_or(canonical.default_scope),
            available_scopes: union_available_scopes(&candidates),
            authority,
            confidence,
            reason: canonical.reason,
            authority_hint,
            warnings,
        })
    }

    fn resolve_canonical(
        &self,
        request: &SourceRequest,
        authority_record: Option<&crate::authority::AuthorityRecord>,
        warnings: &mut Vec<SourceWarning>,
    ) -> Result<canonical::CanonicalSource, ApiError> {
        match authority_record {
            Some(record) => {
                self.validate_authority_record(record)?;
                let scope = request
                    .scope
                    .unwrap_or_else(|| default_scope_for_authority_record(record));
                let canonical_uri = record
                    .entrypoints
                    .iter()
                    .find(|(candidate_scope, _)| candidate_scope == &scope)
                    .map(|(_, uri)| uri.clone())
                    .unwrap_or_else(|| record.canonical_uri.clone());
                warnings.push(warning(
                    "authority.entrypoint_mapped",
                    "source matched an authority entrypoint",
                ));
                Ok(canonical::CanonicalSource {
                    canonical_uri,
                    source_kind: record.source_kind,
                    default_scope: scope,
                    adapter_hint: record.adapter_hint.clone(),
                    display_name: request.source.clone(),
                    reason: format!("matched authority {}", record.authority_id),
                    warnings: Vec::new(),
                })
            }
            None => canonical::canonicalize(&request.source, request.scope).ok_or_else(|| {
                ApiError::new(
                    "source.resolve.unsupported",
                    ErrorStage::Resolving,
                    "unsupported source input",
                )
                .with_context("source", request.source.clone())
            }),
        }
    }

    fn candidates_for(&self, canonical: &canonical::CanonicalSource) -> Vec<AdapterCandidate> {
        let mut candidates = self
            .adapters
            .adapters_for(canonical.source_kind)
            .into_iter()
            .map(|adapter| AdapterCandidate {
                adapter: adapter.adapter.clone(),
                supported_scopes: adapter.supported_scopes.clone(),
                confidence: if Some(adapter.adapter.name.as_str())
                    == canonical.adapter_hint.as_deref()
                {
                    1.0
                } else {
                    0.8
                },
                reason: format!("{} adapter supports source kind", adapter.adapter.name),
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .confidence
                .total_cmp(&left.confidence)
                .then_with(|| left.adapter.name.cmp(&right.adapter.name))
        });
        candidates
    }

    fn validate_authority_record(
        &self,
        record: &crate::authority::AuthorityRecord,
    ) -> Result<(), ApiError> {
        if !uri_matches_kind(&record.canonical_uri, record.source_kind)
            || record
                .entrypoints
                .iter()
                .any(|(_, uri)| !uri_matches_kind(uri, record.source_kind))
        {
            return Err(ApiError::new(
                "source.authority.invalid",
                ErrorStage::Resolving,
                "authority record URI does not match declared source kind",
            )
            .with_context("authority_id", record.authority_id.clone()));
        }
        if let Some(adapter_hint) = record.adapter_hint.as_deref() {
            let adapter = self.adapters.find(adapter_hint).ok_or_else(|| {
                ApiError::new(
                    "source.authority.invalid",
                    ErrorStage::Resolving,
                    "authority record references an unknown adapter",
                )
                .with_context("authority_id", record.authority_id.clone())
                .with_context("adapter", adapter_hint.to_string())
            })?;
            if adapter.source_kind != record.source_kind {
                return Err(ApiError::new(
                    "source.authority.invalid",
                    ErrorStage::Resolving,
                    "authority adapter does not support declared source kind",
                )
                .with_context("authority_id", record.authority_id.clone())
                .with_context("adapter", adapter_hint.to_string()));
            }
        }
        Ok(())
    }
}

fn uri_matches_kind(uri: &str, kind: SourceKind) -> bool {
    match kind {
        SourceKind::Web => uri.starts_with("http://") || uri.starts_with("https://"),
        SourceKind::Local => uri.starts_with("local://"),
        SourceKind::Git => {
            uri.starts_with("github://")
                || uri.starts_with("gitlab://")
                || uri.starts_with("gitea://")
                || uri.starts_with("git+http://")
                || uri.starts_with("git+https://")
                || git_provider_url(uri)
        }
        SourceKind::Registry => uri.starts_with("pkg://") || uri.starts_with("docker://"),
        SourceKind::Feed => uri.starts_with("feed://"),
        SourceKind::Reddit => uri.starts_with("reddit://"),
        SourceKind::Youtube => uri.starts_with("youtube://"),
        SourceKind::Session => uri.starts_with("session://"),
        SourceKind::CliTool => uri.starts_with("cli://"),
        SourceKind::McpTool => uri.starts_with("mcp://"),
        SourceKind::Memory => uri.starts_with("memory://"),
        SourceKind::Upload => uri.starts_with("upload://"),
    }
}

fn union_available_scopes(candidates: &[AdapterCandidate]) -> Vec<SourceScope> {
    let mut scopes = Vec::new();
    for candidate in candidates {
        for scope in &candidate.supported_scopes {
            if !scopes.contains(scope) {
                scopes.push(*scope);
            }
        }
    }
    scopes
}

fn default_scope_for_authority_record(record: &crate::authority::AuthorityRecord) -> SourceScope {
    record
        .entrypoints
        .first()
        .map(|(scope, _)| *scope)
        .unwrap_or_else(|| default_scope_for_kind(record.source_kind))
}

fn default_scope_for_kind(source_kind: SourceKind) -> SourceScope {
    match source_kind {
        SourceKind::Web => SourceScope::Site,
        SourceKind::Local => SourceScope::Directory,
        SourceKind::Git => SourceScope::Repo,
        SourceKind::Registry => SourceScope::Package,
        SourceKind::Feed => SourceScope::Feed,
        SourceKind::Reddit => SourceScope::Subreddit,
        SourceKind::Youtube => SourceScope::Video,
        SourceKind::Session => SourceScope::Thread,
        SourceKind::CliTool | SourceKind::McpTool => SourceScope::Tool,
        SourceKind::Memory => SourceScope::Thread,
        SourceKind::Upload => SourceScope::File,
    }
}

fn git_provider_url(uri: &str) -> bool {
    let Ok(url) = url::Url::parse(uri) else {
        return false;
    };
    let Some(host) = url.host_str().map(|host| host.trim_start_matches("www.")) else {
        return false;
    };
    host == "github.com"
        || host.ends_with(".github.com")
        || host == "gitlab.com"
        || host.ends_with(".gitlab.com")
        || host.starts_with("gitlab.")
        || host == "codeberg.org"
        || host.ends_with(".codeberg.org")
        || host == "gitea.com"
        || host.ends_with(".gitea.com")
        || host == "forgejo.org"
        || host.ends_with(".forgejo.org")
        || host.starts_with("gitea.")
        || host.starts_with("forgejo.")
        || url.path().trim_end_matches('/').ends_with(".git")
}

fn public_requested_uri(request: &SourceRequest, canonical: &canonical::CanonicalSource) -> String {
    if canonical.source_kind == SourceKind::Local {
        return "local://redacted".to_string();
    }
    let redacted_query = canonical
        .warnings
        .iter()
        .any(|warning| warning.code == "source.query.sensitive_redacted");
    if redacted_query || has_url_userinfo(&request.source) {
        canonical.canonical_uri.clone()
    } else {
        request.source.clone()
    }
}

fn has_url_userinfo(source: &str) -> bool {
    let Some((_, rest)) = source.split_once("://") else {
        return false;
    };
    rest.split('/')
        .next()
        .is_some_and(|host| host.contains('@'))
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
