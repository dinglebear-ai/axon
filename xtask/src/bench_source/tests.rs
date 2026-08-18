use super::*;
use serde_json::json;

#[test]
fn summarizes_phase_wall_time_terminal_counts_and_warning_codes() {
    let page = json!({
        "events": [
            {
                "sequence": 1,
                "phase": "discovering",
                "status": "running",
                "timestamp": "2026-08-11T22:45:03Z",
                "details": {"source_progress_event": {"counts": {"items_done": 0, "documents_done": 0, "chunks_done": 0}}}
            },
            {
                "sequence": 2,
                "phase": "discovering",
                "status": "completed",
                "timestamp": "2026-08-11T22:45:05Z",
                "details": {"source_progress_event": {"counts": {"items_done": 370, "items_total": 370, "documents_done": 0, "chunks_done": 0}}}
            },
            {
                "sequence": 3,
                "phase": "embedding",
                "status": "running",
                "timestamp": "2026-08-11T22:45:10Z",
                "details": {"source_progress_event": {"counts": {"items_done": 370, "documents_done": 64, "chunks_done": 512}}}
            },
            {
                "sequence": 4,
                "phase": "embedding",
                "status": "completed",
                "timestamp": "2026-08-11T22:45:16Z",
                "details": {"source_progress_event": {"counts": {"items_done": 370, "documents_done": 370, "chunks_done": 7904}}}
            },
            {
                "sequence": 5,
                "phase": "publishing",
                "status": "completed_degraded",
                "timestamp": "2026-08-11T22:45:17Z",
                "details": {"source_progress_event": {
                    "counts": {"items_done": 370, "documents_done": 370, "chunks_done": 7904},
                    "warning": {"code": "source.vectorize.redaction_skipped_chunks"}
                }}
            }
        ]
    });

    let summary = summarize_events(&page).expect("valid event summary");
    assert_eq!(summary.phase_seconds["discovering"], 2.0);
    assert_eq!(summary.phase_seconds["embedding"], 6.0);
    assert_eq!(summary.items, 370);
    assert_eq!(summary.documents, 370);
    assert_eq!(summary.chunks, 7904);
    assert_eq!(
        summary.warning_codes["source.vectorize.redaction_skipped_chunks"],
        1
    );
}

#[test]
fn repeated_phase_timings_exclude_time_spent_in_other_phases() {
    let page = json!({"events": [
        {"sequence": 1, "phase": "embedding", "status": "running", "timestamp": "2026-08-11T22:00:00Z", "details": {"source_progress_event": {"counts": {}}}},
        {"sequence": 2, "phase": "embedding", "status": "completed", "timestamp": "2026-08-11T22:00:02Z", "details": {"source_progress_event": {"counts": {}}}},
        {"sequence": 3, "phase": "upserting", "status": "running", "timestamp": "2026-08-11T22:00:02Z", "details": {"source_progress_event": {"counts": {}}}},
        {"sequence": 4, "phase": "upserting", "status": "completed", "timestamp": "2026-08-11T22:00:12Z", "details": {"source_progress_event": {"counts": {}}}},
        {"sequence": 5, "phase": "embedding", "status": "running", "timestamp": "2026-08-11T22:00:12Z", "details": {"source_progress_event": {"counts": {}}}},
        {"sequence": 6, "phase": "embedding", "status": "completed", "timestamp": "2026-08-11T22:00:15Z", "details": {"source_progress_event": {"counts": {}}}}
    ]});

    let summary = summarize_events(&page).unwrap();
    assert_eq!(summary.phase_seconds["embedding"], 5.0);
    assert_eq!(summary.phase_seconds["upserting"], 10.0);
}

#[test]
fn distribution_is_stable_for_even_and_odd_sample_counts() {
    assert_eq!(
        distribution(&[9.0, 1.0, 5.0]).unwrap(),
        Distribution {
            min: 1.0,
            median: 5.0,
            max: 9.0
        }
    );
    assert_eq!(distribution(&[8.0, 2.0]).unwrap().median, 5.0);
    assert!(distribution(&[]).is_err());
}

#[test]
fn percent_change_reports_improvement_and_handles_zero_baseline() {
    assert_eq!(percent_change(100.0, 75.0), Some(-25.0));
    assert_eq!(percent_change(100.0, 125.0), Some(25.0));
    assert_eq!(percent_change(0.0, 5.0), None);
}

#[test]
fn finds_job_id_across_supported_command_projections() {
    assert_eq!(
        find_job_id(&json!({"job_id": "job-direct"})),
        Some("job-direct")
    );
    assert_eq!(
        find_job_id(&json!({"result": {"summary": {"job_id": "job-nested"}}})),
        Some("job-nested")
    );
    assert_eq!(find_job_id(&json!({"status": "completed"})), None);
}

#[test]
fn warm_scenario_is_explicitly_cached_and_conditional() {
    assert_eq!(source_cache_args(false), vec!["--cache", "false"]);
    assert_eq!(
        source_cache_args(true),
        vec!["--cache", "true", "--etag-conditional"]
    );
}

#[test]
fn failed_run_preserves_cleanup_failure_context() {
    let error = resolve_run_with_cleanup::<()>(
        Err(anyhow!("run exploded")),
        Err(anyhow!("cleanup exploded")),
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("run exploded"));
    assert!(message.contains("cleanup exploded"));
}

#[test]
fn successful_run_still_fails_when_cleanup_fails() {
    let error = resolve_run_with_cleanup(Ok(42_u64), Err(anyhow!("cleanup exploded"))).unwrap_err();
    assert!(format!("{error:#}").contains("cleanup exploded"));
}

#[test]
fn baseline_must_contain_every_candidate_scenario() {
    let summary = |scenario: &str| ScenarioSummary {
        scenario: scenario.to_string(),
        runs: 1,
        wall_seconds: Distribution {
            min: 1.0,
            median: 1.0,
            max: 1.0,
        },
        items: Distribution {
            min: 1.0,
            median: 1.0,
            max: 1.0,
        },
        documents: Distribution {
            min: 1.0,
            median: 1.0,
            max: 1.0,
        },
        chunks: Distribution {
            min: 1.0,
            median: 1.0,
            max: 1.0,
        },
        points: None,
        phase_median_seconds: BTreeMap::new(),
        unattributed_seconds: Distribution {
            min: 0.0,
            median: 0.0,
            max: 0.0,
        },
        degraded_runs: 0,
        warning_codes: BTreeMap::new(),
        retry_events: 0,
    };
    let baseline = vec![summary("cold")];
    let candidate = vec![summary("cold"), summary("warm")];

    let error = validate_baseline_scenarios(&baseline, &candidate).unwrap_err();
    assert!(error.to_string().contains("warm"));
}
