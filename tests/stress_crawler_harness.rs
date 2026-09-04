#![cfg(unix)]

use std::path::Path;
use std::process::Command;

#[test]
fn stress_crawler_defaults_to_a_non_mutating_external_qdrant_plan() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output_dir = temp.path().join("stress-output");
    let output = Command::new("bash")
        .arg("scripts/stress-crawler.sh")
        .args([
            "--mode",
            "plan",
            "--url",
            "https://docs.example.test/",
            "--out-dir",
        ])
        .arg(&output_dir)
        .env("AXON_STRESS_MAX_PAGES", "500")
        .env("AXON_STRESS_CONCURRENT_JOBS", "8")
        .env("QDRANT_URL", "http://qdrant.example.test:6333")
        .output()
        .expect("run stress plan");

    assert!(
        output.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("plan JSON on stdout");
    assert_eq!(report["mode"], "plan");
    assert_eq!(report["mutates"], false);
    assert_eq!(report["desired_pages"], 500);
    assert_eq!(report["concurrent_jobs"], 8);
    assert_eq!(report["qdrant_runtime"], "external-existing-only");
    assert!(
        report["collection"]
            .as_str()
            .is_some_and(|value| value.starts_with("axon_stress_"))
    );
    assert!(
        !Path::new(report["state_dir"].as_str().expect("state_dir")).exists(),
        "plan mode must not create isolated state"
    );
    assert!(
        !output_dir.exists(),
        "plan mode must not create the report directory"
    );
}

#[test]
fn stress_crawler_heavy_mode_is_explicit_and_rejects_local_qdrant() {
    let unconfirmed = Command::new("bash")
        .arg("scripts/stress-crawler.sh")
        .args(["--mode", "heavy", "--url", "https://docs.example.test/"])
        .env_remove("AXON_STRESS_CONFIRM")
        .env("QDRANT_URL", "http://qdrant.example.test:6333")
        .output()
        .expect("run unconfirmed heavy plan");
    assert!(!unconfirmed.status.success());
    assert!(
        String::from_utf8_lossy(&unconfirmed.stderr).contains("AXON_STRESS_CONFIRM"),
        "missing confirmation must fail clearly"
    );

    let local_qdrant = Command::new("bash")
        .arg("scripts/stress-crawler.sh")
        .args(["--mode", "heavy", "--url", "https://docs.example.test/"])
        .env("AXON_STRESS_CONFIRM", "CRAWL_AND_DELETE_ISOLATED_STATE")
        .env("QDRANT_URL", "http://127.0.0.1:53333")
        .env("AXON_BIN", "/bin/true")
        .output()
        .expect("run local-Qdrant heavy plan");
    assert!(!local_qdrant.status.success());
    assert!(
        String::from_utf8_lossy(&local_qdrant.stderr).contains("external Qdrant"),
        "loopback Qdrant must be refused"
    );
}

#[test]
fn stress_crawler_contains_no_container_runtime_control() {
    let entry = std::fs::read_to_string("scripts/stress-crawler.sh").expect("read entry script");
    let runtime = std::fs::read_to_string("scripts/lib/stress-crawler-runtime.sh")
        .expect("read runtime helpers");
    let script = format!("{entry}\n{runtime}");
    for forbidden in [
        "docker ",
        "docker-compose",
        "incus ",
        "qdrant-up",
        "compose up",
    ] {
        assert!(
            !script.contains(forbidden),
            "stress harness must not control container/Qdrant runtime: {forbidden}"
        );
    }
    assert!(
        script.contains("epoch_ns / 1000000"),
        "stress timing must convert nanoseconds to milliseconds explicitly"
    );
    assert!(
        !script.contains("date +%s%3N"),
        "GNU date fractional-width behavior is not portable on this host"
    );
    assert!(
        script.contains("bounded-map-plus-explicit-rust-book-seeds"),
        "the documented Rust Book smoke fallback must be explicit in its report"
    );
    assert!(
        script.contains("qdrant_collection_absent")
            && script.contains("state_dir_absent")
            && script.contains("outcome: \"failed\""),
        "failed stress runs must retain a structured report with cleanup evidence"
    );
    assert!(
        script.contains("durable_provider_reservations_applicable: true")
            && script.contains("provider_reservations_terminal")
            && !script.contains("durable_provider_reservations_applicable: false"),
        "source stress must validate durable scheduler reservation evidence"
    );
    assert!(
        script.contains("secret_redaction_skips")
            && script.contains("point_delta_after_policy")
            && !script.contains("qdrant_covers_primary_chunks"),
        "stress verification must distinguish pre-redaction chunks from published points"
    );
}
