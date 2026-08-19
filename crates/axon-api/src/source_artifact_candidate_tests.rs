use super::*;

const NEUTRAL_FIXTURE: &str =
    include_str!("../tests/fixtures/artifact-registry/artifact-candidate-v1.json");
const SCHEMA_FIXTURE: &str =
    include_str!("../tests/fixtures/schema/artifact_candidate.v1.neutral.json");
const INTERCHANGE_FIXTURE: &str =
    include_str!("../tests/fixtures/artifact-registry/artifact-interchange-v1.json");

fn fixture_value() -> serde_json::Value {
    serde_json::from_str(NEUTRAL_FIXTURE).expect("neutral ArtifactCandidate fixture is valid JSON")
}

fn candidate() -> ArtifactCandidate {
    serde_json::from_value(fixture_value()).expect("neutral ArtifactCandidate fixture deserializes")
}

#[test]
fn neutral_candidate_fixture_round_trips_with_exact_shared_field_names() {
    let expected = fixture_value();
    let value = candidate();
    value
        .validate_shared_contract()
        .expect("neutral fixture satisfies shared bounds");

    assert_eq!(value.schema_version, ARTIFACT_CANDIDATE_SCHEMA_VERSION);
    assert_eq!(
        ARTIFACT_CANDIDATE_SCHEMA_VERSION,
        "dinglebear.artifact-candidate/v1"
    );

    let encoded = serde_json::to_value(&value).expect("candidate serializes");
    assert_eq!(encoded, expected);
    assert_eq!(
        serde_json::to_string(&encoded).expect("candidate canonical JSON serializes"),
        NEUTRAL_FIXTURE.trim_end(),
        "Rust serialization must match the frozen Depot 25de725 canonical JSON fixture exactly"
    );

    let decoded: ArtifactCandidate =
        serde_json::from_value(encoded.clone()).expect("serialized candidate round-trips");
    assert_eq!(decoded, value);

    let keys = encoded
        .as_object()
        .expect("candidate is a JSON object")
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let expected_keys = [
        "schemaVersion",
        "id",
        "canonicalSourceUri",
        "sourceProvider",
        "observedAt",
        "repository",
        "ref",
        "sourcePath",
        "kindHints",
        "observedFiles",
        "manifestMetadata",
        "contentDigests",
        "discoveryEvidence",
        "popularitySignals",
        "licenseEvidence",
        "crawlGenerationId",
        "crawlJobId",
        "warnings",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(keys, expected_keys);
}

#[test]
fn optional_g0_evidence_defaults_and_canonicalizes_to_the_full_shared_shape() {
    let minimal = serde_json::json!({
        "schemaVersion": ARTIFACT_CANDIDATE_SCHEMA_VERSION,
        "id": "cand_minimal_v1",
        "canonicalSourceUri": "https://github.com/dinglebear-ai/example",
        "sourceProvider": "axon",
        "observedAt": "2026-08-19T13:00:00Z"
    });
    let value: ArtifactCandidate =
        serde_json::from_value(minimal).expect("optional G0 evidence defaults");
    value
        .validate_shared_contract()
        .expect("minimal canonical candidate remains valid");

    let encoded = serde_json::to_value(value).expect("minimal candidate canonicalizes");
    let object = encoded.as_object().expect("candidate is an object");
    assert_eq!(object.len(), 18);
    for key in [
        "repository",
        "ref",
        "sourcePath",
        "crawlGenerationId",
        "crawlJobId",
    ] {
        assert!(object[key].is_null(), "{key} must canonicalize to null");
    }
    for key in ["kindHints", "observedFiles", "contentDigests", "warnings"] {
        assert_eq!(
            object[key],
            serde_json::json!([]),
            "{key} must default empty"
        );
    }
    for key in [
        "manifestMetadata",
        "discoveryEvidence",
        "popularitySignals",
        "licenseEvidence",
    ] {
        assert_eq!(
            object[key],
            serde_json::json!({}),
            "{key} must default empty"
        );
    }
}

#[test]
fn copied_g0_fixtures_are_pinned_to_the_frozen_shared_contract() {
    assert_eq!(
        SCHEMA_FIXTURE, NEUTRAL_FIXTURE,
        "the schema-validation copy must remain byte-identical to the frozen Depot candidate fixture"
    );

    let interchange: serde_json::Value = serde_json::from_str(INTERCHANGE_FIXTURE)
        .expect("frozen ArtifactInterchange fixture is valid JSON");
    assert_eq!(
        interchange["schemaVersion"],
        serde_json::json!("dinglebear.artifact-interchange/v1")
    );
}

#[test]
fn neutral_candidate_rejects_axons_old_competing_top_level_fields() {
    for forbidden in [
        "contractVersion",
        "candidateId",
        "jobId",
        "sourceId",
        "generation",
        "sourceItemKey",
        "dedupe",
        "provenance",
        "license",
        "enrichmentEvidence",
        "observedMetadata",
        "publication",
        "revision",
        "artifact",
    ] {
        let mut encoded = fixture_value();
        encoded
            .as_object_mut()
            .expect("candidate is an object")
            .insert(forbidden.to_string(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<ArtifactCandidate>(encoded).is_err(),
            "unexpected shared candidate field accepted: {forbidden}"
        );
    }
}

#[test]
fn shared_candidate_bounds_match_w20_g0_contract() {
    let mut value = candidate();
    value.kind_hints = vec!["skill".to_string(); ARTIFACT_CANDIDATE_MAX_KIND_HINTS + 1];
    assert!(value.validate_shared_contract().is_err());

    let mut value = candidate();
    value.repository = Some("r".repeat(513));
    assert!(value.validate_shared_contract().is_err());

    let mut value = candidate();
    value.warnings = vec!["warning".to_string(); ARTIFACT_CANDIDATE_MAX_WARNINGS + 1];
    assert!(value.validate_shared_contract().is_err());

    let mut value = candidate();
    value.content_digests = vec!["sha256:not-a-digest".to_string()];
    assert!(value.validate_shared_contract().is_err());

    let mut value = candidate();
    value
        .manifest_metadata
        .insert("apiToken".to_string(), serde_json::json!("do-not-store"));
    assert!(value.validate_shared_contract().is_err());
}

#[test]
fn unknown_and_index_only_license_evidence_fail_closed_for_public_bytes() {
    for state in ["unknown", "restricted", "metadata_only", "cache_for_index"] {
        let mut value = candidate();
        value
            .license_evidence
            .insert("redistribution".to_string(), serde_json::json!(state));
        assert!(!value.permits_public_byte_mirroring(), "state={state}");
    }

    for state in ["redistributable", "forkable"] {
        let mut value = candidate();
        value
            .license_evidence
            .insert("redistribution".to_string(), serde_json::json!(state));
        assert!(value.permits_public_byte_mirroring(), "state={state}");
    }
}

#[test]
fn candidate_payload_remains_byte_free_and_authority_free() {
    let encoded = serde_json::to_value(candidate()).expect("candidate serializes");
    let object = encoded.as_object().expect("candidate is an object");
    for forbidden in [
        "bytes",
        "content",
        "bundle",
        "archive",
        "rawBytes",
        "publication",
        "redistribution",
        "owner",
        "revision",
        "revisionAuthority",
        "authoritativeLicense",
        "artifact",
    ] {
        assert!(
            !object.contains_key(forbidden),
            "unexpected field: {forbidden}"
        );
    }
}
