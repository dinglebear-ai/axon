use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axon_api::source::*;
use axon_core::boundary::{ArtifactStore, FakeCoreBoundaries};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::store::FakeVectorStore;

static CLEANUP_GLOBAL_TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

struct CountingStore {
    inner: Arc<FakeCoreBoundaries>,
    deletes: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl axon_core::boundary::ArtifactStore for CountingStore {
    async fn put(
        &self,
        value: ArtifactWriteRequest,
    ) -> axon_core::boundary::Result<ArtifactHandle> {
        self.inner.put(value).await
    }
    async fn get(&self, value: ArtifactHandle) -> axon_core::boundary::Result<ArtifactReadResult> {
        self.inner.get(value).await
    }
    async fn delete(&self, value: ArtifactHandle) -> axon_core::boundary::Result<()> {
        self.deletes.fetch_add(1, Ordering::AcqRel);
        self.inner.delete(value).await
    }
    async fn reset(&self) -> axon_core::boundary::Result<()> {
        self.inner.reset().await
    }
    async fn capabilities(&self) -> axon_core::boundary::Result<ArtifactStoreCapability> {
        self.inner.capabilities().await
    }
}

fn work() -> ArtifactCleanupWork {
    ArtifactCleanupWork {
        store: Arc::new(FakeCoreBoundaries::new()),
        ledger: Arc::new(FakeLedgerStore::new()),
        scheduler: None,
        job_id: JobId::new(uuid::Uuid::new_v4()),
        attempt: 2,
        source_id: SourceId::new("src_journal"),
        generation: SourceGenerationId::new("gen_journal"),
        artifacts: vec![ArtifactRef {
            artifact_id: ArtifactId::new("art_journal"),
            artifact_kind: ArtifactKind::NormalizedContent,
            uri: "/secret/local/path".to_string(),
            size_bytes: None,
            content_hash: None,
            created_at: Timestamp("2026-09-04T00:00:00Z".to_string()),
        }],
        journal: None,
    }
}

async fn seeded_counting_work(
    suffix: &str,
    artifact_count: usize,
    deletes: Arc<AtomicUsize>,
) -> ArtifactCleanupWork {
    let ledger = FakeLedgerStore::new();
    let mut pending = work();
    pending.source_id = SourceId::new(format!("src_{suffix}"));
    pending.generation = SourceGenerationId::new(format!("gen_{suffix}"));
    let now = Timestamp("2026-09-04T00:00:00Z".into());
    ledger
        .upsert_source(SourceSummary {
            source_id: pending.source_id.clone(),
            canonical_uri: format!("file:///{suffix}"),
            display_name: suffix.into(),
            source_kind: SourceKind::Local,
            adapter: AdapterRef {
                name: "test".into(),
                version: "1".into(),
            },
            authority: AuthorityLevel::UserPinned,
            status: LifecycleStatus::Running,
            counts: SourceCounts {
                items_total: 0,
                items_changed: 0,
                documents_total: 0,
                chunks_total: 0,
                vector_points_total: 0,
                bytes_total: 0,
            },
            created_at: now.clone(),
            updated_at: now,
            tags: vec![],
            watch_id: None,
            graph_node_ids: vec![],
            last_job_id: None,
            last_refreshed_at: None,
            user_label: None,
        })
        .await
        .unwrap();
    pending.ledger = Arc::new(ledger);
    pending.store = Arc::new(CountingStore {
        inner: Arc::new(FakeCoreBoundaries::new()),
        deletes,
    });
    pending.artifacts = (0..artifact_count)
        .map(|index| ArtifactRef {
            artifact_id: ArtifactId::new(format!("art_{suffix}_{index}")),
            artifact_kind: ArtifactKind::RawContent,
            uri: String::new(),
            size_bytes: None,
            content_hash: None,
            created_at: Timestamp("2026-09-04T00:00:00Z".into()),
        })
        .collect();
    pending
}

#[tokio::test]
async fn journal_is_private_atomic_identity_only_and_removable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let token = persist(&root, &work()).await.unwrap();
    let bytes = tokio::fs::read(&token.0).await.unwrap();
    let json = String::from_utf8(bytes).unwrap();
    assert!(!json.contains("/secret/local/path"));
    assert!(!json.contains("uri"));
    let record: ArtifactCleanupJournalRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record.schema_version, 1);
    assert_eq!(record.artifacts[0].artifact_id.0, "art_journal");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&token.0).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    remove(&token).await.unwrap();
    assert!(!token.0.exists());
}

#[tokio::test]
async fn rewrite_replaces_the_record_with_only_the_unresolved_suffix() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let mut work = work();
    work.artifacts.push(ArtifactRef {
        artifact_id: ArtifactId::new("art_tail"),
        artifact_kind: ArtifactKind::RawContent,
        uri: String::new(),
        size_bytes: None,
        content_hash: None,
        created_at: Timestamp("2026-09-04T00:00:00Z".to_string()),
    });
    let token = persist(&root, &work).await.unwrap();
    work.artifacts.remove(0);
    rewrite(&token, &work).await.unwrap();
    let record: ArtifactCleanupJournalRecord =
        serde_json::from_slice(&tokio::fs::read(&token.0).await.unwrap()).unwrap();
    assert_eq!(record.artifacts.len(), 1);
    assert_eq!(record.artifacts[0].artifact_id.0, "art_tail");
}

#[tokio::test]
async fn failed_create_sync_and_rename_keep_the_previous_authoritative_record() {
    for fault in [
        JournalFault::Create,
        JournalFault::FileSync,
        JournalFault::Rename,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("journal");
        let mut pending = work();
        let token = persist(&root, &pending).await.unwrap();
        let previous = tokio::fs::read(&token.0).await.unwrap();
        pending.artifacts.clear();
        inject_fault(&token.0, fault);

        rewrite(&token, &pending)
            .await
            .expect_err("injected failure");

        assert_eq!(tokio::fs::read(&token.0).await.unwrap(), previous);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn failed_remove_keeps_the_authoritative_journal() {
    let temp = tempfile::tempdir().unwrap();
    let token = persist(&temp.path().join("journal"), &work())
        .await
        .unwrap();
    inject_fault(&token.0, JournalFault::Remove);

    remove(&token).await.expect_err("injected remove failure");

    assert!(token.0.exists());
    let record: ArtifactCleanupJournalRecord =
        serde_json::from_slice(&tokio::fs::read(&token.0).await.unwrap()).unwrap();
    assert_eq!(record.artifacts.len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn journal_root_symlink_is_rejected_without_touching_target() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let root = temp.path().join("journal");
    symlink(&target, &root).unwrap();

    persist(&root, &work())
        .await
        .expect_err("symlink root rejected");

    assert!(std::fs::read_dir(&target).unwrap().next().is_none());
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[test]
fn replay_validation_rejects_empty_and_duplicate_artifact_identity() {
    let mut record = ArtifactCleanupJournalRecord {
        schema_version: SCHEMA_VERSION,
        job_id: JobId::new(uuid::Uuid::nil()),
        attempt: 0,
        source_id: SourceId::new("source"),
        generation: SourceGenerationId::new("generation"),
        artifacts: vec![JournalArtifact {
            artifact_id: ArtifactId::new("artifact"),
            artifact_kind: ArtifactKind::RawContent,
        }],
        created_at: Timestamp("2026-09-04T00:00:00Z".into()),
    };
    let canonical = journal_id_record(&record).to_string();
    assert!(valid_record(&record, &canonical));
    assert!(!valid_record(&record, &uuid::Uuid::new_v4().to_string()));
    record.artifacts.push(record.artifacts[0].clone());
    assert!(!valid_record(&record, &canonical));
    record.artifacts[1].artifact_id = ArtifactId::new("");
    assert!(!valid_record(&record, &canonical));
}

#[cfg(unix)]
#[test]
fn journal_sweep_removes_old_but_preserves_live_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    std::fs::create_dir(&root).unwrap();
    let abandoned = root.join(".journal-abandoned.tmp");
    let owner = root.join("record.owner-abandoned.tmp");
    let live = root.join(".journal-live.tmp");
    std::fs::write(&abandoned, b"partial").unwrap();
    std::fs::write(&owner, b"partial").unwrap();
    std::fs::write(&live, b"active publication").unwrap();
    for path in [&abandoned, &owner] {
        assert!(
            std::process::Command::new("touch")
                .args(["-t", "200001010000"])
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }

    SecureJournalDir::open(&root)
        .unwrap()
        .sweep_stale_temporaries()
        .unwrap();

    assert!(!abandoned.exists());
    assert!(!owner.exists());
    assert_eq!(std::fs::read(live).unwrap(), b"active publication");
}

#[test]
fn paused_temp_publish_process_helper() {
    let Some(root) = std::env::var_os("AXON_TEMP_PUBLISH_HELPER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    std::fs::write(root.join(".journal-live.tmp"), b"publishing").unwrap();
    std::fs::write(root.join("publisher-ready"), b"ready").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}

#[test]
fn startup_sweep_does_not_unlink_another_process_live_publish() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    std::fs::create_dir(&root).unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut publisher = std::process::Command::new(executable)
        .args([
            "--exact",
            "reserved_call::artifact_cleanup_journal::tests::paused_temp_publish_process_helper",
        ])
        .env("AXON_TEMP_PUBLISH_HELPER_ROOT", &root)
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if root.join("publisher-ready").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    SecureJournalDir::open(&root)
        .unwrap()
        .sweep_stale_temporaries()
        .unwrap();
    assert_eq!(
        std::fs::read(root.join(".journal-live.tmp")).unwrap(),
        b"publishing"
    );
    let _ = publisher.kill();
    let _ = publisher.wait();
}

#[test]
fn owner_write_sync_and_rename_failures_leave_no_temporary_file() {
    for fault in [
        JournalFault::OwnerWrite,
        JournalFault::OwnerSync,
        JournalFault::OwnerRename,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let pending = temp.path().join(format!("{}.json", uuid::Uuid::new_v4()));
        let claimed = pending.with_extension("claim");
        std::fs::write(&pending, b"record").unwrap();
        inject_fault(&claimed, fault);

        claim(&pending, &claimed, true).expect_err("injected owner failure");

        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".owner-")
        }));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn swapping_root_after_open_never_touches_external_target() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let displaced = temp.path().join("displaced");
    let external = temp.path().join("external");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&external).unwrap();
    let sentinel = external.join("sentinel");
    std::fs::write(&sentinel, b"unchanged").unwrap();
    std::fs::set_permissions(&external, std::fs::Permissions::from_mode(0o755)).unwrap();
    inject_root_swap(&root, &displaced, &external);

    persist(&root, &work())
        .await
        .expect_err("root replacement detected");

    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unchanged");
    assert_eq!(
        std::fs::metadata(&external).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(std::fs::read_dir(&external).unwrap().count() == 1);
    assert!(
        std::fs::symlink_metadata(&root)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn swapping_root_before_remove_never_unlinks_external_target() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let token = persist(&root, &work()).await.unwrap();
    let displaced = temp.path().join("displaced");
    let external = temp.path().join("external");
    std::fs::create_dir(&external).unwrap();
    let external_record = external.join(token.0.file_name().unwrap());
    std::fs::write(&external_record, b"sentinel").unwrap();
    std::fs::rename(&root, &displaced).unwrap();
    symlink(&external, &root).unwrap();

    remove(&token).await.expect_err("root identity changed");

    assert_eq!(std::fs::read(&external_record).unwrap(), b"sentinel");
    assert!(displaced.join(token.0.file_name().unwrap()).exists());
}

#[tokio::test]
async fn copied_record_with_noncanonical_name_is_quarantined_without_delete() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let token = persist(&root, &work()).await.unwrap();
    let renamed = root.join(format!("{}.json", uuid::Uuid::new_v4()));
    std::fs::rename(&token.0, &renamed).unwrap();
    let deletes = Arc::new(AtomicUsize::new(0));
    let mut runtime = crate::context::TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("identity", 8)),
        Arc::new(FakeVectorStore::new("identity")),
        ProviderId::new("identity"),
        "identity",
        8,
    );
    runtime.artifact_store = Arc::new(CountingStore {
        inner: Arc::new(FakeCoreBoundaries::new()),
        deletes: deletes.clone(),
    });

    let summary = replay(&root, &runtime).await.unwrap();
    super::super::ARTIFACT_CLEANUP_WORKERS.drain();

    assert_eq!(summary.quarantined, 1);
    assert_eq!(summary.claimed, 0);
    assert_eq!(deletes.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn replay_read_failure_retains_claim_for_retry_instead_of_quarantine() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let token = persist(&root, &work()).await.unwrap();
    let claimed = token.0.with_extension("claim");
    inject_fault(&claimed, JournalFault::Read);
    let runtime = crate::context::TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("read", 8)),
        Arc::new(FakeVectorStore::new("read")),
        ProviderId::new("read"),
        "read",
        8,
    );

    let summary = replay(&root, &runtime).await.unwrap();

    assert_eq!(summary.claimed, 0);
    assert_eq!(summary.quarantined, 0);
    assert_eq!(summary.errors.len(), 1);
    assert!(claimed.exists());
}

#[test]
fn stable_lease_namespace_is_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let directory = SecureJournalDir::open(&root).unwrap();
    for value in 0..600_u128 {
        let pending = root.join(format!("{}.json", uuid::Uuid::from_u128(value + 1)));
        let claimed = pending.with_extension("claim");
        std::fs::write(&pending, b"record").unwrap();
        drop(
            directory
                .acquire_lease(&pending, &claimed, true)
                .unwrap()
                .unwrap(),
        );
    }
    let leases = std::fs::read_dir(&root)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("lease-")
        })
        .count();
    assert!(leases <= 256, "lease count was {leases}");
}

#[test]
fn same_process_same_shard_claims_do_not_deadlock_active_registry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("journal");
    let directory = Arc::new(SecureJournalDir::open(&root).unwrap());
    let first = root.join(format!("{}.json", uuid::Uuid::from_u128(1)));
    let second = root.join(format!("{}.json", uuid::Uuid::from_u128(2)));
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let first_claim = first.with_extension("claim");
    let second_claim = second.with_extension("claim");
    assert!(claim_in(&directory, &first, &first_claim, true).unwrap());
    let (sender, receiver) = std::sync::mpsc::channel();
    let waiting_directory = Arc::clone(&directory);
    let waiting_claim = second_claim.clone();
    let second_pending = second.clone();
    std::thread::spawn(move || {
        sender
            .send(claim_in(&waiting_directory, &second, &waiting_claim, true))
            .unwrap();
    });
    std::thread::sleep(std::time::Duration::from_millis(25));
    let (released, release_receiver) = std::sync::mpsc::channel();
    let release_claim = first_claim.clone();
    std::thread::spawn(move || {
        ACTIVE_CLAIMS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&release_claim);
        released.send(()).unwrap();
    });
    release_receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("registry mutex available while shard waiter blocks");
    assert!(
        !receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("second claim skips a live shard owner promptly")
            .unwrap()
    );
    assert!(claim_in(&directory, &second_pending, &second_claim, true).unwrap());
    ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&second_claim);
}

#[test]
fn journal_claim_is_exclusive_and_a_stale_claim_can_be_recovered() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.blocking_lock();
    let temp = tempfile::tempdir().unwrap();
    let pending = temp.path().join(format!("{}.json", uuid::Uuid::new_v4()));
    let claimed = pending.with_extension("claim");
    std::fs::write(&pending, b"record").unwrap();

    assert!(claim(&pending, &claimed, true).unwrap());
    assert!(!claim(&claimed, &claimed, false).unwrap());
    ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&claimed);
    assert!(claim(&claimed, &claimed, false).unwrap());

    ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&claimed);
}

#[test]
fn claim_process_helper() {
    let Some(root) = std::env::var_os("AXON_CLAIM_HELPER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let pending = root.join("00000000-0000-0000-0000-000000000001.json");
    let claimed = pending.with_extension("claim");
    let acquired = claim(&pending, &claimed, pending.exists()).unwrap();
    if acquired {
        use std::io::Write;
        let mut counter = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("destructive-cleanups"))
            .unwrap();
        writeln!(counter, "{}", std::process::id()).unwrap();
        counter.sync_all().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

#[test]
#[serial_test::serial]
fn separate_processes_exclude_live_owner_and_recover_after_owner_death() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.blocking_lock();
    let temp = tempfile::tempdir().unwrap();
    let pending = temp
        .path()
        .join("00000000-0000-0000-0000-000000000001.json");
    std::fs::write(&pending, b"record").unwrap();
    let executable = std::env::current_exe().unwrap();
    let spawn = || {
        std::process::Command::new(&executable)
            .args([
                "--exact",
                "reserved_call::artifact_cleanup_journal::tests::claim_process_helper",
                "--nocapture",
            ])
            .env("AXON_CLAIM_HELPER_ROOT", temp.path())
            .spawn()
            .unwrap()
    };
    let mut first = spawn();
    let counter = temp.path().join("destructive-cleanups");
    for _ in 0..100 {
        if std::fs::read_to_string(&counter)
            .map(|value| value.lines().count() == 1)
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().lines().count(),
        1
    );
    // Replacing the mutable claim record must not replace the stable lease lock.
    let replacement = temp.path().join("replacement.tmp");
    std::fs::write(&replacement, b"updated record").unwrap();
    std::fs::rename(&replacement, pending.with_extension("claim")).unwrap();
    // A concurrently recreated pending record must not overwrite a live claim.
    std::fs::write(&pending, b"duplicate record").unwrap();
    let mut second = spawn();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        if second.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "second replay blocked behind a live lease"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().lines().count(),
        1
    );

    let _ = first.kill();
    let _ = first.wait();
    let mut recovered = spawn();
    for _ in 0..100 {
        if std::fs::read_to_string(&counter)
            .map(|value| value.lines().count() == 2)
            .unwrap_or(false)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().lines().count(),
        2
    );
    let _ = recovered.kill();
    let _ = recovered.wait();
}

#[cfg(unix)]
#[tokio::test]
async fn replay_rejects_symlink_root_without_touching_external_record() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let external = temp.path().join("external");
    let real_token = persist(&external, &work()).await.unwrap();
    let before = std::fs::read(&real_token.0).unwrap();
    let root = temp.path().join("journal-link");
    symlink(&external, &root).unwrap();
    let runtime = crate::context::TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("symlink", 8)),
        Arc::new(FakeVectorStore::new("symlink")),
        ProviderId::new("symlink"),
        "symlink",
        8,
    );

    replay(&root, &runtime)
        .await
        .expect_err("symlink replay root rejected");

    assert_eq!(std::fs::read(&real_token.0).unwrap(), before);
    assert!(!real_token.0.with_extension("claim").exists());
}

#[test]
fn unresolved_empty_suffix_still_counts_as_owned_journal_work() {
    let mut work = work();
    work.artifacts.clear();
    let temp = tempfile::tempdir().unwrap();
    let directory = Arc::new(SecureJournalDir::open(&temp.path().join("journal")).unwrap());
    work.journal = Some(JournalToken(
        temp.path().join("journal/owned.claim"),
        directory,
    ));
    assert_eq!(super::super::unresolved_cleanup_units(&work), 1);
}

#[test]
fn worker_spawn_runtime_and_post_handoff_panic_retain_ownership() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.blocking_lock();
    for fault in [
        super::super::CleanupWorkerFault::Spawn,
        super::super::CleanupWorkerFault::RuntimeBuild,
        super::super::CleanupWorkerFault::PanicAfterHandoff,
    ] {
        super::super::spawn_artifact_cleanup_retry_inner(work(), Some(fault));
        super::super::ARTIFACT_CLEANUP_WORKERS.drain();
        let mut unresolved = super::super::UNRESOLVED_ARTIFACT_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(
            unresolved
                .iter()
                .map(super::super::unresolved_cleanup_units)
                .sum::<usize>(),
            1
        );
        unresolved.clear();
    }
}

#[test]
fn drain_worker_real_panic_after_take_retains_the_handoff() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.blocking_lock();
    super::super::UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(work());

    assert_eq!(
        super::super::drain_unresolved_artifact_cleanups_inner(Some(
            super::super::CleanupWorkerFault::DrainPanicAfterHandoff,
        )),
        1
    );
    super::super::UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

#[tokio::test]
async fn retry_panic_after_first_durable_progress_owns_only_the_suffix() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.lock().await;
    let deletes = Arc::new(AtomicUsize::new(0));
    let pending = seeded_counting_work("retry_progress", 2, deletes.clone()).await;
    super::super::spawn_artifact_cleanup_retry_inner(
        pending,
        Some(super::super::CleanupWorkerFault::PanicAfterFirstProgress),
    );
    super::super::ARTIFACT_CLEANUP_WORKERS.drain();
    let mut unresolved = super::super::UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(deletes.load(Ordering::Acquire), 1);
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].artifacts.len(), 1);
    unresolved.clear();
}

#[tokio::test]
async fn drain_panic_on_second_entry_never_requeues_completed_first_entry() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.lock().await;
    let deletes = Arc::new(AtomicUsize::new(0));
    let first = seeded_counting_work("drain_first", 1, deletes.clone()).await;
    let second = seeded_counting_work("drain_second", 1, deletes.clone()).await;
    super::super::UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend([first, second]);
    assert_eq!(
        super::super::drain_unresolved_artifact_cleanups_inner(Some(
            super::super::CleanupWorkerFault::DrainPanicOnSecondEntry
        )),
        1
    );
    assert_eq!(deletes.load(Ordering::Acquire), 1);
    let mut unresolved = super::super::UNRESOLVED_ARTIFACT_CLEANUPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].source_id.0, "src_drain_second");
    unresolved.clear();
}

#[tokio::test]
async fn fresh_root_and_reconstructed_runtime_replay_exactly_once() {
    let _serial = CLEANUP_GLOBAL_TEST_LOCK.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("fresh-journal");
    let ledger_state = FakeLedgerStore::new();
    let now = Timestamp("2026-09-04T00:00:00Z".into());
    ledger_state
        .upsert_source(SourceSummary {
            source_id: SourceId::new("src_journal"),
            canonical_uri: "file:///journal".into(),
            display_name: "journal".into(),
            source_kind: SourceKind::Local,
            adapter: AdapterRef {
                name: "test".into(),
                version: "1".into(),
            },
            authority: AuthorityLevel::UserPinned,
            status: LifecycleStatus::Running,
            counts: SourceCounts {
                items_total: 0,
                items_changed: 0,
                documents_total: 0,
                chunks_total: 0,
                vector_points_total: 0,
                bytes_total: 0,
            },
            created_at: now.clone(),
            updated_at: now,
            tags: vec![],
            watch_id: None,
            graph_node_ids: vec![],
            last_job_id: None,
            last_refreshed_at: None,
            user_label: None,
        })
        .await
        .unwrap();
    let mut original = work();
    original.ledger = Arc::new(ledger_state.clone());
    let token = persist(&root, &original).await.unwrap();
    clear_process_local_state();

    let rebuilt_store = Arc::new(FakeCoreBoundaries::new());
    let deletes = Arc::new(AtomicUsize::new(0));
    let rebuilt_ledger = Arc::new(ledger_state.clone());
    let mut rebuilt = crate::context::TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        rebuilt_ledger.clone(),
        Arc::new(FakeEmbeddingProvider::new("fresh", 8)),
        Arc::new(FakeVectorStore::new("fresh")),
        ProviderId::new("fresh"),
        "fresh",
        8,
    );
    rebuilt.artifact_store = Arc::new(CountingStore {
        inner: rebuilt_store,
        deletes: deletes.clone(),
    });

    let summary = replay(&root, &rebuilt).await.unwrap();
    assert_eq!(summary.claimed, 1);
    super::super::ARTIFACT_CLEANUP_WORKERS.drain();
    assert_eq!(deletes.load(Ordering::Acquire), 1);
    assert!(!token.0.exists() && !token.0.with_extension("claim").exists());
    assert!(
        rebuilt_ledger
            .list_pending_cleanup_debt(original.source_id)
            .await
            .unwrap()
            .is_empty()
    );
}
