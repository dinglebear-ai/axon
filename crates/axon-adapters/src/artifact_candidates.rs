//! ArtifactCandidate provider/sink boundary.
//!
//! Candidate discovery stays inside the unified SourceRequest pipeline. This
//! module only defines the typed delivery boundary used after source adapters
//! have produced normalized observations.

use async_trait::async_trait;
use axon_api::source::{
    ARTIFACT_CANDIDATE_CONTRACT_VERSION, ApiError, ArtifactCandidateBatch, ArtifactCandidateDedupe,
    ArtifactCandidateId, ArtifactCandidateSinkCapability, ArtifactCandidateSinkResult,
    ArtifactCandidateSinkStatus,
};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, ApiError>;

/// A version-negotiated output sink for Axon artifact observations.
///
/// Sinks do not publish artifacts. Depot implementations accept evidence for
/// later intake/normalization/publication decisions.
#[async_trait]
pub trait ArtifactCandidateSink: Send + Sync {
    async fn submit(&self, batch: ArtifactCandidateBatch) -> Result<ArtifactCandidateSinkResult>;
    async fn capabilities(&self) -> Result<ArtifactCandidateSinkCapability>;
}

/// Production default when no artifact-candidate destination is configured.
#[derive(Debug, Clone, Default)]
pub struct NoopArtifactCandidateSink;

#[async_trait]
impl ArtifactCandidateSink for NoopArtifactCandidateSink {
    async fn submit(&self, batch: ArtifactCandidateBatch) -> Result<ArtifactCandidateSinkResult> {
        Ok(ArtifactCandidateSinkResult {
            status: ArtifactCandidateSinkStatus::Disabled,
            attempted: batch.candidates.len() as u64,
            accepted: 0,
            rejected: 0,
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ArtifactCandidateSinkCapability> {
        Ok(ArtifactCandidateSinkCapability {
            name: "noop".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            contract_versions: vec![ARTIFACT_CANDIDATE_CONTRACT_VERSION.to_string()],
            max_batch_size: u32::MAX,
            supports_idempotency: true,
        })
    }
}

/// Build stable source-identity and content-aware de-duplication keys.
///
/// Inputs are length-prefixed before hashing so path/ref delimiter characters
/// cannot create accidental aliases. The caller must pass an already-resolved
/// canonical source URI; source resolution remains owned by axon-route/adapters.
pub fn artifact_candidate_dedupe(
    canonical_source_uri: &str,
    source_ref: Option<&str>,
    source_path: Option<&str>,
    content_hash: Option<&str>,
) -> ArtifactCandidateDedupe {
    let identity_key = digest_parts(
        b"axon-artifact-candidate-identity-v1",
        &[Some(canonical_source_uri), source_ref, source_path],
    );
    let content_key = content_hash.map(|content_hash| {
        digest_parts(
            b"axon-artifact-candidate-content-v1",
            &[
                Some(canonical_source_uri),
                source_ref,
                source_path,
                Some(content_hash),
            ],
        )
    });

    ArtifactCandidateDedupe {
        identity_key,
        content_key,
        content_hash: content_hash.map(str::to_string),
    }
}

/// Candidate identity is content-aware when a content hash is available and
/// otherwise falls back to the stable canonical source identity.
pub fn artifact_candidate_id(dedupe: &ArtifactCandidateDedupe) -> ArtifactCandidateId {
    ArtifactCandidateId::from(
        dedupe
            .content_key
            .as_deref()
            .unwrap_or(&dedupe.identity_key),
    )
}

fn digest_parts(domain: &[u8], parts: &[Option<&str>]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        match part {
            Some(value) => {
                hasher.update([1]);
                hasher.update((value.len() as u64).to_be_bytes());
                hasher.update(value.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "artifact_candidates_tests.rs"]
mod tests;
