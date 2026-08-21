//! ArtifactCandidate collection and committed-generation delivery.

use std::collections::{BTreeMap, BTreeSet};

use axon_adapters::{ArtifactCandidateSink, artifact_candidate_batch_idempotency_key};
use axon_api::source::*;
use axon_core::redact::{DefaultRedactor, RedactionContext, redact_public_write};

use super::SourcePipelineInput;
use super::helpers::timestamp;
mod outbox;

pub(crate) use outbox::spawn_outbox_drain;

/// Hard Axon-side batch ceiling even when a sink advertises a larger limit.
const MAX_CANDIDATE_SINK_BATCH_SIZE: usize = 64;

pub(super) struct CandidateCollection {
    pub(super) candidates: Vec<ArtifactCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
}

struct CandidateDeliveryResult {
    warnings: Vec<SourceWarning>,
    stop_delivery: bool,
    retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CandidateDeliveryDisposition {
    Terminal,
    Retryable,
}

pub(super) struct CandidateDeliveryOutcome {
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) disposition: CandidateDeliveryDisposition,
}

/// Ask the current source adapter for typed candidates beside this normalized
/// changed-document batch. Producer errors/malformed correlation degrade the
/// optional artifact path but never replace SourceDocument/RAG processing.
pub(super) async fn collect_changed_candidates(
    input: &SourcePipelineInput<'_>,
    generation: &SourceGenerationId,
    documents: &[SourceDocument],
    enrichments: &BTreeMap<SourceItemKey, SourceEnrichment>,
) -> CandidateCollection {
    let produced = match input
        .adapter
        .artifact_candidates(&input.plan, generation, documents, enrichments)
        .await
    {
        Ok(candidates) => candidates,
        Err(error) => {
            return CandidateCollection {
                candidates: Vec::new(),
                warnings: vec![warning(
                    "source.artifact_candidate.producer_failed",
                    format!("artifact candidate production failed: {error}"),
                    true,
                )],
            };
        }
    };

    validate_produced_candidates(input.plan.job_id, generation, documents, produced)
}

fn validate_produced_candidates(
    job_id: JobId,
    generation: &SourceGenerationId,
    documents: &[SourceDocument],
    produced: Vec<ArtifactCandidate>,
) -> CandidateCollection {
    let document_keys = documents
        .iter()
        .map(|document| document.source_item_key.clone())
        .collect::<BTreeSet<_>>();
    let mut candidate_ids = BTreeSet::new();
    let mut candidates = Vec::with_capacity(produced.len());
    let mut warnings = Vec::new();

    let expected_job_id = job_id.0.to_string();
    for candidate in produced {
        let candidate = match redact_candidate(candidate) {
            Ok(candidate) => candidate,
            Err(error) => {
                warnings.push(warning(
                    "source.artifact_candidate.redaction_rejected",
                    format!("artifact candidate was rejected at the public-write redaction boundary: {error}"),
                    false,
                ));
                continue;
            }
        };
        if let Err(error) = candidate.validate_shared_contract() {
            warnings.push(warning(
                "source.artifact_candidate.shared_contract_rejected",
                format!(
                    "artifact candidate {} violated the shared {} payload contract: {error}",
                    candidate.id.0, ARTIFACT_CANDIDATE_SCHEMA_VERSION
                ),
                false,
            ));
            continue;
        }
        let source_item_key = candidate
            .manifest_metadata
            .get("axonSourceItemKey")
            .and_then(serde_json::Value::as_str);
        let mismatch = if candidate.crawl_job_id.as_deref() != Some(expected_job_id.as_str()) {
            Some("crawlJobId")
        } else if candidate.crawl_generation_id.as_deref() != Some(generation.0.as_str()) {
            Some("crawlGenerationId")
        } else if !source_item_key
            .is_some_and(|key| document_keys.contains(&SourceItemKey::from(key)))
        {
            Some("manifestMetadata.axonSourceItemKey")
        } else {
            None
        };
        if let Some(field) = mismatch {
            warnings.push(warning(
                "source.artifact_candidate.correlation_rejected",
                format!(
                    "artifact candidate {} had invalid {field} correlation and was not delivered",
                    candidate.id.0
                ),
                false,
            ));
            continue;
        }
        if !candidate_ids.insert(candidate.id.clone()) {
            warnings.push(warning(
                "source.artifact_candidate.duplicate_suppressed",
                format!(
                    "duplicate artifact candidate {} was suppressed in generation {}",
                    candidate.id.0, generation.0
                ),
                false,
            ));
            continue;
        }
        candidates.push(candidate);
    }

    CandidateCollection {
        candidates,
        warnings,
    }
}

fn redact_candidate(candidate: ArtifactCandidate) -> Result<ArtifactCandidate, ApiError> {
    let serialized = serde_json::to_value(candidate).map_err(|error| {
        ApiError::new(
            "source.artifact_candidate.serialize_failed",
            ErrorStage::Enriching,
            error.to_string(),
        )
    })?;
    let redacted = redact_public_write(
        serialized,
        &RedactionContext::artifact_metadata(),
        &DefaultRedactor::new(),
    )?;
    serde_json::from_value(redacted.payload).map_err(|error| {
        ApiError::new(
            "source.artifact_candidate.redacted_payload_invalid",
            ErrorStage::Enriching,
            error.to_string(),
        )
    })
}

/// Deliver candidates only after their source generation has committed.
/// Sink/provider failures are explicitly degraded evidence output; they do not
/// roll back already-published RAG/vector state.
pub(super) async fn submit_committed_candidates(
    sink: &dyn ArtifactCandidateSink,
    job_id: JobId,
    source_id: SourceId,
    generation: &SourceGenerationId,
    candidates: Vec<ArtifactCandidate>,
) -> Vec<SourceWarning> {
    submit_committed_candidates_with_outcome(sink, job_id, source_id, generation, candidates)
        .await
        .warnings
}

pub(super) async fn submit_committed_candidates_with_outcome(
    sink: &dyn ArtifactCandidateSink,
    job_id: JobId,
    source_id: SourceId,
    generation: &SourceGenerationId,
    candidates: Vec<ArtifactCandidate>,
) -> CandidateDeliveryOutcome {
    if candidates.is_empty() {
        return CandidateDeliveryOutcome {
            warnings: Vec::new(),
            disposition: CandidateDeliveryDisposition::Terminal,
        };
    }

    let (mut candidates, mut warnings) = dedupe_generation_candidates(candidates, generation);
    if candidates.is_empty() {
        return CandidateDeliveryOutcome {
            warnings,
            disposition: CandidateDeliveryDisposition::Terminal,
        };
    }

    let (capability, batch_size) = match candidate_sink_capability(sink).await {
        Ok(value) => value,
        Err(warning) => {
            let disposition = if warning.retryable {
                CandidateDeliveryDisposition::Retryable
            } else {
                CandidateDeliveryDisposition::Terminal
            };
            warnings.push(warning);
            return CandidateDeliveryOutcome {
                warnings,
                disposition,
            };
        }
    };
    let mut disposition = CandidateDeliveryDisposition::Terminal;
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    for (chunk_index, chunk) in candidates.chunks(batch_size).enumerate() {
        let delivery = submit_candidate_chunk(
            sink,
            &capability.name,
            job_id,
            &source_id,
            generation,
            chunk,
        )
        .await;
        warnings.extend(delivery.warnings);
        if delivery.retryable {
            disposition = CandidateDeliveryDisposition::Retryable;
        }
        if delivery.stop_delivery {
            let visited = (chunk_index + 1)
                .saturating_mul(batch_size)
                .min(candidates.len());
            let skipped = candidates.len().saturating_sub(visited);
            if skipped > 0 {
                warnings.push(warning(
                    "source.artifact_candidate.sink_delivery_skipped",
                    format!(
                        "artifact candidate sink delivery stopped after provider backpressure; {skipped} candidates were not attempted"
                    ),
                    false,
                ));
            }
            break;
        }
    }
    CandidateDeliveryOutcome {
        warnings,
        disposition,
    }
}

/// Apply the candidate identity invariant once over the fully accumulated
/// generation. Collection happens per changed-document chunk, so chunk-local
/// checks alone cannot detect duplicates or conflicting reuse of an id.
fn dedupe_generation_candidates(
    candidates: Vec<ArtifactCandidate>,
    generation: &SourceGenerationId,
) -> (Vec<ArtifactCandidate>, Vec<SourceWarning>) {
    let mut by_id = BTreeMap::new();
    let mut collided = BTreeSet::new();
    let mut warnings = Vec::new();

    for candidate in candidates {
        if collided.contains(&candidate.id) {
            continue;
        }
        match by_id.entry(candidate.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            std::collections::btree_map::Entry::Occupied(entry) if entry.get() == &candidate => {
                warnings.push(warning(
                    "source.artifact_candidate.duplicate_suppressed",
                    format!(
                        "duplicate artifact candidate {} was suppressed across generation {}",
                        candidate.id.0, generation.0
                    ),
                    false,
                ));
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                let id = entry.key().clone();
                entry.remove();
                collided.insert(id.clone());
                warnings.push(warning(
                    "source.artifact_candidate.identity_collision_rejected",
                    format!(
                        "conflicting artifact candidates reused id {} in generation {}; all candidates with that id were rejected",
                        id.0, generation.0
                    ),
                    false,
                ));
            }
        }
    }

    (by_id.into_values().collect(), warnings)
}

async fn candidate_sink_capability(
    sink: &dyn ArtifactCandidateSink,
) -> Result<(ArtifactCandidateSinkCapability, usize), SourceWarning> {
    let capability = sink.capabilities().await.map_err(|error| {
        let retryable = error.retryable;
        warning(
            "source.artifact_candidate.sink_capability_failed",
            format!("artifact candidate sink capability probe failed: {error}"),
            retryable,
        )
    })?;
    let batch_size = validate_sink_capability(&capability)?;
    Ok((capability, batch_size))
}

fn validate_sink_capability(
    capability: &ArtifactCandidateSinkCapability,
) -> Result<usize, SourceWarning> {
    if !capability
        .contract_versions
        .iter()
        .any(|version| version == ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION)
    {
        return Err(warning(
            "source.artifact_candidate.sink_contract_unsupported",
            format!(
                "artifact candidate sink {} does not advertise contract version {}",
                capability.name, ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION
            ),
            false,
        ));
    }
    if !capability.supports_idempotency {
        return Err(warning(
            "source.artifact_candidate.sink_idempotency_unsupported",
            format!(
                "artifact candidate sink {} does not support idempotent delivery",
                capability.name
            ),
            false,
        ));
    }
    let batch_size = usize::try_from(capability.max_batch_size)
        .unwrap_or(usize::MAX)
        .min(MAX_CANDIDATE_SINK_BATCH_SIZE);
    if batch_size == 0 {
        return Err(warning(
            "source.artifact_candidate.sink_zero_batch_limit",
            format!(
                "artifact candidate sink {} advertised max_batch_size=0",
                capability.name
            ),
            false,
        ));
    }
    Ok(batch_size)
}

async fn submit_candidate_chunk(
    sink: &dyn ArtifactCandidateSink,
    sink_name: &str,
    job_id: JobId,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    candidates: &[ArtifactCandidate],
) -> CandidateDeliveryResult {
    let idempotency_key =
        artifact_candidate_batch_idempotency_key(&job_id, source_id, generation, candidates);
    let batch = ArtifactCandidateBatch {
        contract_version: ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string(),
        delivery_id: format!("axon-artifact-candidates:{idempotency_key}"),
        idempotency_key,
        job_id,
        source_id: source_id.clone(),
        generation: generation.clone(),
        produced_at: timestamp(),
        candidates: candidates.to_vec(),
    };
    match sink.submit(batch).await {
        Ok(receipt) => receipt_warnings(sink_name, receipt, candidates.len() as u64),
        Err(error) => CandidateDeliveryResult {
            retryable: error.retryable,
            warnings: vec![warning(
                "source.artifact_candidate.sink_failed",
                format!("artifact candidate sink delivery failed: {error}"),
                error.retryable,
            )],
            stop_delivery: true,
        },
    }
}

fn receipt_warnings(
    sink_name: &str,
    mut receipt: ArtifactCandidateSinkResult,
    expected_attempted: u64,
) -> CandidateDeliveryResult {
    let mut warnings = std::mem::take(&mut receipt.warnings);
    if !valid_receipt(&receipt, expected_attempted) {
        warnings.push(warning(
            "source.artifact_candidate.sink_receipt_invalid",
            format!(
                "artifact candidate sink {sink_name} returned an invalid {:?} receipt: attempted={} accepted={} rejected={} expected_attempted={expected_attempted}",
                receipt.status, receipt.attempted, receipt.accepted, receipt.rejected
            ),
            false,
        ));
        return CandidateDeliveryResult {
            warnings,
            stop_delivery: true,
            retryable: false,
        };
    }
    let stop_delivery = matches!(
        receipt.status,
        ArtifactCandidateSinkStatus::Partial | ArtifactCandidateSinkStatus::Rejected
    );
    match receipt.status {
        ArtifactCandidateSinkStatus::Disabled => warnings.push(warning(
            "source.artifact_candidate.sink_disabled",
            format!(
                "artifact candidate sink {sink_name} is disabled; {} candidates were not accepted",
                receipt.attempted
            ),
            false,
        )),
        ArtifactCandidateSinkStatus::Accepted => warnings.push(warning(
            "source.artifact_candidate.sink_accepted",
            format!(
                "artifact candidate sink {sink_name} accepted all {} attempted candidates",
                receipt.accepted
            ),
            false,
        )),
        ArtifactCandidateSinkStatus::Partial => warnings.push(warning(
            "source.artifact_candidate.sink_partial",
            format!(
                "artifact candidate sink accepted {} of {} attempted candidates",
                receipt.accepted, receipt.attempted
            ),
            false,
        )),
        ArtifactCandidateSinkStatus::Rejected => warnings.push(warning(
            "source.artifact_candidate.sink_rejected",
            format!(
                "artifact candidate sink rejected {} of {} attempted candidates",
                receipt.rejected, receipt.attempted
            ),
            false,
        )),
    }
    CandidateDeliveryResult {
        warnings,
        stop_delivery,
        retryable: false,
    }
}

fn valid_receipt(receipt: &ArtifactCandidateSinkResult, expected_attempted: u64) -> bool {
    if receipt.attempted != expected_attempted {
        return false;
    }
    match receipt.status {
        ArtifactCandidateSinkStatus::Disabled => receipt.accepted == 0 && receipt.rejected == 0,
        ArtifactCandidateSinkStatus::Accepted => {
            receipt.accepted == expected_attempted && receipt.rejected == 0
        }
        ArtifactCandidateSinkStatus::Partial => {
            receipt.accepted > 0
                && receipt.rejected > 0
                && receipt.accepted.saturating_add(receipt.rejected) == expected_attempted
        }
        ArtifactCandidateSinkStatus::Rejected => {
            receipt.accepted == 0 && receipt.rejected == expected_attempted
        }
    }
}

fn warning(code: &str, message: String, retryable: bool) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: None,
        retryable,
    }
}

#[cfg(test)]
#[path = "artifact_candidates_tests.rs"]
mod tests;
