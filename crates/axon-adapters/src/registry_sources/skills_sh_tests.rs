use super::*;
use axon_api::source::*;
use std::path::PathBuf;
use uuid::Uuid;

use crate::adapter::SourceAdapter;
use crate::registry_sources::RegistrySourceAdapter;

fn plan_with_options(values: MetadataMap, max_items: Option<u64>) -> SourcePlan {
    let canonical_uri = "catalog://skills.sh/leaderboard".to_string();
    let source_id = SourceId::from("src_skills_sh");
    SourcePlan {
        job_id: JobId::new(Uuid::from_u128(21)),
        request: SourceRequest::new("skills.sh"),
        route: RoutePlan {
            source: ResolvedSource {
                source: "skills.sh".to_string(),
                canonical_uri,
                source_id,
                source_kind: SourceKind::Registry,
                adapter: AdapterRef {
                    name: "registry".to_string(),
                    version: "1".to_string(),
                },
                default_scope: SourceScope::Api,
                available_scopes: vec![SourceScope::Api],
                authority: AuthorityLevel::Community,
                confidence: 1.0,
                reason: "test skills.sh catalog".to_string(),
                graph: Vec::new(),
                warnings: Vec::new(),
                metadata: MetadataMap::new(),
            },
            adapter: AdapterRef {
                name: "registry".to_string(),
                version: "1".to_string(),
            },
            scope: SourceScope::Api,
            provider_requirements: Vec::new(),
            credential_requirements: Vec::new(),
            execution_affinity: ExecutionAffinity::Worker,
            safety_class: SafetyClass::AuthenticatedNetwork,
            option_schema_id: "adapter:skills_sh:options:v1".to_string(),
            validated_options: AdapterOptions { values },
            chunking_hints: Vec::new(),
            parser_hints: Vec::new(),
            graph_fact_kinds: vec!["source".to_string(), "artifact".to_string()],
            watch_supported: true,
            refresh_supported: true,
        },
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits {
                max_items,
                ..SourceLimits::default()
            },
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits {
                max_items,
                ..SourceLimits::default()
            },
        },
        config_snapshot_id: ConfigSnapshotId::from("cfg_skills_sh"),
        provider_reservations: Vec::new(),
    }
}

fn search_plan_with_options(values: MetadataMap, max_items: Option<u64>) -> SourcePlan {
    let mut plan = plan_with_options(values, max_items);
    plan.route.source.canonical_uri = "catalog://skills.sh/search".to_string();
    plan
}

#[test]
fn default_options_are_small_bounded_and_incremental_friendly() {
    let plan = plan_with_options(MetadataMap::new(), None);
    let options = options(&plan).expect("default skills.sh options");
    assert_eq!(options.mode, SkillsShMode::Leaderboard);
    assert_eq!(options.view, SkillsShView::AllTime);
    assert_eq!(options.start_page, 0);
    assert_eq!(options.per_page, 100);
    assert_eq!(options.max_pages, 1);
    assert_eq!(options.total_limit, 100);
    assert_eq!(options.audit_limit, 0);
}

#[test]
fn source_max_items_caps_catalog_rows_and_page_size() {
    let plan = plan_with_options(MetadataMap::new(), Some(25));
    let options = options(&plan).expect("max_items applies");
    assert_eq!(options.total_limit, 25);
    assert_eq!(options.per_page, 25);
}

#[test]
fn explicit_limits_are_clamped_to_hard_provider_bounds() {
    let mut values = MetadataMap::new();
    values.insert("per_page".to_string(), serde_json::json!(9_999));
    values.insert("max_pages".to_string(), serde_json::json!(999));
    values.insert("audit_limit".to_string(), serde_json::json!(999));
    let plan = plan_with_options(values, Some(50_000));
    let options = options(&plan).expect("provider bounds apply");
    assert_eq!(options.total_limit, 1_000);
    assert_eq!(options.per_page, 500);
    assert_eq!(options.max_pages, 10);
    assert_eq!(options.audit_limit, 25);
}

fn dump_with_skill(installs: u64) -> SkillsShDump {
    SkillsShDump {
        provider: "skills.sh".to_string(),
        observed_at: Timestamp("2026-08-19T13:00:00Z".to_string()),
        skills: vec![SkillsShSkill {
            id: "vercel-labs/skills/find-skills".to_string(),
            slug: "find-skills".to_string(),
            name: "find-skills".to_string(),
            source: "vercel-labs/skills".to_string(),
            installs,
            source_type: "github".to_string(),
            install_url: Some("https://github.com/vercel-labs/skills".to_string()),
            url: Some("https://skills.sh/vercel-labs/skills/find-skills".to_string()),
            is_duplicate: Some(false),
            audits: Vec::new(),
            audit_status: None,
            audit_warnings: Vec::new(),
        }],
    }
}

fn write_dump(dump: &SkillsShDump) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("skills-sh.json");
    std::fs::write(&path, serde_json::to_vec(dump).expect("serialize dump")).expect("write dump");
    (temp, path)
}

fn added_diff(plan: &SourcePlan, manifest: &SourceManifest) -> SourceManifestDiff {
    SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::for_job_stage(plan.job_id, "skills-sh-diff-test", 0),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: Timestamp("2026-08-19T13:00:00Z".to_string()),
            completed_at: Some(Timestamp("2026-08-19T13:00:00Z".to_string())),
            counts: StageCounts {
                items_total: Some(manifest.items.len() as u64),
                items_done: manifest.items.len() as u64,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: manifest.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::from("1"),
        added: manifest.items.clone(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: manifest.items.len() as u64,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

#[tokio::test]
async fn registry_adapter_maps_structured_catalog_to_documents_and_shared_candidates() {
    let mut dump = dump_with_skill(24_531);
    dump.skills[0].audit_status = Some("available".to_string());
    dump.skills[0].audits.push(SkillsShAudit {
        provider: "Socket".to_string(),
        slug: "socket".to_string(),
        status: "pass".to_string(),
        summary: "No alerts".to_string(),
        audited_at: "2026-08-19T14:00:00Z".to_string(),
        risk_level: Some("LOW".to_string()),
        categories: Vec::new(),
    });
    let (_temp, path) = write_dump(&dump);
    let mut plan = plan_with_options(MetadataMap::new(), Some(10));
    set_dump_path(&mut plan, &path);
    let item_store = path.parent().expect("dump parent").join("items");
    std::fs::create_dir(&item_store).expect("item store");
    for skill in &dump.skills {
        let filename = map::materialized_item_filename(&plan, skill).expect("item filename");
        std::fs::write(
            item_store.join(filename),
            serde_json::to_vec(skill).expect("serialize skill"),
        )
        .expect("write keyed skill");
    }
    set_item_store_path(&mut plan, &item_store);
    set_observed_at(&mut plan, &dump.observed_at);
    let adapter = RegistrySourceAdapter::new();

    let manifest = adapter.discover(&plan).await.expect("discover catalog");
    assert_eq!(manifest.scope, SourceScope::Api);
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(manifest.items[0].item_kind, ItemKind::Artifact);
    assert!(
        manifest.items[0]
            .content_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );

    let diff = added_diff(&plan, &manifest);
    let acquisition = adapter
        .acquire(&plan, &diff)
        .await
        .expect("acquire listing metadata");
    assert_eq!(acquisition.fetched_items.len(), 1);
    assert!(acquisition.artifacts.is_empty());
    assert!(acquisition.fetched_items[0].raw_artifact_id.is_none());

    let normalized = adapter
        .normalize(&plan, acquisition)
        .await
        .expect("normalize catalog listing");
    assert_eq!(normalized.data.len(), 1);
    assert_eq!(normalized.data[0].content_kind, ContentKind::Structured);
    let mut graph_metadata = normalized.data[0].metadata.clone();
    let graph_artifacts = axon_parse::vertical::take_metadata_artifacts(&mut graph_metadata);
    assert_eq!(graph_artifacts.graph_candidates.len(), 1);
    let graph = &graph_artifacts.graph_candidates[0];
    assert!(graph.nodes.iter().any(|node| node.node_kind == "source"));
    assert!(graph.nodes.iter().any(|node| node.node_kind == "artifact"));
    assert!(graph.nodes.iter().any(|node| node.node_kind == "repo"));
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.edge_kind == "source_indexed_as")
    );
    assert!(
        graph
            .edges
            .iter()
            .any(|edge| edge.edge_kind == "derived_from")
    );
    assert_eq!(graph.metadata["authorityScope"], "evidence-only");
    assert_eq!(
        graph.evidence[0].metadata["authorityScope"],
        "evidence-only"
    );
    let candidates = adapter
        .artifact_candidates(
            &plan,
            &SourceGenerationId::from("1"),
            &normalized.data,
            &std::collections::BTreeMap::new(),
        )
        .await
        .expect("build candidate evidence");
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    candidate
        .validate_shared_contract()
        .expect("shared candidate contract");
    assert_eq!(candidate.schema_version, ARTIFACT_CANDIDATE_SCHEMA_VERSION);
    assert_eq!(candidate.source_provider, "axon");
    assert_eq!(candidate.repository.as_deref(), Some("vercel-labs/skills"));
    assert_eq!(
        candidate.canonical_source_uri,
        "https://github.com/vercel-labs/skills"
    );
    assert_eq!(candidate.crawl_generation_id.as_deref(), Some("1"));
    let expected_job_id = plan.job_id.0.to_string();
    assert_eq!(
        candidate.crawl_job_id.as_deref(),
        Some(expected_job_id.as_str())
    );
    assert_eq!(candidate.license_evidence["redistribution"], "unknown");
    assert_eq!(
        candidate.discovery_evidence["skillsSh"]["auditStatus"],
        "available"
    );
    assert_eq!(
        candidate.discovery_evidence["skillsSh"]["audits"][0]["status"],
        "pass"
    );
    assert_eq!(
        candidate.discovery_evidence["axonDuplicateEvidence"]["providerSignals"][0]["value"],
        false
    );
    assert_eq!(
        candidate.discovery_evidence["axonDuplicateEvidence"]["authorityScope"],
        "evidence-only"
    );
    assert!(
        candidate.discovery_evidence["axonDuplicateEvidence"]["nearDuplicateCandidateIds"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(candidate.observed_files.is_empty());
    assert!(candidate.content_digests.is_empty());
    assert!(!candidate.permits_public_byte_mirroring());
}

#[tokio::test]
async fn registry_adapter_attaches_item_scoped_semantic_neighbors_as_evidence_only() {
    let dump = dump_with_skill(24_531);
    let (_temp, path) = write_dump(&dump);
    let mut plan = plan_with_options(MetadataMap::new(), Some(10));
    set_dump_path(&mut plan, &path);
    let adapter = RegistrySourceAdapter::new();
    let manifest = adapter.discover(&plan).await.expect("discover catalog");
    let acquisition =
        acquire(&plan, &added_diff(&plan, &manifest), &dump).expect("acquire catalog");
    let normalized = adapter
        .normalize(&plan, acquisition)
        .await
        .expect("normalize catalog");
    let document = &normalized.data[0];
    let mut metadata = MetadataMap::new();
    metadata.insert(
        map::SEMANTIC_NEIGHBOR_IDS_METADATA_KEY.to_string(),
        serde_json::json!([
            "cand_semantic_b",
            "cand_semantic_a",
            "cand_semantic_a",
            42,
            null
        ]),
    );
    let enrichment = SourceEnrichment {
        header: crate::registry_sources::stage_header(
            plan.job_id,
            "test_semantic_neighbors",
            PipelinePhase::Enriching,
            1,
        ),
        source_id: plan.route.source.source_id.clone(),
        source_item_key: document.source_item_key.clone(),
        enrichment_kind: EnrichmentKind::Metadata,
        status: EnrichmentStatus::Completed,
        metadata,
        parse_hints: Vec::new(),
        chunk_hints: Vec::new(),
        graph_candidates: Vec::new(),
        artifacts: Vec::new(),
        warnings: Vec::new(),
    };
    let enrichments =
        std::collections::BTreeMap::from([(document.source_item_key.clone(), enrichment)]);

    let candidates = adapter
        .artifact_candidates(
            &plan,
            &SourceGenerationId::from("1"),
            &normalized.data,
            &enrichments,
        )
        .await
        .expect("build enriched candidate evidence");
    let evidence = &candidates[0].discovery_evidence["axonDuplicateEvidence"];
    assert_eq!(
        evidence["nearDuplicateCandidateIds"],
        serde_json::json!(["cand_semantic_a", "cand_semantic_b"])
    );
    assert_eq!(evidence["nearDuplicateTruncated"], false);
    assert_eq!(evidence["authorityScope"], "evidence-only");
}

#[test]
fn listing_digest_changes_when_catalog_evidence_changes() {
    let first = dump_with_skill(10);
    let second = dump_with_skill(11);
    let plan = plan_with_options(MetadataMap::new(), Some(10));
    let first_manifest = discover(&plan, &first).expect("first manifest");
    let second_manifest = discover(&plan, &second).expect("second manifest");
    assert_ne!(
        first_manifest.items[0].content_hash,
        second_manifest.items[0].content_hash
    );
    assert_eq!(
        first_manifest.items[0].source_item_key,
        second_manifest.items[0].source_item_key
    );
}

#[test]
fn audit_evidence_changes_listing_digest_without_changing_item_identity() {
    let first = dump_with_skill(10);
    let mut second = dump_with_skill(10);
    second.skills[0].audit_status = Some("available".to_string());
    second.skills[0].audits.push(SkillsShAudit {
        provider: "Socket".to_string(),
        slug: "socket".to_string(),
        status: "warn".to_string(),
        summary: "Review recommended".to_string(),
        audited_at: "2026-08-19T14:00:00Z".to_string(),
        risk_level: Some("MEDIUM".to_string()),
        categories: Vec::new(),
    });
    let plan = plan_with_options(MetadataMap::new(), Some(10));
    let first_manifest = discover(&plan, &first).expect("first manifest");
    let second_manifest = discover(&plan, &second).expect("audited manifest");

    assert_ne!(
        first_manifest.items[0].content_hash,
        second_manifest.items[0].content_hash
    );
    assert_eq!(
        first_manifest.items[0].source_item_key,
        second_manifest.items[0].source_item_key
    );
}

#[test]
fn canonical_search_route_infers_search_mode_without_duplicate_mode_option() {
    let mut values = MetadataMap::new();
    values.insert("query".to_string(), serde_json::json!("mcp"));
    let plan = search_plan_with_options(values, Some(25));

    let options = options(&plan).expect("canonical search route selects search mode");
    assert_eq!(options.mode, SkillsShMode::Search);
    assert_eq!(options.query.as_deref(), Some("mcp"));
}

#[test]
fn search_requires_a_real_query_and_uses_search_limit_ceiling() {
    let mut bad = MetadataMap::new();
    bad.insert("query".to_string(), serde_json::json!("x"));
    assert!(options(&search_plan_with_options(bad, None)).is_err());

    let mut good = MetadataMap::new();
    good.insert("query".to_string(), serde_json::json!("mcp servers"));
    good.insert("owner".to_string(), serde_json::json!("dinglebear-ai"));
    good.insert("per_page".to_string(), serde_json::json!(500));
    let options = options(&search_plan_with_options(good, Some(500))).expect("search options");
    assert_eq!(options.mode, SkillsShMode::Search);
    assert_eq!(options.query.as_deref(), Some("mcp servers"));
    assert_eq!(options.owner.as_deref(), Some("dinglebear-ai"));
    assert_eq!(options.per_page, 200);
}

#[test]
fn incremental_diff_fetches_only_modified_and_keeps_removed_as_reconciliation() {
    let plan = plan_with_options(MetadataMap::new(), Some(10));
    let mut previous = dump_with_skill(10);
    let mut removed_skill = previous.skills[0].clone();
    removed_skill.id = "acme/skills/retired-skill".to_string();
    removed_skill.slug = "retired-skill".to_string();
    removed_skill.name = "retired-skill".to_string();
    removed_skill.source = "acme/skills".to_string();
    removed_skill.install_url = Some("https://github.com/acme/skills".to_string());
    removed_skill.url = Some("https://skills.sh/acme/skills/retired-skill".to_string());
    previous.skills.push(removed_skill);

    let previous_manifest = discover(&plan, &previous).expect("previous manifest");
    let mut current = dump_with_skill(11);
    current.observed_at = Timestamp("2026-08-19T13:05:00Z".to_string());
    let current_manifest = discover(&plan, &current).expect("current manifest");
    let modified = current_manifest.items[0].clone();
    let removed = previous_manifest
        .items
        .iter()
        .find(|item| item.source_item_key != modified.source_item_key)
        .expect("removed prior item")
        .clone();
    let diff = SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::for_job_stage(plan.job_id, "skills-sh-incremental-diff", 0),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: current.observed_at.clone(),
            completed_at: Some(current.observed_at.clone()),
            counts: StageCounts {
                items_total: Some(2),
                items_done: 2,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: current_manifest.source_id.clone(),
        previous_generation: Some(previous_manifest.generation.clone()),
        next_generation: SourceGenerationId::from("2"),
        added: Vec::new(),
        modified: vec![modified.clone()],
        removed: vec![removed.clone()],
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 1,
            removed: 1,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    };

    let acquisition = acquire(&plan, &diff, &current).expect("acquire changed rows");
    assert_eq!(acquisition.fetched_items.len(), 1);
    assert_eq!(
        acquisition.fetched_items[0].manifest_item.source_item_key,
        modified.source_item_key
    );
    assert!(
        acquisition
            .manifest
            .items
            .iter()
            .all(|item| item.source_item_key != removed.source_item_key),
        "removed rows are reconciliation evidence, not acquisition/deletion work"
    );
    assert_eq!(diff.removed, vec![removed]);

    let normalized = normalize(&plan, &acquisition).expect("normalize modified row");
    assert_eq!(normalized.data.len(), 1);
    let candidates = artifact_candidates(
        &plan,
        &diff.next_generation,
        &normalized.data,
        &std::collections::BTreeMap::new(),
    )
    .expect("candidate from modified row");
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].manifest_metadata["axonSourceItemKey"],
        serde_json::json!(modified.source_item_key.0)
    );
}

#[test]
fn unchanged_diff_produces_no_acquisition_or_candidate_input() {
    let plan = plan_with_options(MetadataMap::new(), Some(10));
    let dump = dump_with_skill(10);
    let manifest = discover(&plan, &dump).expect("manifest");
    let diff = SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::for_job_stage(plan.job_id, "skills-sh-unchanged-diff", 0),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: dump.observed_at.clone(),
            completed_at: Some(dump.observed_at.clone()),
            counts: StageCounts {
                items_total: Some(1),
                items_done: 1,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: manifest.source_id.clone(),
        previous_generation: Some(manifest.generation.clone()),
        next_generation: SourceGenerationId::from("2"),
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: manifest.items.clone(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 1,
            skipped: 0,
            failed: 0,
        },
    };

    let acquisition = acquire(&plan, &diff, &dump).expect("unchanged acquisition");
    assert!(acquisition.fetched_items.is_empty());
    assert!(acquisition.manifest.items.is_empty());
    let normalized = normalize(&plan, &acquisition).expect("unchanged normalize");
    assert!(normalized.data.is_empty());
    let candidates = artifact_candidates(
        &plan,
        &diff.next_generation,
        &normalized.data,
        &std::collections::BTreeMap::new(),
    )
    .expect("unchanged candidate projection");
    assert!(candidates.is_empty());
}

#[test]
fn keyed_materialization_supports_multiple_batches_without_reloading_dump() {
    let mut plan = plan_with_options(MetadataMap::new(), Some(10));
    let mut dump = dump_with_skill(10);
    let mut second = dump.skills[0].clone();
    second.id = "acme/skills/second".to_string();
    second.slug = "second".to_string();
    second.name = "second".to_string();
    second.source = "acme/skills".to_string();
    second.install_url = Some("https://github.com/acme/skills".to_string());
    second.url = Some("https://skills.sh/acme/skills/second".to_string());
    dump.skills.push(second);

    let temporary = tempfile::tempdir().expect("tempdir");
    let item_store = temporary.path().join("items");
    std::fs::create_dir(&item_store).expect("item store");
    for skill in &dump.skills {
        let filename = map::materialized_item_filename(&plan, skill).expect("item filename");
        std::fs::write(
            item_store.join(filename),
            serde_json::to_vec(skill).expect("serialize skill"),
        )
        .expect("write keyed skill");
    }
    set_item_store_path(&mut plan, &item_store);
    set_observed_at(&mut plan, &dump.observed_at);

    let manifest = discover(&plan, &dump).expect("discover once");
    for (index, item) in manifest.items.iter().cloned().enumerate() {
        let diff = SourceManifestDiff {
            header: StageResultHeader {
                job_id: plan.job_id,
                stage_id: StageId::for_job_stage(plan.job_id, "skills-sh-keyed-batch", index),
                phase: PipelinePhase::Diffing,
                status: LifecycleStatus::Completed,
                started_at: dump.observed_at.clone(),
                completed_at: Some(dump.observed_at.clone()),
                counts: StageCounts {
                    items_total: Some(1),
                    items_done: 1,
                    documents_total: None,
                    documents_done: 0,
                    chunks_total: None,
                    chunks_done: 0,
                    bytes_total: None,
                    bytes_done: 0,
                },
                warnings: Vec::new(),
                error: None,
            },
            source_id: manifest.source_id.clone(),
            previous_generation: None,
            next_generation: SourceGenerationId::from("keyed"),
            added: vec![item.clone()],
            modified: Vec::new(),
            removed: Vec::new(),
            unchanged: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            counts: DiffCounts {
                added: 1,
                modified: 0,
                removed: 0,
                unchanged: 0,
                skipped: 0,
                failed: 0,
            },
        };
        let acquisition = acquire_materialized(&plan, &diff).expect("keyed batch acquisition");
        assert_eq!(acquisition.fetched_items.len(), 1);
        assert_eq!(acquisition.fetched_items[0].fetched_at, dump.observed_at);
        let normalized = normalize(&plan, &acquisition).expect("normalize inline item");
        assert_eq!(normalized.data.len(), 1);
        assert_eq!(normalized.data[0].source_item_key, item.source_item_key);
    }
}
