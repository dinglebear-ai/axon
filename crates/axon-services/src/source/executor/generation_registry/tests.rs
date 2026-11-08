use super::*;
use crate::context::TargetLocalSourceRuntime;
use axon_ledger::store::{FakeLedgerStore, LedgerStore as _};
use std::sync::Arc;

fn owner(index: u32) -> WorkOwner {
    WorkOwner {
        source: SourceId::new("source"),
        generation: SourceGenerationId::new(format!("generation-{index}")),
        job: JobId::new(uuid::Uuid::from_u128(1)),
        attempt: index,
    }
}

#[test]
fn registration_is_scoped_private_bounded_and_never_reuses_an_attempt() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..4 {
        let owner = owner(index);
        let path = register(root.path(), &owner).unwrap();
        assert_eq!(
            read_owner(&path).unwrap().key().unwrap(),
            owner.key().unwrap()
        );
        assert!(register(root.path(), &owner).is_err());
    }
    assert!(register(root.path(), &owner(5)).is_err());
    assert_eq!(entries(root.path()).unwrap().len(), 4);
}

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("source"),
        canonical_uri: "file:///fixture".into(),
        display_name: "fixture".into(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".into(),
            version: "test".into(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: super::super::helpers::empty_source_counts(),
        created_at: Timestamp("2026-10-16T00:00:00Z".into()),
        updated_at: Timestamp("2026-10-16T00:00:00Z".into()),
        tags: Vec::new(),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        user_label: None,
    }
}

#[tokio::test]
async fn recovery_preserves_active_and_uncertain_work_then_reconciles_proven_failed_receipt() {
    let root = tempfile::tempdir().unwrap();
    let ledger = Arc::new(FakeLedgerStore::new());
    ledger.upsert_source(source()).await.unwrap();
    let generation = ledger
        .create_generation(SourceId::new("source"))
        .await
        .unwrap();
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(axon_embedding::fake::FakeEmbeddingProvider::new(
            "embedding",
            8,
        )),
        Arc::new(axon_vectors::store::FakeVectorStore::new("vectors")),
        ProviderId::new("embedding"),
        "embedding",
        8,
    );
    runtime.generation_work_root = Some(root.path().join("work"));
    let work_root = runtime.generation_work_root.as_ref().unwrap();
    let spool = super::super::generation_spool::GenerationSpool::persistent_for(
        work_root,
        generation.source_id.clone(),
        generation.generation.clone(),
        JobId::new(uuid::Uuid::from_u128(1)),
        1,
    )
    .unwrap();
    assert_eq!(
        recover(&runtime).await.unwrap(),
        0,
        "active ownership must remain fenced"
    );
    drop(spool);
    let entry = entries(work_root).unwrap().pop().unwrap();
    let owner = read_owner(&entry).unwrap();
    drop(
        super::super::generation_spool::open_registered(&entry, &owner.key().unwrap())
            .expect("closed registered spool must reopen before receipt checks"),
    );

    assert_eq!(
        recover(&runtime).await.unwrap(),
        0,
        "closed ownership cannot imply a commit or failure"
    );
    assert_eq!(entries(work_root).unwrap().len(), 1);
    ledger.fail_generation(generation).await.unwrap();
    assert_eq!(recover(&runtime).await.unwrap(), 1);
    assert!(entries(work_root).unwrap().is_empty());
    assert_eq!(
        recover(&runtime).await.unwrap(),
        0,
        "reconciliation is idempotent"
    );
}

#[test]
fn owner_identity_mismatch_is_preserved_for_inspection() {
    let root = tempfile::tempdir().unwrap();
    let directory = register(root.path(), &owner(1)).unwrap();
    std::fs::write(
        directory.join("owner.json"),
        serde_json::to_vec(&owner(2)).unwrap(),
    )
    .unwrap();
    assert!(read_owner(&directory).is_err());
    assert!(directory.exists());
}

#[test]
fn persistent_work_crash_child() {
    let Ok(root) = std::env::var("AXON_TEST_REGISTERED_CRASH_ROOT") else {
        return;
    };
    let generation =
        SourceGenerationId::new(std::env::var("AXON_TEST_REGISTERED_CRASH_GENERATION").unwrap());
    if std::env::var("AXON_TEST_REGISTERED_CRASH_PHASE").unwrap() == "owner_only" {
        register(
            std::path::Path::new(&root),
            &WorkOwner {
                source: SourceId::new("source"),
                generation,
                job: JobId::new(uuid::Uuid::from_u128(1)),
                attempt: 1,
            },
        )
        .unwrap();
        std::process::exit(37);
    }
    let mut spool = super::super::generation_spool::GenerationSpool::persistent_for(
        std::path::Path::new(&root),
        SourceId::new("source"),
        generation,
        JobId::new(uuid::Uuid::from_u128(1)),
        1,
    )
    .unwrap();
    let record = super::super::generation_spool::SideEffectsSpoolRecord {
        skipped_statuses: Vec::new(),
        archive_items: Vec::new(),
        artifact_candidates: Vec::new(),
        warnings: vec![SourceWarning {
            code: "crash.fixture".into(),
            severity: Severity::Warning,
            message: "durable record".into(),
            source_item_key: None,
            retryable: false,
        }],
        reused_item_keys: Vec::new(),
        refreshed_manifest_items: Vec::new(),
    };
    spool.append("durable", &record).unwrap();
    let archive = tempfile::NamedTempFile::new_in(spool.work_directory().unwrap()).unwrap();
    archive.as_file().sync_all().unwrap();
    if std::env::var("AXON_TEST_REGISTERED_CRASH_PHASE").unwrap() == "sealed" {
        spool.seal().unwrap();
    }
    std::process::exit(37); // Deliberately skip every Rust destructor.
}

#[tokio::test]
async fn restart_before_and_after_seal_preserves_unknown_work_then_reaps_only_a_failed_receipt() {
    for phase in ["appended", "sealed"] {
        let root = tempfile::tempdir().unwrap();
        let ledger = Arc::new(FakeLedgerStore::new());
        ledger.upsert_source(source()).await.unwrap();
        let generation = ledger
            .create_generation(SourceId::new("source"))
            .await
            .unwrap();
        let root_path = root.path().join("work");
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("source::executor::generation_registry::tests::persistent_work_crash_child")
            .env("AXON_TEST_REGISTERED_CRASH_ROOT", &root_path)
            .env(
                "AXON_TEST_REGISTERED_CRASH_GENERATION",
                &generation.generation.0,
            )
            .env("AXON_TEST_REGISTERED_CRASH_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(37));
        let mut runtime = TargetLocalSourceRuntime::new(
            Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
            ledger.clone(),
            Arc::new(axon_embedding::fake::FakeEmbeddingProvider::new(
                "embedding",
                8,
            )),
            Arc::new(axon_vectors::store::FakeVectorStore::new("vectors")),
            ProviderId::new("embedding"),
            "embedding",
            8,
        );
        runtime.generation_work_root = Some(root_path.clone());
        assert_eq!(recover(&runtime).await.unwrap(), 0);
        let directory = entries(&root_path).unwrap().pop().unwrap();
        let owner = read_owner(&directory).unwrap();
        let spool =
            super::super::generation_spool::open_registered(&directory, &owner.key().unwrap())
                .unwrap();
        let mut records = 0;
        spool
            .replay_each::<super::super::generation_spool::SideEffectsSpoolRecord>(|_, record| {
                assert_eq!(record.warnings[0].message, "durable record");
                records += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(records, 1);
        drop(spool);
        assert!(
            std::fs::read_dir(&directory).unwrap().count() >= 4,
            "crash archive remains owned by work entry"
        );
        ledger.fail_generation(generation).await.unwrap();
        assert_eq!(recover(&runtime).await.unwrap(), 1);
        assert!(entries(&root_path).unwrap().is_empty());
        assert!(
            ledger
                .committed_generation(&SourceId::new("source"))
                .await
                .is_none()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_generations_share_admission_and_wait_for_a_settled_slot() {
    let root = tempfile::tempdir().unwrap();
    let ledger = Arc::new(FakeLedgerStore::new());
    ledger.upsert_source(source()).await.unwrap();
    let mut runtime = TargetLocalSourceRuntime::new(
        Arc::new(axon_jobs::boundary::FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(axon_embedding::fake::FakeEmbeddingProvider::new(
            "embedding",
            8,
        )),
        Arc::new(axon_vectors::store::FakeVectorStore::new("vectors")),
        ProviderId::new("embedding"),
        "embedding",
        8,
    );
    runtime.generation_work_root = Some(root.path().join("work"));
    let runtime = Arc::new(runtime);
    let mut generations = Vec::new();
    for _ in 0..5 {
        generations.push(
            ledger
                .create_generation(SourceId::new("source"))
                .await
                .unwrap(),
        );
    }
    let mut tasks = Vec::new();
    for generation in &generations[..4] {
        let generation = generation.clone();
        let runtime = runtime.clone();
        tasks.push(tokio::spawn(async move {
            super::super::generation_state::GenerationAccumulator::new(
                &generation,
                JobId::new(uuid::Uuid::from_u128(1)),
                1,
                &runtime,
            )
            .await
        }));
    }
    let mut states = Vec::new();
    for task in tasks {
        states.push(
            task.await
                .unwrap()
                .expect("four admitted jobs must not fail on metadata lock contention"),
        );
    }
    assert_eq!(runtime.generation_work_slots.available_permits(), 0);
    let fifth_runtime = runtime.clone();
    let fifth_generation = generations[4].clone();
    let mut fifth = tokio::spawn(async move {
        super::super::generation_state::GenerationAccumulator::new(
            &fifth_generation,
            JobId::new(uuid::Uuid::from_u128(1)),
            1,
            &fifth_runtime,
        )
        .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut fifth)
            .await
            .is_err()
    );
    ledger
        .fail_generation(generations[0].clone())
        .await
        .unwrap();
    drop(states.remove(0));
    let fifth = fifth
        .await
        .unwrap()
        .expect("released settled capacity admits the next full generation");
    assert_eq!(runtime.generation_work_slots.available_permits(), 0);
    drop(fifth);
    drop(states);
}

#[path = "recovery_tests.rs"]
mod recovery_tests;
