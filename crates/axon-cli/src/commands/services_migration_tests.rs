#[test]
fn migrated_cli_commands_do_not_import_raw_business_logic_layers() {
    let checks = [
        (
            "evaluate.rs",
            include_str!("evaluate.rs"),
            &["vector::ops::run_evaluate_native"][..],
        ),
        (
            "suggest.rs",
            include_str!("suggest.rs"),
            &["vector::ops::run_suggest_native"][..],
        ),
        (
            "debug.rs",
            include_str!("debug.rs"),
            &["build_doctor_report", "http_client()"][..],
        ),
    ];

    for (file, source, forbidden_fragments) in checks {
        // Scan the whole file — forbidden imports should not appear anywhere,
        // including test modules (tests should use service layer too).
        let production_source = source;
        for forbidden in forbidden_fragments {
            assert!(
                !production_source.contains(forbidden),
                "{file} still contains forbidden direct-layer reference: {forbidden}"
            );
        }
    }
}

#[test]
fn migrated_cli_commands_do_not_import_raw_business_logic_layers_v2() {
    let checks = [
        (
            "extract.rs",
            include_str!("extract.rs"),
            &["jobs::extract::{"][..],
        ),
        (
            "ingest_common.rs",
            include_str!("ingest_common.rs"),
            &["jobs::ingest::{"][..],
        ),
        (
            "watch.rs",
            include_str!("watch.rs"),
            &["jobs::watch::{"][..],
        ),
        (
            "domains.rs",
            include_str!("domains.rs"),
            &["vector::ops::qdrant::domains_payload"][..],
        ),
        (
            "sources.rs",
            include_str!("sources.rs"),
            &["vector::ops::qdrant::sources_payload"][..],
        ),
        (
            "stats.rs",
            include_str!("stats.rs"),
            &["vector::ops::stats::stats_payload"][..],
        ),
    ];

    for (file, source, forbidden_fragments) in checks {
        for forbidden in forbidden_fragments {
            assert!(
                !source.contains(forbidden),
                "{file} still contains forbidden direct-layer reference: {forbidden}"
            );
        }
    }
}
