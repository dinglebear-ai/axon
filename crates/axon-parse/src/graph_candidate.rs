use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    MetadataMap, SourceRange,
};
use serde_json::json;

pub const MODULE_NAME: &str = "graph_candidate";

mod evidence_quote;

use crate::facts::PARSER_VERSION;
use crate::parser::ParseInput;

pub fn graph_candidate(
    input: &ParseInput,
    parser_id: &str,
    kind: &str,
    name: &str,
    line: Option<u32>,
    quote: Option<String>,
) -> GraphCandidate {
    graph_candidate_ranged(
        input,
        parser_id,
        kind,
        name,
        line.map(crate::facts::line_range),
        quote,
    )
}

pub fn graph_candidate_ranged(
    input: &ParseInput,
    parser_id: &str,
    kind: &str,
    name: &str,
    range: Option<SourceRange>,
    quote: Option<String>,
) -> GraphCandidate {
    let (quote, excerpted, original_bytes) = evidence_quote::bounded(quote, name);
    // JSON tuples preserve field boundaries even when paths/names contain
    // delimiters. Identity is source-scoped and independent of job/line moves.
    let source_scope = source_scope(input);
    let file_key = format!("repo_file:{}", stable_token(&source_scope));
    let item_identity = json!([source_scope, kind, name]).to_string();
    let item_token = stable_token(&item_identity);
    let candidate_id = format!("cand_{kind}_{item_token}");
    let item_key = format!("{kind}:{item_token}");
    let evidence_id = format!(
        "ev_{}",
        stable_token(&json!([candidate_id, parser_id, range, quote]).to_string())
    );

    let mut evidence_metadata = MetadataMap::new();
    evidence_metadata.insert("parser_id".to_string(), json!(parser_id));
    if excerpted {
        evidence_metadata.insert("quote_excerpted".into(), json!(true));
        evidence_metadata.insert("quote_original_bytes".into(), json!(original_bytes));
    }

    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: input.job_id,
        source_id: input.document.source_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        item_canonical_uri: input.document.canonical_uri.clone(),
        document_id: Some(input.document.document_id.clone()),
        kind: kind.to_string(),
        merge_key: Some(format!("{kind}:{item_token}")),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some(parser_id.to_string()),
            version: PARSER_VERSION.to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "repo_file".to_string(),
                stable_key: file_key.clone(),
                label: input.document.source_item_key.0.clone(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "artifact".to_string(),
                stable_key: item_key.clone(),
                label: name.to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "source_indexed_as".to_string(),
            from_stable_key: file_key,
            to_stable_key: item_key,
            evidence_ids: vec![evidence_id.clone()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: "text_mention".to_string(),
            source_id: input.document.source_id.clone(),
            source_item_key: input.document.source_item_key.clone(),
            document_id: Some(input.document.document_id.clone()),
            chunk_id: None,
            range,
            quote,
            confidence: 0.9,
            metadata: evidence_metadata,
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

pub fn candidate_edge(
    input: &ParseInput,
    parser_id: &str,
    candidate_kind: &str,
    from_node_kind: &str,
    from_stable_key: &str,
    to_node_kind: &str,
    to_stable_key: &str,
    edge_kind: &str,
    evidence_kind: &str,
    line: Option<u32>,
    quote: Option<String>,
) -> GraphCandidate {
    let evidence_range = line.map(|line| SourceRange {
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
    });
    let identity = json!([
        source_scope(input),
        candidate_kind,
        from_node_kind,
        from_stable_key,
        to_node_kind,
        to_stable_key,
        edge_kind
    ])
    .to_string();
    let candidate_token = stable_token(&identity);
    let evidence_token =
        stable_token(&json!([identity, parser_id, evidence_range, quote]).to_string());
    let evidence_id = format!("ev_{evidence_token}");

    GraphCandidate {
        candidate_id: format!("cand_{candidate_kind}_{candidate_token}"),
        job_id: input.job_id,
        source_id: input.document.source_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        item_canonical_uri: input.document.canonical_uri.clone(),
        document_id: Some(input.document.document_id.clone()),
        kind: candidate_kind.to_string(),
        merge_key: Some(format!("{candidate_kind}:{candidate_token}")),
        producer: GraphCandidateProducer {
            adapter: "axon-parse".to_string(),
            parser: Some(parser_id.to_string()),
            version: PARSER_VERSION.to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: from_node_kind.to_string(),
                stable_key: from_stable_key.to_string(),
                label: input.document.source_item_key.0.clone(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: to_node_kind.to_string(),
                stable_key: to_stable_key.to_string(),
                label: to_stable_key.to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: edge_kind.to_string(),
            from_stable_key: from_stable_key.to_string(),
            to_stable_key: to_stable_key.to_string(),
            evidence_ids: vec![evidence_id.clone()],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: evidence_kind.to_string(),
            source_id: input.document.source_id.clone(),
            source_item_key: input.document.source_item_key.clone(),
            document_id: Some(input.document.document_id.clone()),
            chunk_id: None,
            range: evidence_range,
            quote,
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

fn source_scope(input: &ParseInput) -> String {
    json!([
        input.document.source_id.0,
        input.document.source_item_key.0,
        input.document.canonical_uri
    ])
    .to_string()
}

fn stable_token(value: &str) -> String {
    // UUIDv5 gives a fixed-width 122-bit identity, rather than truncating or
    // sanitizing names (which would merge distinct facts). Display labels and
    // evidence retain the complete source text; only identity is encoded.
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, value.as_bytes())
        .simple()
        .to_string()
}

#[cfg(test)]
#[path = "graph_candidate_tests.rs"]
mod tests;
