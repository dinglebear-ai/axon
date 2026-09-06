//! Cross-family characterization for the production source composition.
//!
//! This is deliberately test-only: observations are derived from the existing
//! public result, durable job stages, and progress events. It does not add a
//! production phase registry or a second pipeline model.

#![allow(unsafe_code)]

use axon_adapters::SourceAdapter;
use axon_adapters::acquisition::MaterializedSource;
use axon_adapters::git::GitSourceAdapter;
use axon_api::source::{
    AdapterRef, ArtifactHandle, ArtifactKind, ArtifactMode, AuthSnapshot, ContentRef,
    GraphWriteSummary, JobEvent, JobEventListRequest, JobStageSnapshot, LifecycleStatus,
    OutputPolicy, PipelinePhase, ResponseMode, SourceKind, SourceRequest, SourceResult,
    SourceScope, Visibility,
};
use axon_core::boundary::ArtifactStore;

#[derive(Debug)]
struct PipelineObservation {
    request: SourceRequest,
    progress: Vec<JobEvent>,
    durable_stages: Vec<JobStageSnapshot>,
    provider_calls: FakeProviderCalls,
    result: SourceResult,
}

#[derive(Debug, PartialEq, Eq)]
struct FakeProviderCalls {
    embedding_batches: usize,
    vector_operations: usize,
    vector_points: usize,
}

async fn observe(
    request: SourceRequest,
    harness: &crate::test_support::SourceWebJobIdentityHarness,
) -> anyhow::Result<PipelineObservation> {
    let result = crate::source::index_source_with_auth(
        request.clone(),
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("differential-test")),
    )
    .await?;
    let store = harness.ctx().job_store().expect("job store");
    let events = store
        .events(JobEventListRequest {
            job_id: result.job_id,
            after_sequence: None,
            limit: Some(256),
            severity: None,
            visibility: Some(Visibility::Public),
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await?
        .events;
    let durable_stages = store.stages(result.job_id).await?;
    let provider_calls = FakeProviderCalls {
        embedding_batches: harness.embedder().calls().await.len(),
        vector_operations: harness.vectors().calls().await.len(),
        vector_points: harness
            .vectors()
            .points(&harness.ctx().cfg().collection)
            .await
            .len(),
    };
    Ok(PipelineObservation {
        request,
        progress: events,
        durable_stages,
        provider_calls,
        result,
    })
}

async fn observe_materialized_git(
    repo_root: &std::path::Path,
    harness: &crate::test_support::SourceWebJobIdentityHarness,
) -> anyhow::Result<PipelineObservation> {
    let input = "https://github.com/example/differential.git";
    let request = SourceRequest::new(input);
    let mut routed = crate::source::routing::resolve_source_route(&request)?;
    assert_eq!(routed.kind, SourceKind::Git);

    let adapter = GitSourceAdapter::new();
    let adapter_ref = AdapterRef {
        name: adapter.name().to_string(),
        version: adapter.version().to_string(),
    };
    routed.route.adapter = adapter_ref.clone();
    routed.route.source.adapter = adapter_ref.clone();
    routed.route.validated_options.values.insert(
        "repo_root".to_string(),
        serde_json::json!(repo_root.to_string_lossy()),
    );
    let plan = crate::source::dispatch::family_source_plan(input, &routed.route, true, None, None);
    let observed_request = plan.request.clone();
    let auth = AuthSnapshot::trusted_system("differential-git-test");
    let execution =
        crate::source::SourceExecutionContext::inline(plan.request.clone(), Some(auth.clone()));
    let runtime = harness
        .ctx()
        .target_local_source_runtime()
        .expect("target runtime");
    let collection = harness.ctx().cfg().collection.clone();
    let materialized_path = repo_root.to_path_buf();
    let counts = crate::source::dispatch::dispatch_materialized(
        runtime,
        &adapter,
        plan,
        &collection,
        "differential-test",
        Some(&auth),
        &execution,
        move |plan| async move { Ok(MaterializedSource::persistent(plan, materialized_path)) },
    )
    .await?;

    let result = crate::source::result_map::to_source_result(
        SourceKind::Git,
        adapter_ref,
        routed.route.scope,
        routed.route.source.canonical_uri,
        counts.clone(),
        GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: false,
        },
    );
    let store = harness.ctx().job_store().expect("job store");
    let progress = store
        .events(JobEventListRequest {
            job_id: counts.job_id,
            after_sequence: None,
            limit: Some(256),
            severity: None,
            visibility: Some(Visibility::Public),
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await?
        .events;
    let durable_stages = store.stages(counts.job_id).await?;
    let provider_calls = FakeProviderCalls {
        embedding_batches: harness.embedder().calls().await.len(),
        vector_operations: harness.vectors().calls().await.len(),
        vector_points: harness.vectors().points(&collection).await.len(),
    };
    Ok(PipelineObservation {
        request: observed_request,
        progress,
        durable_stages,
        provider_calls,
        result,
    })
}

fn phase_spine(observation: &PipelineObservation) -> Vec<PipelinePhase> {
    let mut phases = Vec::new();
    for phase in observation
        .progress
        .iter()
        .filter(|event| {
            event.status != LifecycleStatus::CompletedDegraded
                && event.phase != PipelinePhase::Graphing
        })
        .map(|event| event.phase)
    {
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
    }
    phases
}

fn public_observation(observation: &PipelineObservation) -> (Vec<PipelinePhase>, usize, usize) {
    (
        phase_spine(observation),
        observation.durable_stages.len(),
        observation.result.counts.documents_total as usize,
    )
}

fn expected_shared_phase_spine() -> Vec<PipelinePhase> {
    vec![
        PipelinePhase::Leasing,
        PipelinePhase::Discovering,
        PipelinePhase::Diffing,
        PipelinePhase::Fetching,
        PipelinePhase::Enriching,
        PipelinePhase::Normalizing,
        PipelinePhase::Preparing,
        PipelinePhase::Batching,
        PipelinePhase::Embedding,
        PipelinePhase::Vectorizing,
        PipelinePhase::Upserting,
        PipelinePhase::Publishing,
    ]
}

fn assert_one_job(observation: &PipelineObservation) {
    assert_eq!(
        observation.result.status,
        LifecycleStatus::Completed,
        "unexpected source warnings: {:?}",
        observation.result.warnings
    );
    assert!(!observation.result.canonical_uri.is_empty());
    assert!(!observation.request.source.is_empty());
}

fn write_session_fixture(home: &std::path::Path) -> String {
    let root = home.join(".claude/projects/-home-differential");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("fixture.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/differential","timestamp":"2026-01-01T00:00:00Z","message":{"content":"session fixture"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"fake","content":[{"type":"text","text":"response"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    format!("session:claude:{}", root.display())
}

#[tokio::test]
async fn web_local_and_git_share_the_observable_source_contract() {
    let web = crate::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let web_request = SourceRequest::new("https://docs.example.test/differential");
    let web_observation = observe(web_request.clone(), &web).await.unwrap();

    let local_dir = crate::test_support::visible_tempdir().unwrap();
    std::fs::write(
        local_dir.path().join("fixture.md"),
        "# differential fixture\n\nshared source body\n",
    )
    .unwrap();
    let local = crate::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let local_request = SourceRequest::local_path(local_dir.path().to_string_lossy(), true);
    let local_observation = observe(local_request.clone(), &local).await.unwrap();

    let git_dir = crate::test_support::visible_tempdir().unwrap();
    std::fs::write(
        git_dir.path().join("fixture.md"),
        "# differential fixture\n\nshared source body\n",
    )
    .unwrap();
    let git = crate::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let git_observation = observe_materialized_git(git_dir.path(), &git)
        .await
        .unwrap();

    for observation in [&web_observation, &local_observation, &git_observation] {
        assert_one_job(observation);
        assert_eq!(phase_spine(observation), expected_shared_phase_spine());
    }
    assert_eq!(
        public_observation(&web_observation).0,
        public_observation(&local_observation).0,
        "web/local phase drift: web={:?} local={:?}",
        phase_spine(&web_observation),
        phase_spine(&local_observation)
    );
    assert_eq!(
        public_observation(&web_observation).0,
        public_observation(&git_observation).0,
        "web/git phase drift: web={:?} git={:?}",
        phase_spine(&web_observation),
        phase_spine(&git_observation)
    );
    assert_eq!(
        web_observation.result.counts.documents_total,
        local_observation.result.counts.documents_total
    );
    assert_eq!(
        web_observation.result.counts.documents_total,
        git_observation.result.counts.documents_total
    );
    assert_eq!(
        web_observation.provider_calls,
        local_observation.provider_calls
    );
    assert_eq!(
        web_observation.provider_calls,
        git_observation.provider_calls
    );
    assert!(
        web_observation
            .progress
            .iter()
            .all(|event| !event.message.contains("shared source body")),
        "progress must not contain document content"
    );
}

#[tokio::test]
async fn shared_web_output_supports_inline_and_durable_archive_modes() {
    let inline_harness = crate::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let mut inline_request = SourceRequest::new("https://docs.example.test/inline");
    inline_request.scope = Some(SourceScope::Page);
    inline_request.embed = false;
    inline_request.output = OutputPolicy {
        response_mode: ResponseMode::Inline,
        inline_limit_bytes: 4096,
        artifact_mode: ArtifactMode::None,
        ..OutputPolicy::default()
    };
    let inline_result = crate::source::index_source_with_auth(
        inline_request,
        inline_harness.ctx(),
        Some(AuthSnapshot::trusted_system("output-test")),
    )
    .await
    .unwrap();
    let ContentRef::InlineText { text } = inline_result
        .inline
        .expect("inline result")
        .content
        .expect("inline content")
    else {
        panic!("expected inline text")
    };
    assert!(!text.trim().is_empty(), "inline output was empty: {text:?}");
    assert!(inline_result.artifacts.is_empty());

    let archive_harness = crate::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let mut archive_request = SourceRequest::new("https://docs.example.test/archive");
    archive_request.scope = Some(SourceScope::Page);
    archive_request.embed = false;
    archive_request.output = OutputPolicy {
        response_mode: ResponseMode::Artifact,
        inline_limit_bytes: 1,
        artifact_mode: ArtifactMode::Always,
        ..OutputPolicy::default()
    };
    archive_request.options.values.insert(
        "warc_path".to_string(),
        serde_json::json!("artifact://source/archive.warc"),
    );
    let archive_result = crate::source::index_source_with_auth(
        archive_request,
        archive_harness.ctx(),
        Some(AuthSnapshot::trusted_system("output-test")),
    )
    .await
    .unwrap();
    assert!(archive_result.inline.is_none());
    let warc = archive_result
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_kind == ArtifactKind::Warc)
        .expect("warc artifact");
    assert!(
        warc.content_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
    assert!(
        archive_result
            .artifacts
            .iter()
            .any(|artifact| artifact.artifact_kind == ArtifactKind::NormalizedContent)
    );
    let stored = ArtifactStore::get(
        archive_harness.core().as_ref(),
        ArtifactHandle {
            artifact_id: warc.artifact_id.clone(),
            artifact_kind: ArtifactKind::Warc,
            uri: Some(warc.uri.clone()),
        },
    )
    .await
    .expect("stored warc");
    assert_eq!(stored.metadata["producer"], "web");
}

#[tokio::test]
#[serial_test::serial]
async fn session_source_joins_the_same_observable_source_contract() {
    let home = crate::test_support::visible_tempdir().unwrap();
    let previous_home = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", home.path()) };
    let request = SourceRequest::new(write_session_fixture(home.path()));
    let harness = crate::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let observation = observe(request, &harness).await;
    match previous_home {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    let observation = observation.unwrap();
    assert_one_job(&observation);
    assert_eq!(phase_spine(&observation), expected_shared_phase_spine());
    assert_eq!(observation.result.counts.documents_total, 1);
    let points = harness
        .vectors()
        .points(&harness.ctx().cfg().collection)
        .await;
    assert!(!points.is_empty());
    assert!(points.iter().all(|point| {
        point
            .payload
            .get("redaction_status")
            .and_then(serde_json::Value::as_str)
            == Some("clean")
    }));
    assert!(points.iter().all(|point| {
        point
            .payload
            .get("item_canonical_uri")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|uri| uri.starts_with("session://"))
    }));
    assert!(observation.progress.iter().all(|event| {
        !event.message.contains("session fixture") && !event.message.contains("response")
    }));
}

#[tokio::test]
async fn route_failures_map_to_the_same_failed_source_result() {
    for input in ["", "ftp://unsupported.example.test/source"] {
        let harness = crate::test_support::source_context_with_fake_web()
            .await
            .unwrap();
        let result = crate::source::index_source_with_auth(
            SourceRequest::new(input),
            harness.ctx(),
            Some(AuthSnapshot::trusted_system("differential-test")),
        )
        .await
        .unwrap();
        assert_eq!(result.status, LifecycleStatus::Failed, "input={input:?}");
        assert!(
            !result.warnings.is_empty(),
            "route failure must be surfaced as a warning: input={input:?}"
        );
        assert_eq!(
            result.counts.documents_total, 0,
            "route failure must not prepare documents: input={input:?}"
        );
    }
}
