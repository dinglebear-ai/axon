use serde_json::{Value, json};

pub(super) fn leaf_alternative_fields(action: &str, selector: &str) -> &'static [&'static str] {
    match (action, selector) {
        ("graph", "resolve") => &["id", "canonical_uri"],
        ("graph", "query") => &["id", "node_id"],
        ("watch", "status" | "exec" | "history") => &["id", "source"],
        ("uploads", "put_content") => &["content", "content_ref"],
        ("codex", "events") => &["cursor_boot_id", "after_sequence"],
        ("codex", "reconcile") => &["effect_applied", "disposition_note"],
        _ => &[],
    }
}

pub(super) fn leaf_field_combination(action: &str, selector: &str) -> Option<Value> {
    match (action, selector) {
        ("graph", "resolve") => Some(json!({"anyOf": [
            {"required": ["id"]}, {"required": ["canonical_uri"]}
        ]})),
        ("graph", "query") => Some(json!({"anyOf": [
            {"required": ["id"]}, {"required": ["node_id"]}
        ]})),
        ("watch", "status" | "exec" | "history") => Some(json!({"anyOf": [
            {"required": ["id"]}, {"required": ["source"]}
        ]})),
        ("uploads", "put_content") => Some(json!({"oneOf": [
            {"required": ["content"]}, {"required": ["content_ref"]}
        ]})),
        ("codex", "events") => Some(
            json!({"if": {"required": ["cursor_boot_id"]}, "then": {"required": ["after_sequence"]}, "else": {"not": {"required": ["after_sequence"]}}}),
        ),
        ("codex", "reconcile") => Some(
            json!({"if": {"properties": {"without_replay": {"const": true}}, "required": ["without_replay"]}, "then": {"required": ["effect_applied", "disposition_note"]}}),
        ),
        _ => None,
    }
}

pub(super) fn disallow_null(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = object.get_mut(key).and_then(Value::as_array_mut) {
            variants.retain(|variant| variant.get("type") != Some(&json!("null")));
            if variants.len() == 1 {
                *schema = variants.pop().expect("one non-null variant");
            }
            return;
        }
    }
    if let Some(types) = object.get_mut("type").and_then(Value::as_array_mut) {
        types.retain(|kind| kind != "null");
    }
}

pub(super) fn leaf_required_fields(action: &str, selector: &str) -> &'static [&'static str] {
    match (action, selector) {
        ("artifacts", "get" | "content") => &["artifact_id"],
        ("codex", "resource") => &["resource"],
        ("codex", "prepare") => &["mutation_action", "params", "idempotency_key"],
        ("codex", "approve" | "cancel" | "reconcile") => &["operation_id"],
        ("codex", "execute") => &["operation_id", "capability", "mutation_action", "params"],
        ("codex", "respond") => &["boot_id", "request_id", "approved"],
        ("collections", "get") => &["collection"],
        ("graph", "node" | "edge" | "source") => &["id"],
        ("jobs", "get" | "status" | "events" | "stream" | "cancel" | "retry") => &["job_id"],
        ("jobs", "recover") => &["stale_before"],
        ("jobs", "clear") => &["confirm"],
        ("memory", "remember") => &["body"],
        ("memory", "search") => &["query"],
        ("memory", "show" | "reinforce" | "pin" | "archive" | "forget") => &["id"],
        ("memory", "link" | "supersede" | "contradict") => &["source_id", "target_id"],
        ("memory", "compact") => &["memory_ids"],
        ("memory", "import") => &["records"],
        ("providers", "get") => &["provider_id"],
        ("prune", "plan") => &["target"],
        ("prune", "get") => &["plan_id"],
        ("prune", "exec") => &["plan_id", "confirm"],
        ("reset", "get") => &["plan_id"],
        ("reset", "exec") => &["plan_id", "confirm"],
        ("uploads", "create") => &["filename", "content_type", "size_bytes", "purpose"],
        ("uploads", "get" | "put_content" | "complete" | "abort") => &["upload_id"],
        ("watch", "create") => &["source", "every_seconds"],
        ("watch", "get" | "update" | "pause" | "resume" | "delete") => &["id"],
        _ => &[],
    }
}

/// Fields consumed by each finite leaf. Shapes still come from the canonical
/// request DTO; this table only removes sibling fields from flat family DTOs.
/// A new selector in one of these families must be classified explicitly.
pub(super) fn leaf_fields(action: &str, selector: &str) -> Option<&'static [&'static str]> {
    if action == "extract" && selector == "start" {
        return None;
    }
    let fields = match action {
        "artifacts" | "codex" | "collections" | "graph" | "jobs" => core_fields(action, selector),
        "memory" => memory_fields(action, selector),
        "providers" | "prune" | "reset" | "uploads" | "watch" => admin_fields(action, selector),
        _ => return None,
    };
    Some(fields.unwrap_or_else(|| panic!("unclassified MCP leaf {action}_{selector}")))
}

fn core_fields(action: &str, selector: &str) -> Option<&'static [&'static str]> {
    let fields: &[&str] = match (action, selector) {
        ("artifacts", "list") => &[
            "source_id",
            "job_id",
            "kind",
            "limit",
            "cursor",
            "response_mode",
        ],
        ("artifacts", "get" | "content") => &["artifact_id", "response_mode"],
        ("codex", "snapshot" | "operations") => &[],
        ("codex", "resource") => &["resource", "params"],
        ("codex", "events") => &["cursor_boot_id", "after_sequence", "limit"],
        ("codex", "prepare") => &["mutation_action", "params", "idempotency_key"],
        ("codex", "approve" | "cancel") => &["operation_id"],
        ("codex", "execute") => &["operation_id", "capability", "mutation_action", "params"],
        ("codex", "reconcile") => &[
            "operation_id",
            "without_replay",
            "effect_applied",
            "disposition_note",
        ],
        ("codex", "respond") => &["boot_id", "request_id", "approved", "params"],
        ("collections", "list") => &["prefix", "limit", "cursor", "response_mode"],
        ("collections", "get") => &["collection", "prefix", "response_mode"],
        ("graph", "kinds") => &["response_mode"],
        ("graph", "resolve") => &[
            "id",
            "canonical_uri",
            "kind",
            "include_edges",
            "response_mode",
        ],
        ("graph", "query") => &[
            "node_id",
            "id",
            "kind",
            "edges",
            "direction",
            "depth",
            "limit",
            "cursor",
            "response_mode",
        ],
        ("graph", "node") => &["id", "include_edges", "response_mode"],
        ("graph", "edge") => &["id", "response_mode"],
        ("graph", "source") => &["id", "depth", "edge_kind", "limit", "response_mode"],
        ("jobs", "list") => &[
            "status",
            "kind",
            "source_id",
            "watch_id",
            "limit",
            "cursor",
            "response_mode",
        ],
        ("jobs", "get" | "status") => &["job_id", "response_mode"],
        ("jobs", "events" | "stream") => &[
            "job_id",
            "after_sequence",
            "limit",
            "severity",
            "visibility",
            "since_sequence",
            "cursor",
            "response_mode",
        ],
        ("jobs", "cancel") => &["job_id", "reason", "response_mode"],
        ("jobs", "retry") => &[
            "job_id",
            "retry_mode",
            "from_phase",
            "idempotency_key",
            "overrides",
            "response_mode",
        ],
        ("jobs", "recover") => &["kind", "stale_before", "limit", "dry_run", "response_mode"],
        ("jobs", "cleanup") => &[
            "kind",
            "older_than",
            "status",
            "limit",
            "dry_run",
            "response_mode",
        ],
        ("jobs", "clear") => &["kind", "older_than", "status", "confirm", "response_mode"],
        _ => return None,
    };
    Some(fields)
}

fn memory_fields(action: &str, selector: &str) -> Option<&'static [&'static str]> {
    let fields: &[&str] = match (action, selector) {
        ("memory", "remember") => &[
            "body",
            "memory_type",
            "title",
            "confidence",
            "project",
            "repo",
            "file",
        ],
        ("memory", "list") => &["limit", "status", "memory_type", "project", "repo", "file"],
        ("memory", "search") => &[
            "query",
            "limit",
            "status",
            "memory_type",
            "project",
            "repo",
            "file",
        ],
        ("memory", "show") => &["id"],
        ("memory", "link") => &["source_id", "target_id", "edge_type"],
        ("memory", "supersede") => &["source_id", "target_id"],
        ("memory", "context") => &["query", "limit", "token_budget", "project", "repo", "file"],
        ("memory", "reinforce") => &["id", "amount"],
        ("memory", "contradict") => &["source_id", "target_id", "reason"],
        ("memory", "pin") => &["id", "pinned", "reason"],
        ("memory", "archive" | "forget") => &["id", "reason"],
        ("memory", "review") => &["limit", "memory_type", "reason"],
        ("memory", "compact") => &[
            "memory_ids",
            "strategy",
            "memory_type",
            "project",
            "repo",
            "file",
            "title",
            "archive_sources",
        ],
        ("memory", "import") => &["records", "import_mode", "dry_run"],
        ("memory", "export") => &["export_scope", "include_archived", "include_working"],
        _ => return None,
    };
    Some(fields)
}

fn admin_fields(action: &str, selector: &str) -> Option<&'static [&'static str]> {
    let fields: &[&str] = match (action, selector) {
        ("providers", "list") => &["response_mode"],
        ("providers", "get") => &["provider_id", "response_mode"],
        ("prune", "plan") => &["target", "generation", "response_mode"],
        ("prune", "get") => &["plan_id", "response_mode"],
        ("prune", "exec") => &["plan_id", "confirm", "response_mode"],
        ("reset", "plan") => &["stores", "collection", "include_artifacts", "response_mode"],
        ("reset", "get") => &["plan_id", "response_mode"],
        ("reset", "exec") => &[
            "plan_id",
            "confirm",
            "stores",
            "collection",
            "include_artifacts",
            "response_mode",
        ],
        ("uploads", "list") => &["status", "limit", "cursor", "response_mode"],
        ("uploads", "create") => &[
            "filename",
            "content_type",
            "size_bytes",
            "purpose",
            "sha256",
            "source_hint",
            "response_mode",
        ],
        ("uploads", "get") => &["upload_id", "response_mode"],
        ("uploads", "put_content") => &[
            "upload_id",
            "content",
            "content_ref",
            "sha256",
            "response_mode",
        ],
        ("uploads", "complete") => &["upload_id", "sha256", "source_options", "response_mode"],
        ("uploads", "abort") => &["upload_id", "reason", "response_mode"],
        ("watch", "create") => &[
            "source",
            "every_seconds",
            "embed",
            "options",
            "limits",
            "metadata",
            "scope",
            "collection",
            "enabled",
            "response_mode",
        ],
        ("watch", "list") => &["enabled", "limit", "cursor", "response_mode"],
        ("watch", "get") => &["id", "response_mode"],
        ("watch", "status") => &["id", "source", "response_mode"],
        ("watch", "update") => &[
            "id",
            "enabled",
            "every_seconds",
            "options",
            "limits",
            "metadata",
            "embed",
            "collection",
            "scope",
            "response_mode",
        ],
        ("watch", "pause" | "resume" | "delete") => &["id", "response_mode"],
        ("watch", "exec") => &["id", "source", "reason", "refresh", "wait", "response_mode"],
        ("watch", "history") => &["id", "source", "limit", "cursor", "status", "response_mode"],
        _ => return None,
    };
    Some(fields)
}
