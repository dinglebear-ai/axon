use super::*;
use uuid::Uuid;

fn timestamp() -> Timestamp {
    Timestamp("2026-08-19T13:45:00Z".to_string())
}

fn candidate() -> ArtifactCandidate {
    let observed_at = timestamp();
    ArtifactCandidate {
        contract_version: ARTIFACT_CANDIDATE_CONTRACT_VERSION.to_string(),
        candidate_id: ArtifactCandidateId::from("sha256:candidate"),
        job_id: JobId::from(Uuid::nil()),
        source_id: SourceId::from("src_skills_sh"),
        generation: SourceGenerationId::from("7"),
        source_item_key: SourceItemKey::from("vercel-labs/skills/find-skills"),
        canonical_observed_uri: "https://skills.sh/vercel-labs/skills/find-skills".to_string(),
        canonical_source_uri: "https://github.com/vercel-labs/skills/tree/main/skills/find-skills"
            .to_string(),
        kind_hints: vec![ArtifactCandidateKindHint::Skill],
        dedupe: ArtifactCandidateDedupe {
            identity_key: "sha256:identity".to_string(),
            content_key: Some("sha256:content-key".to_string()),
            content_hash: Some("sha256:content".to_string()),
        },
        provenance: ArtifactProvenanceEvidence {
            provider: "skills.sh".to_string(),
            source_kind: SourceKind::Registry,
            observed_uri: "https://skills.sh/vercel-labs/skills/find-skills".to_string(),
            canonical_source_uri:
                "https://github.com/vercel-labs/skills/tree/main/skills/find-skills".to_string(),
            repository_url: Some("https://github.com/vercel-labs/skills".to_string()),
            source_ref: Some("main".to_string()),
            source_path: Some("skills/find-skills".to_string()),
            source_digest: Some("sha256:content".to_string()),
            adapter: AdapterRef {
                name: "skills_sh".to_string(),
                version: "1".to_string(),
            },
            observed_at: observed_at.clone(),
            metadata: MetadataMap::new(),
        },
        license: ArtifactLicenseEvidence {
            declared_expression: None,
            detected_expression: None,
            detection_confidence: None,
            redistribution: ArtifactRedistributionClass::Unknown,
            modification: ArtifactModificationClass::Unknown,
            evidence: Vec::new(),
            notice_refs: Vec::new(),
            attribution_refs: Vec::new(),
            observed_at: observed_at.clone(),
        },
        discovery_evidence: Vec::new(),
        enrichment_evidence: Vec::new(),
        observed_metadata: MetadataMap::new(),
        observed_at,
        warnings: Vec::new(),
    }
}

#[test]
fn artifact_candidate_round_trips_with_generation_and_job_correlation() {
    let value = candidate();
    let encoded = serde_json::to_value(&value).expect("candidate serializes");
    assert_eq!(
        encoded["contract_version"],
        ARTIFACT_CANDIDATE_CONTRACT_VERSION
    );
    assert_eq!(encoded["source_id"], "src_skills_sh");
    assert_eq!(encoded["generation"], "7");
    assert_eq!(encoded["source_item_key"], "vercel-labs/skills/find-skills");
    assert_eq!(encoded["job_id"], Uuid::nil().to_string());

    let decoded: ArtifactCandidate =
        serde_json::from_value(encoded).expect("candidate deserializes");
    assert_eq!(decoded, value);
}

#[test]
fn artifact_candidate_contract_rejects_unknown_fields() {
    let mut encoded = serde_json::to_value(candidate()).expect("candidate serializes");
    encoded
        .as_object_mut()
        .expect("candidate is an object")
        .insert(
            "authoritative_revision".to_string(),
            serde_json::json!(true),
        );

    assert!(serde_json::from_value::<ArtifactCandidate>(encoded).is_err());
}

#[test]
fn unknown_and_index_only_license_states_fail_closed_for_public_bytes() {
    for state in [
        ArtifactRedistributionClass::Unknown,
        ArtifactRedistributionClass::Restricted,
        ArtifactRedistributionClass::MetadataOnly,
        ArtifactRedistributionClass::CacheForIndex,
    ] {
        assert!(!state.permits_public_byte_mirroring(), "state={state:?}");
    }

    assert!(ArtifactRedistributionClass::Redistributable.permits_public_byte_mirroring());
    assert!(ArtifactRedistributionClass::Forkable.permits_public_byte_mirroring());
}

#[test]
fn serialized_candidate_has_no_byte_payload_field() {
    let encoded = serde_json::to_value(candidate()).expect("candidate serializes");
    let object = encoded.as_object().expect("candidate is an object");

    for forbidden in ["bytes", "content", "bundle", "archive", "raw_bytes"] {
        assert!(
            !object.contains_key(forbidden),
            "unexpected field: {forbidden}"
        );
    }
}
