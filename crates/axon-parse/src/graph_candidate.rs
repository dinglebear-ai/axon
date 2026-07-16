use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    MetadataMap, SourceRange,
};
use serde_json::json;

pub const MODULE_NAME: &str = "graph_candidate";

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
        line.map(|line| crate::facts::line_range(line)),
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
    let line = range.as_ref().and_then(|range| range.line_start);
    let source_scope = format!(
        "source_id={}|item={}|uri={}",
        input.document.source_id.0, input.document.source_item_key.0, input.document.canonical_uri
    );
    let file_key = format!("repo_file:{}", stable_token(&source_scope));
    let item_identity = format!("{source_scope}|kind={kind}|name={name}");
    let item_token = stable_token(&item_identity);
    let candidate_id = format!("cand_{kind}_{item_token}");
    let item_key = format!("{kind}:{item_token}");
    let evidence_id = format!(
        "ev_{}",
        stable_token(&format!("{candidate_id}|line={line:?}|quote={quote:?}"))
    );

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
    let candidate_token = stable_token(&format!(
        "{}:{}:{to_stable_key}:{line:?}",
        input.document.canonical_uri, candidate_kind
    ));
    let evidence_token = stable_token(&format!(
        "{candidate_kind}:{to_stable_key}:{line:?}:{quote:?}"
    ));

    GraphCandidate {
        candidate_id: format!("cand_{candidate_kind}_{candidate_token}"),
        job_id: input.job_id,
        source_id: input.document.source_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        item_canonical_uri: input.document.canonical_uri.clone(),
        document_id: Some(input.document.document_id.clone()),
        kind: candidate_kind.to_string(),
        merge_key: Some(format!(
            "{candidate_kind}:{}:{to_stable_key}",
            input.document.canonical_uri
        )),
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
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!("ev_{evidence_token}"),
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

fn stable_token(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
#[path = "graph_candidate_tests.rs"]
mod tests;
