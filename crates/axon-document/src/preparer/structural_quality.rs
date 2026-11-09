//! Structural evidence preserves concise code and records without lowering the prose floor.
use super::skipped_document;
use crate::prepared::PrepareSourceDocumentRequest;
use crate::{chunk::DocumentChunk, profile::ChunkingProfile};
use axon_api::source::{ContentKind, ContentSkipReason, SkippedDocument, SourceParseFacts};

pub(super) fn minimum(
    facts: &[SourceParseFacts],
    profile: ChunkingProfile,
    kind: ContentKind,
    chunks: &[DocumentChunk],
    source: &str,
    configured: usize,
) -> usize {
    if source.chars().count() >= configured {
        return configured;
    }
    let ast = profile == ChunkingProfile::CodeSymbol
        && facts.iter().any(|fact| {
            fact.fact_kind == "code_parse_outcome"
                && fact.parser_method == "tree_sitter"
                && crate::PreparationObservation::from_value(&fact.value).ast_status()
                    == Some("parsed")
                && fact
                    .value
                    .get("code_searchable_structure")
                    .and_then(|v| v.as_bool())
                    == Some(true)
        });
    let records = axon_parse::config::has_record_structure(kind, source)
        && profile == ChunkingProfile::StructuredRecords
        && !chunks.is_empty()
        && chunks.iter().all(|chunk| {
            chunk
                .range
                .json_pointer
                .as_deref()
                .is_some_and(|p| !p.is_empty())
                || chunk.range.yaml_path.is_some()
                || chunk.range.xml_xpath.is_some()
                || chunk.range.csv_row.is_some()
                || chunk
                    .metadata
                    .get("toml_table")
                    .and_then(|v| v.as_str())
                    .is_some()
        });
    if ast || records { 1 } else { configured }
}

pub(super) fn quality_filtered_document(
    request: PrepareSourceDocumentRequest,
    input_bytes: usize,
    minimum_chunk_chars: usize,
) -> SkippedDocument {
    tracing::info!(
        target: "axon_document::prepare",
        stage = "preparing",
        operation = "chunk_quality_filter",
        reason = "no_searchable_chunks",
        source_id = %request.document.source_id.0,
        source_item_key = %request.document.source_item_key.0,
        input_bytes,
        minimum_chunk_chars,
        "nonempty content has no chunks after bounded quality filtering; inspect structural projection and configured minimum before retrying"
    );
    skipped_document(request, ContentSkipReason::NoSearchableChunks)
}

/// Recover validated records when projection loses all chunks or source coordinates.
/// Source-backed windows retain field names and exact redacted-source coordinates.
pub(super) fn recover_filtered_records(
    chunks: Vec<DocumentChunk>,
    profile: ChunkingProfile,
    kind: ContentKind,
    source: &str,
    configured: usize,
    max_chars: usize,
    item: &axon_api::source::SourceItemKey,
) -> (Vec<DocumentChunk>, Option<axon_api::source::SourceWarning>) {
    let unlocated = chunks.iter().any(|chunk| {
        chunk
            .range
            .byte_start
            .zip(chunk.range.byte_end)
            .is_none_or(|(start, end)| start >= end)
    });
    if (!chunks.is_empty() && !unlocated)
        || !matches!(
            profile,
            ChunkingProfile::CodeManifest
                | ChunkingProfile::StructuredRecords
                | ChunkingProfile::ApiSchema
        )
        || !axon_parse::config::has_record_structure(kind, source)
    {
        return (chunks, None);
    }
    let minimum = if source.chars().count() < configured {
        1
    } else {
        configured
    };
    let recovered = crate::quality::useful_chunks_with_limit(
        crate::text::plain_text_windows_with_limits(
            source,
            crate::text::MAX_PLAIN_TEXT_CHUNK_BYTES,
            max_chars,
        ),
        source,
        max_chars,
        minimum,
    )
    .into_iter()
    .map(|chunk| {
        chunk
            .with_metadata("actual_chunking_method", "plain_text_windows".into())
            .with_metadata(
                "chunking_fallback",
                if unlocated {
                    "unlocated_structured_records"
                } else {
                    "quality_filtered_structured_records"
                }
                .into(),
            )
    })
    .collect::<Vec<_>>();
    if recovered.is_empty() {
        return (recovered, None);
    }
    let notice = super::warning(
        "chunk.structured_quality_fallback",
        "structural projection lost every chunk at the configured quality floor or lacked nonempty source byte ranges; indexed bounded literal windows of the complete validated redacted configuration instead; field names and source coordinates are preserved",
        item,
    );
    (recovered, Some(notice))
}

impl super::DocumentPreparer {
    pub(super) fn finish_quality(
        &self,
        request: &PrepareSourceDocumentRequest,
        profile: ChunkingProfile,
        chunks: Vec<DocumentChunk>,
        source: &str,
        method: &str,
        markdown_limits: crate::markdown::MarkdownChunkLimits,
    ) -> (Vec<DocumentChunk>, Option<axon_api::source::SourceWarning>) {
        let max_chars = markdown_limits
            .max_chars()
            .min(crate::text::MAX_PLAIN_TEXT_CHUNK_CHARS);
        let configured = self.semantic_config().minimum_chunk_chars.min(max_chars);
        let minimum = minimum(
            &request.parse_facts,
            profile,
            request.document.content_kind,
            &chunks,
            source,
            configured,
        );
        let mut chunks =
            super::finalize_chunks(profile, chunks, source, markdown_limits, method, minimum);
        // Syntactic locators from a permissive structured chunker do not prove
        // that malformed input or a scalar is a complete searchable record.
        // Keep the normal prose floor for those inputs while retaining valid
        // short records and independent session/temporal identities elsewhere.
        if matches!(
            request.document.content_kind,
            ContentKind::Json | ContentKind::Yaml | ContentKind::Toml
        ) && !axon_parse::config::has_record_structure(request.document.content_kind, source)
        {
            chunks.retain(|chunk| chunk.content.trim().chars().count() >= configured);
        }
        recover_filtered_records(
            chunks,
            profile,
            request.document.content_kind,
            source,
            configured,
            max_chars,
            &request.document.source_item_key,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_recovery_preserves_scoped_character_ceiling() {
        let source = serde_json::json!({
            "instructions": "Useful configuration context with literal field values. ".repeat(80)
        })
        .to_string();
        let (chunks, warning) = recover_filtered_records(
            Vec::new(),
            ChunkingProfile::StructuredRecords,
            ContentKind::Json,
            &source,
            200,
            1_000,
            &axon_api::source::SourceItemKey::from("config.json"),
        );
        assert!(warning.is_some());
        assert!(chunks.len() > 1);
        for chunk in chunks {
            assert!(chunk.content.chars().count() <= 1_000);
            let start = chunk.range.byte_start.unwrap() as usize;
            let end = chunk.range.byte_end.unwrap() as usize;
            assert_eq!(chunk.content, source[start..end]);
        }
    }
}
