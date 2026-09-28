//! Item contribution retirement; shared parser identities remain available.
use super::*;

pub(super) fn retire(
    state: &mut FakeGraphState,
    source: &SourceId,
    item: &SourceItemKey,
) -> GraphDeleteResult {
    let mut deleted = GraphDeleteResult::default();
    state.edges_by_id.retain(|_, edge| {
        let before = edge.evidence.len();
        edge.evidence.retain(|ev| {
            &ev.source_id != source
                || ev
                    .metadata
                    .get("contained_source_item_key")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&ev.source_item_key.0)
                    != item.0
        });
        let unsupported = before != edge.evidence.len() && edge.evidence.is_empty();
        deleted.edges_deleted += u64::from(unsupported);
        !unsupported
    });
    if let Some(id) = state.node_id_by_key.get(&item.0).cloned() {
        let owned = state
            .nodes_by_id
            .get(&id)
            .is_some_and(|node| node.source_ids == [source.clone()]);
        let supported = state
            .edges_by_id
            .values()
            .any(|edge| edge.from_node_id == id || edge.to_node_id == id);
        if owned && !supported {
            state.nodes_by_id.remove(&id);
            state.node_id_by_key.remove(&item.0);
            deleted.nodes_deleted = 1;
        }
    }
    deleted
}
