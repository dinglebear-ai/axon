use super::*;

#[test]
fn retry_mode_accepts_wire_names() {
    assert!(matches!(
        parse_retry_mode("same_config").expect("same_config"),
        JobRetryMode::SameConfig
    ));
    assert!(matches!(
        parse_retry_mode("with_overrides").expect("with_overrides"),
        JobRetryMode::WithOverrides
    ));
}

#[test]
fn job_filters_parse_wire_enums() {
    let cfg = Config {
        positional: vec![
            "list".to_string(),
            "--status".to_string(),
            "completed_degraded".to_string(),
            "--kind".to_string(),
            "provider_probe".to_string(),
        ],
        ..Config::default()
    };
    assert!(matches!(
        parse_opt_flag::<LifecycleStatus>(&cfg, "--status").expect("status"),
        Some(LifecycleStatus::CompletedDegraded)
    ));
    assert!(matches!(
        parse_opt_flag::<JobKind>(&cfg, "--kind").expect("kind"),
        Some(JobKind::ProviderProbe)
    ));
}

#[test]
fn completed_job_counts_render_as_details() {
    let counts = axon_api::source::StageCounts {
        items_total: Some(344),
        items_done: 344,
        documents_total: Some(344),
        documents_done: 344,
        chunks_total: Some(7_608),
        chunks_done: 7_608,
        bytes_total: None,
        bytes_done: 0,
    };

    assert_eq!(
        format_job_counts(&counts).as_deref(),
        Some("344/344 docs · 100% · 7608 chunks")
    );
}

#[test]
fn completed_map_counts_render_as_item_details() {
    let counts = axon_api::source::StageCounts {
        items_total: Some(2),
        items_done: 2,
        documents_total: Some(0),
        documents_done: 0,
        chunks_total: Some(0),
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    };

    assert_eq!(
        format_job_counts(&counts).as_deref(),
        Some("2/2 items · 100%")
    );
}
