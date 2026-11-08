use std::fs;
use std::io::Write as _;

use super::*;

#[test]
fn spool_is_private_ordered_and_deduplicated() {
    let directory = private_tempdir().unwrap();
    let path = directory.path().join("generation.jsonl");
    let mut spool = GenerationSpool::create(&path).unwrap();
    assert!(spool.append("a", &vec![1_u8, 2]).unwrap());
    assert!(!spool.append("a", &vec![9_u8]).unwrap());
    assert!(spool.append("b", &vec![3_u8]).unwrap());
    let mut replayed = Vec::new();
    spool
        .replay_each::<Vec<u8>>(|key, value| {
            replayed.push((key, value));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        replayed,
        vec![("a".to_string(), vec![1, 2]), ("b".to_string(), vec![3])]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn replay_stops_on_corruption_instead_of_silently_dropping_records() {
    let directory = private_tempdir().unwrap();
    let path = directory.path().join("corrupt.jsonl");
    let mut spool = GenerationSpool::create(&path).unwrap();
    assert!(spool.append("good", &vec![1_u8]).unwrap());
    let invalid = b"{not-json}";
    use sha2::Digest as _;
    spool
        .store
        .db
        .execute(
            "UPDATE spool_records SET payload=?,digest=? WHERE record_key='good'",
            rusqlite::params![invalid, hex::encode(sha2::Sha256::digest(invalid))],
        )
        .unwrap();

    let mut replayed = Vec::new();
    let error = spool
        .replay_each::<Vec<u8>>(|key, value| {
            replayed.push((key, value));
            Ok(())
        })
        .expect_err("corrupt replay must fail closed");

    assert!(replayed.is_empty());
    assert!(error.to_string().contains("expected"));
}

#[test]
fn bounded_encoding_enforces_exact_serialized_byte_window() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    let oversized = "x".repeat(128);
    assert!(
        codec::encode(&oversized, 64)
            .unwrap_err()
            .to_string()
            .contains("byte window")
    );
    let encoded = codec::encode(&"x".repeat(62), 64).unwrap();
    assert_eq!(encoded.len(), 64);
    assert!(encoded.capacity() <= 64);
    bytes.write_all(&encoded).unwrap();
    assert_eq!(bytes.into_inner().len(), 64);
}

#[test]
fn replay_propagates_visitor_failure_without_delivering_later_records() {
    let directory = private_tempdir().unwrap();
    let mut spool = GenerationSpool::create(&directory.path().join("visitor.jsonl")).unwrap();
    spool.append("a", &1).unwrap();
    spool.append("b", &2).unwrap();
    let mut visited = 0;
    let error = spool
        .replay_each::<u8>(|_, _| {
            visited += 1;
            anyhow::bail!("cancelled visitor")
        })
        .unwrap_err();
    assert_eq!(visited, 1);
    assert!(error.to_string().contains("cancelled visitor"));
}

#[test]
fn escaped_json_size_is_bounded_and_replay_can_restart_after_visitor_failure() {
    assert!(codec::encode(&"\n".repeat(40), 64).is_err());
    let directory = private_tempdir().unwrap();
    let mut spool = GenerationSpool::create(&directory.path().join("restart.jsonl")).unwrap();
    spool.append("a", &1_u8).unwrap();
    spool.append("b", &2_u8).unwrap();
    assert!(
        spool
            .replay_each::<u8>(|_, _| anyhow::bail!("cancelled"))
            .is_err()
    );
    let mut replayed = Vec::new();
    spool
        .replay_each::<u8>(|key, value| {
            replayed.push((key, value));
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed, vec![("a".into(), 1), ("b".into(), 2)]);
}

#[test]
fn codec_matches_json_and_enforces_capacity_across_small_non_power_of_two_limits() {
    for length in 0..128 {
        let value = "\nλ".repeat(length);
        let reference = serde_json::to_vec(&value).unwrap();
        for limit in [0, 1, 7, 31, 64, 127, 257, 1024] {
            match codec::encode(&value, limit) {
                Ok(encoded) => {
                    assert!(reference.len() <= limit);
                    assert_eq!(encoded, reference);
                    assert!(encoded.capacity() <= limit);
                }
                Err(_) => assert!(reference.len() > limit),
            }
        }
    }
}

#[test]
fn production_spool_scope_fences_source_job_generation_and_attempt() {
    let job = JobId::new(uuid::Uuid::from_u128(1));
    let source = SourceId::new("source-a");
    let generation = SourceGenerationId::new("g");
    let mut spool = GenerationSpool::temporary_for(&source, &generation, job, 7).unwrap();
    spool.append("record", &1_u8).unwrap();
    let directory = spool._directory.take().unwrap();
    let path = directory.path().join("g.sqlite");
    drop(spool);
    for owner in [
        serde_json::to_string(&(SourceId::new("other"), &generation, job, 7)).unwrap(),
        serde_json::to_string(&(&source, SourceGenerationId::new("other"), job, 7)).unwrap(),
        serde_json::to_string(&(
            &source,
            &generation,
            JobId::new(uuid::Uuid::from_u128(2)),
            7,
        ))
        .unwrap(),
        serde_json::to_string(&(&source, &generation, job, 8)).unwrap(),
    ] {
        assert!(storage::RecordStore::open(&path, &owner).is_err());
    }
    let owner = serde_json::to_string(&(&source, &generation, job, 7)).unwrap();
    assert!(
        storage::RecordStore::open(&path, &owner)
            .unwrap()
            .contains("record")
            .unwrap()
    );
}

fn streamed_record() -> SideEffectsSpoolRecord {
    SideEffectsSpoolRecord {
        skipped_statuses: Vec::new(),
        archive_items: Vec::new(),
        artifact_candidates: Vec::new(),
        warnings: Vec::new(),
        reused_item_keys: Vec::new(),
        refreshed_manifest_items: Vec::new(),
    }
}

#[tokio::test]
async fn streamed_replay_applies_backpressure_and_retains_the_sealed_owner() {
    let mut spool = GenerationSpool::temporary("stream-test").unwrap();
    for index in 0..40 {
        spool
            .append(&format!("record-{index}"), &streamed_record())
            .unwrap();
    }
    let (mut records, worker) = spool.replay_stream();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while records.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(records.len(), 1);
    assert!(
        !worker.is_finished(),
        "producer must wait for receiver admission"
    );
    let mut received = 0;
    while records.recv().await.is_some() {
        received += 1;
    }
    assert_eq!(received, 40);
    let mut spool = worker.await.unwrap().unwrap();
    assert!(spool.append("late", &streamed_record()).is_err());
    spool.seal().unwrap();
}

#[tokio::test]
async fn cancelled_stream_receiver_releases_the_blocking_worker() {
    let mut spool = GenerationSpool::temporary("cancel-stream-test").unwrap();
    for index in 0..40 {
        spool
            .append(&format!("record-{index}"), &streamed_record())
            .unwrap();
    }
    let (mut records, worker) = spool.replay_stream();
    assert!(records.recv().await.is_some());
    drop(records);
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), worker)
        .await
        .expect("cancelled producer must terminate")
        .unwrap();
    assert!(result.is_err());
}
