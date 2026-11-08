//! Evidence-backed baseline source/document containment candidates.
use super::*;

/// Lazily assemble bounded baseline graph candidates. Every chunk repeats the
/// source container node so containment edges resolve locally. Each chunk is
/// candidate-atomic; generation retries reconcile any visible committed prefix.
pub(super) fn baseline_candidates<'a>(
    kind: SourceKind,
    counts: &'a IndexCounts,
    canonical_uri: &'a str,
    manifest: &'a SourceManifest,
) -> impl Iterator<Item = GraphCandidate> + 'a {
    let empty = manifest.items.is_empty().then_some(&manifest.items[..]);
    empty
        .into_iter()
        .chain(manifest.items.chunks(BASELINE_GRAPH_BATCH_SIZE))
        .enumerate()
        .map(move |(batch_index, items)| {
            build_candidate_batch(
                kind.clone(),
                manifest.scope,
                counts,
                canonical_uri,
                items,
                batch_index,
            )
        })
}

/// Assemble the full baseline candidate for tests and small direct callers.
#[cfg(test)]
pub(super) fn build_candidate(
    kind: SourceKind,
    counts: &IndexCounts,
    canonical_uri: &str,
    manifest: &SourceManifest,
) -> GraphCandidate {
    build_candidate_batch(
        kind,
        manifest.scope,
        counts,
        canonical_uri,
        &manifest.items,
        0,
    )
}

fn build_candidate_batch(
    kind: SourceKind,
    scope: SourceScope,
    counts: &IndexCounts,
    canonical_uri: &str,
    items: &[ManifestItem],
    batch_index: usize,
) -> GraphCandidate {
    let source_id = counts.source_id.clone();
    let source_item_key = SourceItemKey::new(canonical_uri);
    let container_key = container_stable_key(&source_id, canonical_uri);
    // Carry the real canonical URI as a node property: without it,
    // `canonical_uri_for` falls back to the composite stable key, the
    // canonical_uri alias stores that composite, and `graph resolve <uri>` /
    // `graph query <uri>` can never match a plain URI (seen live on the
    // reset 7.0 stores).
    let container = GraphNodeCandidate {
        node_kind: container_node_kind(kind, scope).to_string(),
        stable_key: container_key.clone(),
        label: canonical_uri.to_string(),
        properties: uri_properties(canonical_uri),
    };

    let mut nodes = vec![container];
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    let edge_kind = containment_edge_kind(kind, scope);

    for item in items {
        let doc_key = document_stable_key(item);
        let item_evidence = containment_evidence(&source_id, &source_item_key, item);
        nodes.push(GraphNodeCandidate {
            node_kind: document_node_kind(item).to_string(),
            stable_key: doc_key.clone(),
            label: item.canonical_uri.clone(),
            properties: uri_properties(&item.canonical_uri),
        });
        edges.push(GraphEdgeCandidate {
            edge_kind: edge_kind.to_string(),
            from_stable_key: container_key.clone(),
            to_stable_key: doc_key,
            evidence_ids: vec![item_evidence.evidence_id.clone()],
            properties: MetadataMap::new(),
        });
        evidence.push(item_evidence);
    }

    let candidate_id = if batch_index == 0 {
        format!("source-baseline:{}:{}", source_id.0, counts.generation.0)
    } else {
        format!(
            "source-baseline:{}:{}:{batch_index}",
            source_id.0, counts.generation.0
        )
    };
    GraphCandidate {
        candidate_id,
        job_id: counts.job_id.clone(),
        source_id: source_id.clone(),
        source_item_key,
        item_canonical_uri: canonical_uri.to_string(),
        document_id: None,
        kind: "source_baseline".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: crate::source::adapter_name_for(kind).to_string(),
            parser: None,
            version: PRODUCER_VERSION.to_string(),
        },
        nodes,
        edges,
        evidence,
        confidence: BASELINE_CONFIDENCE,
        metadata: MetadataMap::new(),
    }
}

/// One `text_mention` evidence record per containment edge. The manifest is the
/// direct observation that the item belongs to this source, so it justifies the
/// containment claim (edges are never "just true").
fn containment_evidence(
    source_id: &SourceId,
    candidate_source_item_key: &SourceItemKey,
    item: &ManifestItem,
) -> GraphEvidence {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "contained_source_item_key".to_string(),
        serde_json::json!(item.source_item_key.0),
    );
    GraphEvidence {
        evidence_id: format!("contains:{}", item.source_item_key.0),
        evidence_kind: "text_mention".to_string(),
        source_id: source_id.clone(),
        source_item_key: candidate_source_item_key.clone(),
        document_id: None,
        chunk_id: None,
        range: None,
        quote: None,
        confidence: BASELINE_CONFIDENCE,
        metadata,
    }
}

/// Stable key for the container node — the source's canonical URI, namespaced by
/// source id so distinct sources never collide on a shared URI shape.
/// Node properties carrying the plain canonical URI so graph merge stores it
/// (and its alias) as the node's canonical identity instead of falling back
/// to the composite stable key.
fn uri_properties(canonical_uri: &str) -> MetadataMap {
    let mut properties = MetadataMap::new();
    properties.insert(
        "canonical_uri".to_string(),
        serde_json::json!(canonical_uri),
    );
    properties
}

fn container_stable_key(source_id: &SourceId, canonical_uri: &str) -> String {
    format!("source:{}:{}", source_id.0, canonical_uri)
}

/// Stable key for a document node — the item's own stable source key.
fn document_stable_key(item: &ManifestItem) -> String {
    if item.item_kind == ItemKind::MemoryRecord {
        return format!("memory:{}", item.source_item_key.0);
    }
    item.source_item_key.0.clone()
}

/// Registry node kind for the source container, chosen per acquisition family.
/// Every returned name is a closed [`axon_graph::node::GraphNodeKind`] variant.
pub(super) fn container_node_kind(kind: SourceKind, scope: SourceScope) -> &'static str {
    match (kind, scope) {
        (SourceKind::Registry, SourceScope::Api) => "source",
        (SourceKind::Web, _) => "web_origin",
        (SourceKind::Git, _) => "repo",
        (SourceKind::Local, _) => "local_checkout",
        (SourceKind::Feed, _) => "feed",
        (SourceKind::Reddit, _) => "reddit_subreddit",
        (SourceKind::Youtube, _) => "youtube_channel",
        (SourceKind::Session, _) => "session",
        (SourceKind::Registry, _) => "package",
        (SourceKind::CliTool | SourceKind::McpTool, _) => "artifact",
        (SourceKind::Memory, _) => "source",
        (SourceKind::Upload, _) => "derived_source",
    }
}

/// Registry document-node kind, derived from the manifest item's [`ItemKind`].
pub(super) fn document_node_kind(item: &ManifestItem) -> &'static str {
    match item.item_kind {
        ItemKind::WebPage => "web_page",
        ItemKind::RepoFile => "repo_file",
        ItemKind::LocalFile => "repo_file",
        ItemKind::PackageVersion => "package_version",
        ItemKind::FeedEntry => "feed_entry",
        ItemKind::Transcript => "youtube_video",
        ItemKind::SessionTurn => "session_turn",
        ItemKind::ToolCall => "tool_call",
        ItemKind::CliOutput => "artifact",
        ItemKind::McpToolOutput => "artifact",
        ItemKind::MemoryRecord => "memory",
        ItemKind::Artifact => "artifact",
    }
}

/// Registry containment edge kind (container → document) per family. Every
/// returned name is a closed [`axon_graph::edge::GraphEdgeKind`] variant.
pub(super) fn containment_edge_kind(kind: SourceKind, scope: SourceScope) -> &'static str {
    if kind == SourceKind::Registry && scope == SourceScope::Api {
        return "source_indexed_as";
    }
    match kind {
        SourceKind::Web => "docs_site_contains_page",
        SourceKind::Git => "commit_contains_file",
        SourceKind::Local => "commit_contains_file",
        SourceKind::Feed => "feed_contains_entry",
        SourceKind::Reddit => "subreddit_has_thread",
        SourceKind::Youtube => "youtube_channel_has_video",
        SourceKind::Session => "session_has_turn",
        SourceKind::Registry => "package_has_version",
        SourceKind::CliTool | SourceKind::McpTool => "source_produced_artifact",
        SourceKind::Memory | SourceKind::Upload => "source_indexed_as",
    }
}
