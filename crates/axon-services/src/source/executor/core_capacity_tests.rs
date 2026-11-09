//! Explicit complete-corpus profiling; never reads a connected provider/index.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

fn disk_bytes(path: &std::path::Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| disk_bytes(&entry.path()))
        .sum()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit full-Core memory/disk qualification with fixed external fixture"]
async fn complete_core_pipeline_profile() {
    let fixture =
        std::env::var("AXON_CORE_FIXTURE_ROOT").expect("fixed full Core checkout required");
    let output = std::path::PathBuf::from(
        std::env::var("AXON_CORE_PROFILE_OUTPUT").expect("private report directory required"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let run = tempfile::Builder::new()
        .prefix("core-pipeline-")
        .tempdir_in(&output)
        .unwrap();
    let run_path = run.path().to_path_buf();
    let harness = crate::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let mut runtime = harness.ctx().target_local_source_runtime().unwrap().clone();
    runtime.generation_work_root = Some(run.path().join("generation-work"));
    runtime.artifact_candidate_outbox = Some(Arc::new(
        crate::artifact_candidate_outbox::ArtifactCandidateOutbox::new(run.path().join("outbox")),
    ));
    runtime.artifact_store = Arc::new(axon_core::boundary::FileArtifactStore::new(
        run.path().join("artifacts"),
    ));
    let database = harness.ctx().cfg().sqlite_path.clone();
    let database_root = database.parent().unwrap().to_path_buf();
    println!(
        "CORE_PROFILE_START run={} database={}",
        run_path.display(),
        database.display()
    );
    let done = Arc::new(AtomicBool::new(false));
    let peak_disk = Arc::new(AtomicU64::new(0));
    let monitor_done = done.clone();
    let monitor_peak = peak_disk.clone();
    let sampled_run = run_path.clone();
    let monitor = tokio::task::spawn_blocking(move || {
        while !monitor_done.load(Ordering::Acquire) {
            monitor_peak.fetch_max(
                disk_bytes(&sampled_run) + disk_bytes(&database_root),
                Ordering::AcqRel,
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    let request = SourceRequest::new("https://github.com/unraid/core");
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    let mut plan =
        crate::source::dispatch::family_source_plan(&request.source, &route, true, None, None);
    plan.route
        .validated_options
        .values
        .insert("repo_root".into(), serde_json::json!(fixture));
    assert!(plan.request.limits.max_items.is_none());
    let execution = crate::source::SourceExecutionContext::inline(request, None);
    let adapter = axon_adapters::git::GitSourceAdapter::new();
    let started = std::time::Instant::now();
    let result = crate::source::dispatch::dispatch_materialized(
        &runtime,
        &adapter,
        plan,
        "core-capacity",
        "core-capacity-profile",
        None,
        &execution,
        |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
    )
    .await;
    done.store(true, Ordering::Release);
    monitor.await.unwrap();
    if let Some(outbox) = &runtime.artifact_candidate_outbox {
        outbox.shutdown_drain().await;
    }
    let report = match &result {
        Ok(result) => {
            serde_json::json!({"status":"committed", "documents":result.documents_prepared,
            "chunks":result.chunks_prepared,"vectors":result.vector_points_written,"generation":result.generation.0})
        }
        Err(error) => serde_json::json!({"status":"failed", "error":format!("{error:#}")}),
    };
    let document_counts: (i64, i64) = sqlx::query_as("SELECT count(*),coalesce(sum(json_extract(status_json,'$.chunk_count')),0) FROM document_status")
        .fetch_one(harness.ctx().sqlite_pool().unwrap().as_ref()).await.unwrap();
    let report = serde_json::json!({"result":report,"durable_document_statuses":document_counts.0,"durable_chunk_count":document_counts.1,"elapsed_seconds":started.elapsed().as_secs_f64(),
        "sampled_peak_disk_bytes":peak_disk.load(Ordering::Acquire),"fixture_root":fixture,
        "all_items":true,"code_indexing":true,"graph_indexing":true,"embedding_dimensions":8,
        "providers":"deterministic fake embeddings/vector persistence; real preparation, graph and SQLite publication",
        "run_directory":run_path});
    let report_path = output.join(format!(
        "{}.json",
        run.path().file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("CORE_PIPELINE_PROFILE {}", report);
    let expected = std::env::var("AXON_CORE_PROFILE_EXPECT").unwrap_or_else(|_| "success".into());
    if expected == "guard" {
        let error = result.expect_err("unchanged admission must reproduce the capacity guard");
        assert!(
            format!("{error:#}").contains("256 MiB"),
            "unexpected failure: {error:#}"
        );
    } else {
        result.expect("full Core generation must commit before qualification");
    }
}
