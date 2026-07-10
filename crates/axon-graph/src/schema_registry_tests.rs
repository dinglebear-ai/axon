use super::*;

/// Extract every `` `snake_case` `` kind name from the first column of a
/// markdown table under the given `## <heading>` section, up to the next
/// `## ` heading. Mirrors the doc contract in graph-schema.md: "Schema
/// generation must fail when `source-graph.md` contains a node or edge kind
/// absent from the generated graph schema" (and vice versa).
fn table_kinds(markdown: &str, heading: &str) -> Vec<String> {
    let start = markdown
        .find(heading)
        .unwrap_or_else(|| panic!("source-graph.md missing heading {heading:?}"));
    let rest = &markdown[start + heading.len()..];
    let section = rest.find("\n## ").map(|end| &rest[..end]).unwrap_or(rest);

    section
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| {
            let first_cell = line.trim_start_matches('|').split('|').next()?.trim();
            let name = first_cell.trim_matches('`');
            if name.is_empty() || name == "Kind" || name == "Edge" || name.starts_with('-') {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

fn source_graph_md() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/pipeline-unification/sources/source-graph.md"
    );
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"))
}

#[test]
fn node_kind_registry_matches_source_graph_md() {
    let markdown = source_graph_md();
    let doc_kinds = table_kinds(&markdown, "## Node Kinds");
    let code_kinds: Vec<String> = GraphNodeKind::ALL
        .iter()
        .map(|k| k.as_str().to_string())
        .collect();

    for kind in &doc_kinds {
        assert!(
            code_kinds.contains(kind),
            "source-graph.md node kind {kind:?} is not in GraphNodeKind::ALL"
        );
    }
    for kind in &code_kinds {
        assert!(
            doc_kinds.contains(kind),
            "GraphNodeKind::{kind:?} has no entry in source-graph.md's Node Kinds table"
        );
    }
}

#[test]
fn edge_kind_registry_matches_source_graph_md() {
    let markdown = source_graph_md();
    let doc_kinds = table_kinds(&markdown, "## Edge Kinds");
    let code_kinds: Vec<String> = GraphEdgeKind::ALL
        .iter()
        .map(|k| k.as_str().to_string())
        .collect();

    for kind in &doc_kinds {
        assert!(
            code_kinds.contains(kind),
            "source-graph.md edge kind {kind:?} is not in GraphEdgeKind::ALL"
        );
    }
    for kind in &code_kinds {
        assert!(
            doc_kinds.contains(kind),
            "GraphEdgeKind::{kind:?} has no entry in source-graph.md's Edge Kinds table"
        );
    }
}

#[test]
fn node_kind_registry_is_generated_from_the_closed_enum() {
    let registry = node_kind_registry();
    assert_eq!(registry.len(), GraphNodeKind::ALL.len());
    assert!(registry.iter().all(|spec| spec.requires_evidence));
    assert!(
        registry
            .iter()
            .zip(GraphNodeKind::ALL.iter())
            .all(|(spec, kind)| spec.kind == kind.as_str())
    );
}

#[test]
fn edge_kind_registry_is_generated_from_the_closed_enum() {
    let registry = edge_kind_registry();
    assert_eq!(registry.len(), GraphEdgeKind::ALL.len());
    assert!(registry.iter().all(|spec| spec.requires_evidence));
    assert!(
        registry
            .iter()
            .zip(GraphEdgeKind::ALL.iter())
            .all(|(spec, kind)| spec.kind == kind.as_str())
    );
}
