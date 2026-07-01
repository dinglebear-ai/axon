use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    MetadataMap, SourceRange,
};
use serde_json::json;

pub const MODULE_NAME: &str = "graph_candidate";

use crate::parser::ParseInput;

pub fn graph_candidate(
    input: &ParseInput,
    parser_id: &str,
    kind: &str,
    name: &str,
    line: Option<u32>,
    quote: Option<String>,
) -> GraphCandidate {
    let source_key = sanitize(input.document.source_item_key.0.as_str());
    let name_key = sanitize(name);
    let candidate_id = format!("cand_{source_key}_{kind}_{name_key}");
    let file_key = format!("source:{}", input.document.canonical_uri);
    let item_key = format!("{kind}:{name_key}");

    let mut evidence_metadata = MetadataMap::new();
    evidence_metadata.insert("parser_id".to_string(), json!(parser_id));

    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: input.job_id,
        source_id: input.document.source_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        item_canonical_uri: input.document.canonical_uri.clone(),
        document_id: Some(input.document.document_id.clone()),
        kind: kind.to_string(),
        merge_key: Some(format!("{kind}:{}:{name}", input.document.canonical_uri)),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some(parser_id.to_string()),
            version: "pr8-baseline".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "source_item".to_string(),
                stable_key: file_key.clone(),
                label: input.document.source_item_key.0.clone(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: kind.to_string(),
                stable_key: item_key.clone(),
                label: name.to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "declares".to_string(),
            from_stable_key: file_key,
            to_stable_key: item_key,
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!("ev_{candidate_id}"),
            evidence_kind: "source_line".to_string(),
            source_id: input.document.source_id.clone(),
            source_item_key: input.document.source_item_key.clone(),
            document_id: Some(input.document.document_id.clone()),
            chunk_id: None,
            range: line.map(|line| SourceRange {
                line_start: Some(line),
                line_end: Some(line),
                byte_start: None,
                byte_end: None,
                char_start: None,
                char_end: None,
                time_start_ms: None,
                time_end_ms: None,
                dom_selector: None,
                json_pointer: None,
                yaml_path: None,
                xml_xpath: None,
                csv_row: None,
                session_turn_id: None,
                turn_start: None,
                turn_end: None,
            }),
            quote,
            confidence: 0.9,
            metadata: evidence_metadata,
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
