use super::*;

#[tokio::test]
async fn terminal_incomplete_and_corrupt_spools_release_admission_but_unknown_work_stays() {
    for phase in ["owner_only", "empty_database", "corrupt_database"] {
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
        let work_root = runtime.generation_work_root.as_ref().unwrap();
        let mut generations = Vec::new();
        for index in 0..4 {
            let generation = ledger
                .create_generation(SourceId::new("source"))
                .await
                .unwrap();
            let identity = WorkOwner {
                source: generation.source_id.clone(),
                generation: generation.generation.clone(),
                job: owner(index).job,
                attempt: 1,
            };
            let directory = register(work_root, &identity).unwrap();
            if phase != "owner_only" {
                let bytes: &[u8] = if phase == "empty_database" {
                    b""
                } else {
                    b"not a SQLite database"
                };
                std::fs::write(directory.join("records.sqlite"), bytes).unwrap();
            }
            generations.push(generation);
        }
        assert_eq!(
            recover(&runtime).await.unwrap(),
            0,
            "unknown work must survive {phase}"
        );
        assert_eq!(entries(work_root).unwrap().len(), 4);
        for generation in generations {
            ledger.fail_generation(generation).await.unwrap();
        }
        assert_eq!(
            recover(&runtime).await.unwrap(),
            4,
            "terminal {phase} work must release capacity"
        );
        assert!(entries(work_root).unwrap().is_empty());
        assert_eq!(recover(&runtime).await.unwrap(), 0);
        register(work_root, &owner(99)).expect("recovered capacity admits fresh work");
    }
}

#[tokio::test]
async fn terminal_receipt_never_deletes_an_actively_owned_spool() {
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
    let spool = super::super::super::generation_spool::GenerationSpool::persistent_for(
        work_root,
        generation.source_id.clone(),
        generation.generation.clone(),
        owner(1).job,
        1,
    )
    .unwrap();
    ledger.fail_generation(generation).await.unwrap();
    assert_eq!(recover(&runtime).await.unwrap(), 0);
    assert_eq!(entries(work_root).unwrap().len(), 1);
    drop(spool);
    assert_eq!(recover(&runtime).await.unwrap(), 1);
}

#[tokio::test]
async fn restart_after_owner_only_process_exit_reconciles_only_terminal_work() {
    let root = tempfile::tempdir().unwrap();
    let ledger = Arc::new(FakeLedgerStore::new());
    ledger.upsert_source(source()).await.unwrap();
    let generation = ledger
        .create_generation(SourceId::new("source"))
        .await
        .unwrap();
    let work_root = root.path().join("work");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("source::executor::generation_registry::tests::persistent_work_crash_child")
        .env("AXON_TEST_REGISTERED_CRASH_ROOT", &work_root)
        .env(
            "AXON_TEST_REGISTERED_CRASH_GENERATION",
            &generation.generation.0,
        )
        .env("AXON_TEST_REGISTERED_CRASH_PHASE", "owner_only")
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
    runtime.generation_work_root = Some(work_root.clone());
    assert_eq!(recover(&runtime).await.unwrap(), 0);
    ledger.fail_generation(generation).await.unwrap();
    assert_eq!(recover(&runtime).await.unwrap(), 1);
    assert!(entries(&work_root).unwrap().is_empty());
}

#[test]
fn registration_retains_cross_process_fence_until_spool_owner_is_ready() {
    let root = tempfile::tempdir().unwrap();
    let (directory, registration) = register_locked(root.path(), &owner(1)).unwrap();
    assert!(directory.join("owner.json").exists());
    assert!(!directory.join("records.sqlite").exists());
    assert!(
        lock(root.path()).is_err(),
        "recovery must not enter the initialization gap"
    );
    drop(registration);
    assert!(lock(root.path()).is_ok());
}
