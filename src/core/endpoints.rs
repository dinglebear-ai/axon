use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    Qdrant,
    Embedding,
    Chrome,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSource {
    Configured,
    LocalhostDefault,
    TrustedCached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEndpoint {
    pub kind: EndpointKind,
    pub url: String,
    pub source: EndpointSource,
    pub warnings: Vec<String>,
}

pub fn resolve_host_endpoint(
    kind: EndpointKind,
    configured: Option<&str>,
    trusted_cached: &[String],
) -> Option<ResolvedEndpoint> {
    if let Some(configured) = configured.filter(|value| !value.trim().is_empty()) {
        if !uses_container_dns(configured) {
            return Some(ResolvedEndpoint {
                kind,
                url: configured.to_string(),
                source: EndpointSource::Configured,
                warnings: vec![],
            });
        }
        return Some(ResolvedEndpoint {
            kind,
            url: localhost_default(kind)?.to_string(),
            source: EndpointSource::LocalhostDefault,
            warnings: vec![format!(
                "configured endpoint {configured} uses container DNS; using host localhost default"
            )],
        });
    }

    if let Some(url) = localhost_default(kind) {
        return Some(ResolvedEndpoint {
            kind,
            url: url.to_string(),
            source: EndpointSource::LocalhostDefault,
            warnings: vec![],
        });
    }

    trusted_cached.first().map(|url| ResolvedEndpoint {
        kind,
        url: url.clone(),
        source: EndpointSource::TrustedCached,
        warnings: vec![],
    })
}

fn uses_container_dns(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    ["axon-qdrant", "axon-tei", "axon-chrome"].contains(&host)
}

fn localhost_default(kind: EndpointKind) -> Option<&'static str> {
    match kind {
        EndpointKind::Qdrant => Some("http://127.0.0.1:53333"),
        EndpointKind::Embedding => Some("http://127.0.0.1:52000"),
        EndpointKind::Chrome => Some("http://127.0.0.1:6000"),
        EndpointKind::Llm => None,
    }
}
