//! Deterministic `config_snapshot_id` derivation for `JobCreateRequest` rows
//! (job-contract.md's `config_snapshot_id`: "Immutable config/provider
//! snapshot used by the job").
//!
//! Before the unified executor owned this value, the retired per-family source
//! runners passed static family literals, so jobs could share an id despite
//! differing collection, embedding model, or provider configuration. The
//! canonical executor now derives this id from the effective routed inputs.
//!
//! `config_snapshot_id_from_json` is for callers that already hold a
//! serialized config string (e.g. `axon_jobs::config_snapshot::config_snapshot_json`
//! output). `config_snapshot_id` is for the unified source executor, which
//! hashes the job-relevant subset already carried by the routed plan instead
//! ([`JobConfigSnapshot`]).
//!
//! Same effective config in -> same id out: this is a pure content hash
//! (SHA-256, truncated to 12 hex chars, `cfg_` prefixed), not a random or
//! per-family-static value, matching `crate::reset`'s existing
//! `cfg_<hash>` pattern for a different (reset-run inventory) identity.

use axon_api::source::ConfigSnapshotId;
use sha2::{Digest, Sha256};

/// The job-relevant config subset used to derive one unified source job's
/// `config_snapshot_id` from the canonical routed request and active provider
/// identities.
#[derive(Debug, Clone, Copy)]
pub struct JobConfigSnapshot<'a> {
    pub source_kind: &'a str,
    pub source_ref: &'a str,
    pub collection: &'a str,
    pub embedding_provider_id: &'a str,
    pub vector_provider_id: &'a str,
    pub embedding_model: &'a str,
    pub embedding_dimensions: u32,
    pub embed: bool,
    pub max_items: Option<u64>,
}

impl JobConfigSnapshot<'_> {
    fn material(&self) -> String {
        format!(
            "kind={};source={};collection={};embed_provider={};vector_provider={};\
             embed_model={};embed_dims={};embed={};max_items={}",
            self.source_kind,
            self.source_ref,
            self.collection,
            self.embedding_provider_id,
            self.vector_provider_id,
            self.embedding_model,
            self.embedding_dimensions,
            self.embed,
            self.max_items
                .map(|n| n.to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
    }
}

/// Derive a stable `cfg_<12hex>` id from a source-family job's effective
/// config subset: same inputs -> same id, so job reproducibility can be
/// checked by comparing ids rather than trusting a static per-family literal.
pub fn config_snapshot_id(snapshot: &JobConfigSnapshot<'_>) -> ConfigSnapshotId {
    config_snapshot_id_from_material(&snapshot.material())
}

/// Derive a stable `cfg_<12hex>` id directly from an already-serialized
/// config string (e.g. `axon_jobs::config_snapshot::config_snapshot_json`'s
/// output), for callers that hold a full `Config` rather than a source-family
/// input struct.
pub fn config_snapshot_id_from_json(config_json: &str) -> ConfigSnapshotId {
    config_snapshot_id_from_material(config_json)
}

fn config_snapshot_id_from_material(material: &str) -> ConfigSnapshotId {
    ConfigSnapshotId::new(format!("cfg_{}", short_hash(material)))
}

fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}").chars().take(12).collect()
}

#[cfg(test)]
#[path = "config_snapshot_hash_tests.rs"]
mod tests;
