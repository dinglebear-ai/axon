//! Journal candidates use the same checked redaction as canonical graph writes.
use super::Result;
use axon_api::source::{GraphCandidate, MetadataMap};
use axon_core::redact::{
    DefaultRedactor, RedactionContext, Redactor, redact_metadata_checked, stamp_redaction_metadata,
};

pub(super) fn sanitize(mut candidate: GraphCandidate) -> Result<GraphCandidate> {
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
    candidate.metadata = sanitize_metadata(candidate.metadata, &redactor, &context)?;
    for node in &mut candidate.nodes {
        node.label = redactor.redact_text(&node.label, &context);
        node.properties =
            sanitize_metadata(std::mem::take(&mut node.properties), &redactor, &context)?;
    }
    for edge in &mut candidate.edges {
        edge.properties =
            sanitize_metadata(std::mem::take(&mut edge.properties), &redactor, &context)?;
    }
    for evidence in &mut candidate.evidence {
        evidence.quote = evidence
            .quote
            .take()
            .map(|quote| redactor.redact_text(&quote, &context));
        evidence.metadata =
            sanitize_metadata(std::mem::take(&mut evidence.metadata), &redactor, &context)?;
    }
    Ok(candidate)
}
fn sanitize_metadata(
    metadata: MetadataMap,
    redactor: &DefaultRedactor,
    context: &RedactionContext,
) -> Result<MetadataMap> {
    let (metadata, report) = redact_metadata_checked(metadata, context, redactor)?;
    Ok(stamp_redaction_metadata(metadata, &report))
}
