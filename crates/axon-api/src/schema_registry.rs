//! Shared schema-registry helpers used by schema-contract generation and
//! runtime tool-schema publication.
//!
//! **History (2026-07-12 alignment audit, #298 finding "axon-api
//! schema_registry.rs is a disconnected placeholder"):** this module used to
//! also define `DtoSchemaSpec`/`dto_schema_registry()`/`enum_schema_registry()`
//! — a hand-maintained registry of invented DTO/enum names (`SourceRecord`,
//! `LedgerEntry`, `ResetPlan`, ...) that never matched any real generated
//! `$defs` entry and had zero real callers outside a self-referential test
//! (the registry asserting it contained its own hardcoded family strings).
//! Those were deleted as dead/fictional. The **real** required/deferred API
//! DTO name registry — the one that actually gates
//! `docs/reference/api/schemas.json` generation — is
//! `xtask/src/schemas/api_defs.rs`'s `PHASE_1_REQUIRED_API_DEFS` /
//! `PHASE_1_DEFERRED_API_DEFS`, which is xtask-local (not exposed from
//! `axon-api`) because it derives schemas straight from the real
//! `axon_api::source::*` types via `schemars::JsonSchema`. See
//! `docs/pipeline-unification/schemas/api-dto-schema.md`'s "DTO Registry
//! Source" section for the up-to-date description.
//!
//! [`removed_dto_names`] below is the one function from the old module that
//! *is* real and load-bearing: `xtask/src/schemas/registry.rs`'s
//! `check_removed_api_dto_shapes` (called from `check_removed_surface_drift`,
//! exercised by `xtask/src/schemas/tests.rs`) asserts none of these names
//! reappear as `$defs` entries in the generated API schema.

pub fn removed_dto_names() -> &'static [&'static str] {
    &[
        "EmbedRequest",
        "IngestRequest",
        "CrawlRequest",
        "ScrapeRequest",
        "CodeSearchRequest",
    ]
}

pub fn prune_public_job_kind_schemas(value: &mut serde_json::Value) {
    let public_values = public_job_kind_values();
    let all_values = all_job_kind_values();
    prune_public_job_kind_schemas_inner(value, &public_values, &all_values);
}

fn prune_public_job_kind_schemas_inner(
    value: &mut serde_json::Value,
    public_values: &[String],
    all_values: &[String],
) {
    if is_job_kind_schema(value, all_values) {
        *value = serde_json::json!({
            "type": "string",
            "enum": public_values,
        });
        return;
    }

    match value {
        serde_json::Value::Object(map) => {
            for value in map.values_mut() {
                prune_public_job_kind_schemas_inner(value, public_values, all_values);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                prune_public_job_kind_schemas_inner(value, public_values, all_values);
            }
        }
        _ => {}
    }
}

fn is_job_kind_schema(value: &serde_json::Value, all_values: &[String]) -> bool {
    let mut values = Vec::new();
    collect_schema_string_values(value, &mut values);
    if values.len() < 4 {
        return false;
    }
    values.iter().all(|value| all_values.contains(value))
        && values.iter().any(|value| value == "source")
        && values.iter().any(|value| value == "provider_probe")
}

fn collect_schema_string_values(value: &serde_json::Value, out: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(values) = object.get("enum").and_then(serde_json::Value::as_array) {
        out.extend(
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string)),
        );
    }
    if let Some(value) = object.get("const").and_then(serde_json::Value::as_str) {
        out.push(value.to_string());
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(values) = object.get(key).and_then(serde_json::Value::as_array) {
            for value in values {
                collect_schema_string_values(value, out);
            }
        }
    }
}

fn all_job_kind_values() -> Vec<String> {
    crate::source::JobKind::all()
        .iter()
        .copied()
        .map(job_kind_wire_value)
        .collect()
}

fn public_job_kind_values() -> Vec<String> {
    crate::source::JobKind::all()
        .iter()
        .copied()
        .filter(|kind| kind.is_public_source_surface())
        .map(job_kind_wire_value)
        .collect()
}

fn job_kind_wire_value(kind: crate::source::JobKind) -> String {
    serde_json::to_value(kind)
        .expect("JobKind serializes")
        .as_str()
        .expect("JobKind serializes to string")
        .to_string()
}
