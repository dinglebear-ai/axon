use std::io::IsTerminal;

use futures_util::stream::{self, StreamExt};

use super::*;
use crate::commands::wait_progress::{BatchProgressSession, ProgressMode, batch_progress_channel};

pub(super) async fn run(
    cfg: &Config,
    service_context: &ServiceContext,
    inputs: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let concurrency = cfg.batch_concurrency.clamp(1, 512);
    let progress_mode = ProgressMode::for_config(cfg, std::io::stderr().is_terminal());
    let (batch_forwarder, batch_session) = if cfg.wait && progress_mode != ProgressMode::Silent {
        let (forwarder, updates) = batch_progress_channel();
        (
            Some(forwarder),
            Some(BatchProgressSession::new(cfg, inputs.len(), updates)),
        )
    } else {
        (None, None)
    };
    let outcomes_work = stream::iter(inputs.into_iter().enumerate())
        .map(|(index, input)| {
            let forwarder = batch_forwarder.clone();
            async move {
                let reported_input = input.clone();
                let result = match build_source_request(cfg, input) {
                    Ok(request) => {
                        execute_batch_source_request(
                            cfg,
                            service_context,
                            index,
                            request,
                            forwarder,
                        )
                        .await
                    }
                    Err(error) => {
                        if let Some(forwarder) = forwarder {
                            forwarder.failed_before_start(index, reported_input.clone());
                        }
                        Err(error)
                    }
                };
                result
                    .map(|result| (index, result))
                    .map_err(|error| (index, reported_input, error.to_string()))
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<Result<(usize, SourceResult), (usize, String, String)>>>();
    let outcomes = if let Some(mut session) = batch_session {
        session.run_until(outcomes_work).await
    } else {
        outcomes_work.await
    };
    ensure_detached_worker(cfg, &outcomes).await;
    render_outcomes(cfg, outcomes)
}

async fn ensure_detached_worker(
    cfg: &Config,
    outcomes: &[Result<(usize, SourceResult), (usize, String, String)>],
) {
    if should_detach(cfg)
        && outcomes.iter().any(|outcome| {
            outcome
                .as_ref()
                .is_ok_and(|(_, result)| result.job.is_some())
        })
    {
        // A sibling input can fail after other jobs were durably enqueued.
        // Start the worker before propagating that error so successful jobs do
        // not remain queued indefinitely.
        detach::ensure_worker_process(cfg).await;
    }
}

fn render_outcomes(
    cfg: &Config,
    outcomes: Vec<Result<(usize, SourceResult), (usize, String, String)>>,
) -> Result<(), Box<dyn Error>> {
    let (mut indexed_results, mut batch_errors) = partition_outcomes(outcomes);
    indexed_results.sort_by_key(|(index, _)| *index);
    batch_errors.sort_by_key(|(index, _, _)| *index);
    let semantic_failures = indexed_results
        .iter()
        .filter(|(_, result)| result.status == LifecycleStatus::Failed)
        .count();
    let failed = batch_errors.len() + semantic_failures;
    let succeeded = indexed_results.len() - semantic_failures;

    if cfg.json_output {
        render_json(cfg, &indexed_results, &batch_errors, succeeded, failed);
    } else {
        for (_, result) in &indexed_results {
            render_source_result(cfg, result);
        }
        for (_, input, error) in &batch_errors {
            eprintln!("{} {}: {}", muted("failed"), input, error);
        }
    }
    if failed > 0 {
        return Err(format!(
            "{} of {} source inputs failed",
            failed,
            indexed_results.len() + batch_errors.len()
        )
        .into());
    }
    Ok(())
}

type IndexedResult = (usize, SourceResult);
type BatchError = (usize, String, String);

fn partition_outcomes(
    outcomes: Vec<Result<IndexedResult, BatchError>>,
) -> (Vec<IndexedResult>, Vec<BatchError>) {
    let mut indexed_results = Vec::new();
    let mut batch_errors = Vec::new();
    for outcome in outcomes {
        match outcome {
            Ok(result) => indexed_results.push(result),
            Err(error) => batch_errors.push(error),
        }
    }
    (indexed_results, batch_errors)
}

fn render_json(
    cfg: &Config,
    indexed_results: &[IndexedResult],
    batch_errors: &[BatchError],
    succeeded: usize,
    failed: usize,
) {
    let mut rendered = indexed_results
        .iter()
        .map(|(index, result)| {
            if is_queued_descriptor(result) {
                (*index, queued_descriptor_json(result))
            } else {
                (*index, source_result_json(cfg, result))
            }
        })
        .collect::<Vec<_>>();
    rendered.extend(batch_errors.iter().map(|(index, input, error)| {
        (
            *index,
            serde_json::json!({
                "input": input,
                "status": "failed",
                "error": error,
            }),
        )
    }));
    rendered.sort_by_key(|(index, _)| *index);
    let rendered = rendered
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::json!({
            "count": rendered.len(),
            "succeeded": succeeded,
            "failed": failed,
            "results": rendered,
        })
    );
}
