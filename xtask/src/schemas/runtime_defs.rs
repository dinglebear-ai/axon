use anyhow::Result;
use serde_json::{Value, json};
use std::path::Path;

use super::super::SchemaFamily;
use super::super::artifact::SchemaArtifact;
use super::super::rel;
use super::super::schema_json::{json_string, schema_defs};
use super::super::source_input::{SourceInput, source_inputs};
use super::family_specs;
use super::{enum_defs, markdown, schema_bundle, schema_id};

pub fn events_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Events);
    let inputs = source_inputs(root, spec.source_paths)?;
    let defs = schema_defs(
        &[
            schema_def::<axon_api::source::SourceProgressEvent>("SourceProgressEvent"),
            schema_def::<axon_api::source::JobEvent>("JobEvent"),
            schema_def::<axon_api::source::JobEventPage>("JobEventPage"),
            schema_def::<axon_api::source::JobHeartbeat>("JobHeartbeat"),
            schema_def::<axon_api::source::ProgressCurrent>("ProgressCurrent"),
            schema_def::<axon_api::source::ProgressTiming>("ProgressTiming"),
            schema_def::<axon_api::source::StageCounts>("StageCounts"),
        ],
        Some(enum_defs("axon-api")),
    );
    let schema = schema_bundle(
        schema_id(SchemaFamily::Events),
        spec.title,
        "cargo xtask schemas events",
        spec.owner_crates,
        &inputs,
        defs,
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(spec.markdown_path), events_markdown(&inputs)),
    ])
}

pub fn database_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Database);
    let inputs = source_inputs(root, spec.source_paths)?;
    let schema = schema_bundle(
        schema_id(SchemaFamily::Database),
        spec.title,
        "cargo xtask schemas database",
        spec.owner_crates,
        &inputs,
        database_defs(),
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(spec.markdown_path), database_markdown(&inputs)),
    ])
}

fn database_defs() -> Value {
    json!({
        "UnifiedJobsObservabilitySchema": {
            "type": "object",
            "description": "SQLite tables introduced by migration 0018_unified_jobs_observability.sql.",
            "required": ["tables"],
            "properties": {
                "tables": {
                    "type": "array",
                    "items": { "type": "string" },
                    "const": ["jobs", "job_attempts", "job_stages", "job_events", "job_heartbeats", "provider_reservations", "job_artifacts"]
                }
            },
            "additionalProperties": false,
            "x-axon": {
                "migration": "crates/axon-jobs/src/migrations/0018_unified_jobs_observability.sql",
                "primary_owner": "axon-jobs",
                "tables": {
                    "jobs": {
                        "primary_key": ["job_id"],
                        "foreign_keys": ["source_id", "watch_id", "parent_job_id", "root_job_id"],
                        "json_columns": ["counts_json", "current_json", "heartbeat_json", "last_error_json", "warnings_json", "request_json", "metadata_json", "auth_snapshot_json", "stage_plan_json", "requirements_json", "error_json"],
                        "indexes": [
                            "jobs_idempotency_key_idx",
                            "jobs_created_at_desc_idx",
                            "jobs_status_created_at_idx",
                            "jobs_kind_status_created_at_idx",
                            "jobs_status_updated_at_idx",
                            "jobs_source_id_idx",
                            "jobs_watch_id_idx",
                            "jobs_source_id_created_at_idx",
                            "jobs_watch_id_created_at_idx",
                            "idx_axon_jobs_status_kind_updated",
                            "idx_axon_jobs_source_status_updated",
                            "idx_axon_jobs_watch_status_updated",
                            "idx_axon_jobs_updated",
                            "idx_axon_jobs_source_updated",
                            "idx_axon_jobs_watch_updated",
                            "idx_axon_jobs_claim"
                        ],
                        "contract_columns": ["auth_snapshot_json", "config_snapshot_id", "stage_plan_json", "requirements_json", "result_schema", "error_json", "last_event_sequence"]
                    },
                    "job_attempts": {
                        "primary_key": ["attempt_id"],
                        "foreign_keys": ["job_id"],
                        "unique": [["job_id", "attempt"]],
                        "json_columns": ["error_json"],
                        "indexes": ["job_attempts_job_id_idx", "idx_axon_job_attempts_job_attempt"]
                    },
                    "job_stages": {
                        "primary_key": ["stage_id"],
                        "foreign_keys": ["job_id"],
                        "json_columns": ["provider_requirements_json", "counts_json", "error_json"],
                        "indexes": ["job_stages_job_id_idx", "idx_axon_job_stages_job_stage"]
                    },
                    "job_events": {
                        "primary_key": ["event_id"],
                        "foreign_keys": ["job_id", "stage_id"],
                        "unique": [["job_id", "sequence"], ["job_id", "dedupe_key"]],
                        "partial_unique_indexes": [
                            {
                                "name": "job_events_job_dedupe_key_idx",
                                "columns": ["job_id", "dedupe_key"],
                                "predicate": "dedupe_key IS NOT NULL"
                            }
                        ],
                        "json_columns": ["details_json"],
                        "indexes": ["job_events_job_dedupe_key_idx", "job_events_job_sequence_idx", "job_events_job_phase_idx", "job_events_job_severity_idx", "job_events_job_visibility_idx", "idx_axon_job_events_job_sequence", "idx_axon_job_events_job_severity_sequence"]
                    },
                    "job_heartbeats": {
                        "primary_key": ["job_id", "attempt"],
                        "foreign_keys": ["job_id"],
                        "json_columns": ["heartbeat_json"],
                        "indexes": ["job_heartbeats_job_id_idx", "job_heartbeats_heartbeat_at_idx"]
                    },
                    "provider_reservations": {
                        "primary_key": ["reservation_id"],
                        "foreign_keys": ["job_id", "stage_id"],
                        "json_columns": ["cooling_json"],
                        "indexes": ["provider_reservations_job_id_idx", "provider_reservations_stage_id_idx", "provider_reservations_provider_kind_idx"]
                    },
                    "job_artifacts": {
                        "primary_key": ["artifact_id"],
                        "foreign_keys": ["job_id"],
                        "indexes": ["job_artifacts_job_id_idx", "job_artifacts_job_kind_idx"]
                    }
                }
            }
        }
    })
}

fn events_markdown(inputs: &[SourceInput]) -> String {
    let mut out = markdown("events", inputs);
    out.push_str("\n## Event DTO Coverage\n\n| DTO |\n|---|\n");
    for dto in [
        "SourceProgressEvent",
        "JobEvent",
        "JobEventPage",
        "JobHeartbeat",
        "ProgressCurrent",
        "ProgressTiming",
        "StageCounts",
    ] {
        out.push_str(&format!("| `{dto}` |\n"));
    }
    out
}

fn database_markdown(inputs: &[SourceInput]) -> String {
    let mut out = markdown("database", inputs);
    out.push_str(
        "\n## Unified Job Tables\n\n| Table | Primary Key | Important Indexes |\n|---|---|---|\n",
    );
    for (table, primary_key, indexes) in [
        (
            "jobs",
            "job_id",
            "jobs_created_at_desc_idx, jobs_status_created_at_idx, jobs_kind_status_created_at_idx, jobs_source_id_created_at_idx, jobs_watch_id_created_at_idx",
        ),
        ("job_attempts", "attempt_id", "job_attempts_job_id_idx"),
        ("job_stages", "stage_id", "job_stages_job_id_idx"),
        (
            "job_events",
            "event_id",
            "job_events_job_dedupe_key_idx (partial unique WHERE dedupe_key IS NOT NULL), job_events_job_sequence_idx, job_events_job_visibility_idx",
        ),
        (
            "job_heartbeats",
            "job_id, attempt",
            "job_heartbeats_heartbeat_at_idx",
        ),
        (
            "provider_reservations",
            "reservation_id",
            "provider_reservations_job_id_idx, provider_reservations_stage_id_idx, provider_reservations_provider_kind_idx",
        ),
        ("job_artifacts", "artifact_id", "job_artifacts_job_kind_idx"),
    ] {
        out.push_str(&format!("| `{table}` | `{primary_key}` | `{indexes}` |\n"));
    }
    out
}

fn schema_def<T: schemars::JsonSchema>(name: &'static str) -> (&'static str, Value) {
    (name, schemars::schema_for!(T).into())
}
