use super::*;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a workspace parent")
        .to_path_buf()
}

#[test]
fn parses_real_migrations_and_finds_known_tables() {
    let schema = parse_all(&workspace_root()).expect("parse real migration directories");
    for table in [
        "jobs",
        "job_attempts",
        "job_stages",
        "job_events",
        "job_heartbeats",
        "provider_reservations",
        "job_artifacts",
        "sources",
        "source_generations",
        "source_manifests",
        "source_items",
        "document_status",
        "cleanup_debt",
        "leases",
        "graph_nodes",
        "graph_edges",
        "memory_records",
        "memory_links",
    ] {
        assert!(
            schema.tables.contains_key(table),
            "expected table {table:?} to be parsed"
        );
    }
    // Tables renamed away by ALTER TABLE ... RENAME TO must not survive as
    // stray `_v2` entries in the final snapshot.
    for stray in ["jobs_v2"] {
        assert!(
            !schema.tables.contains_key(stray),
            "renamed-away table {stray:?} should not remain in the final snapshot"
        );
    }
}

#[test]
fn jobs_table_carries_alter_table_add_column_fields_and_foreign_keys() {
    let schema = parse_all(&workspace_root()).expect("parse real migration directories");
    let jobs = schema.tables.get("jobs").expect("jobs table parsed");
    let column_names: std::collections::BTreeSet<&str> =
        jobs.columns.iter().map(|c| c.name.as_str()).collect();
    // Present from the base 0018 CREATE TABLE.
    assert!(column_names.contains("job_id"));
    assert!(column_names.contains("kind"));
    // Added later via ALTER TABLE ... ADD COLUMN (0019 and 0022).
    assert!(column_names.contains("auth_snapshot_json"));
    assert!(column_names.contains("last_event_sequence"));
    assert!(column_names.contains("cooldown_until"));

    let fk_targets: Vec<&str> = jobs
        .foreign_keys
        .iter()
        .map(|fk| fk.ref_table.as_str())
        .collect();
    assert!(fk_targets.contains(&"sources"));
    assert!(fk_targets.contains(&"axon_source_watches"));
    assert!(!fk_targets.contains(&"axon_watch_defs"));
}

#[test]
fn source_manifests_composite_foreign_key_is_captured() {
    let schema = parse_all(&workspace_root()).expect("parse real migration directories");
    let table = schema
        .tables
        .get("source_manifests")
        .expect("source_manifests table parsed");
    let fk = table
        .foreign_keys
        .iter()
        .find(|fk| fk.ref_table == "source_generations")
        .expect("composite FK to source_generations present");
    assert_eq!(fk.columns, vec!["source_id", "generation"]);
    assert_eq!(fk.ref_columns, vec!["source_id", "generation"]);
}

#[test]
fn build_artifact_fields_is_idempotent_and_free_of_legacy_names() {
    let root = workspace_root();
    let (first, _) = build_artifact_fields(&root).expect("first build");
    let (second, _) = build_artifact_fields(&root).expect("second build");
    assert_eq!(
        first, second,
        "database schema generation must be deterministic"
    );

    let serialized = first.to_string();
    for legacy in ["memory_decay", "watch_events", "job_config_snapshots"] {
        assert!(
            !serialized.contains(legacy),
            "legacy table name {legacy:?} must not appear in generated database schema fields"
        );
    }
    assert!(first["tables"].as_array().unwrap().len() > 10);
    assert!(first["migrations"].as_array().unwrap().len() > 10);
    assert!(!first["divergences"].as_array().unwrap().is_empty());
}
