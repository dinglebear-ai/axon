use super::*;
use crate::crates::core::content::ExtractRun;
use crate::crates::jobs::common::spawn_heartbeat_task;
use crate::crates::jobs::worker_lane::{
    ProcessFn, WorkerConfig, resolve_lane_count, run_job_worker,
};
use tokio::time::Duration;

const EXTRACT_HEARTBEAT_INTERVAL_SECS: u64 = 30;

struct ExtractAggregation {
    runs: Vec<serde_json::Value>,
    all_results: Vec<serde_json::Value>,
    pages_visited: usize,
    pages_with_data: usize,
    deterministic_pages: usize,
    llm_fallback_pages: usize,
    llm_requests: usize,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
    parser_hits: serde_json::Map<String, serde_json::Value>,
}

impl ExtractAggregation {
    fn new() -> Self {
        Self {
            runs: Vec::new(),
            all_results: Vec::new(),
            pages_visited: 0,
            pages_with_data: 0,
            deterministic_pages: 0,
            llm_fallback_pages: 0,
            llm_requests: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            estimated_cost_usd: 0.0,
            parser_hits: serde_json::Map::new(),
        }
    }
}

async fn load_extract_job_inputs(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(Vec<String>, ExtractJobConfig)>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, (serde_json::Value, serde_json::Value)>(
        "SELECT urls_json, config_json FROM axon_extract_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((urls_json, cfg_json)) = row else {
        return Ok(None);
    };
    let job_cfg: ExtractJobConfig = serde_json::from_value(cfg_json)?;
    let urls: Vec<String> = serde_json::from_value(urls_json)?;
    Ok(Some((urls, job_cfg)))
}

async fn mark_extract_canceled(
    redis_conn: &mut redis::aio::MultiplexedConnection,
    pool: &PgPool,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    let cancel_key = format!("axon:extract:cancel:{id}");
    let cancel_before: Option<String> = redis_conn
        .get(&cancel_key)
        .await
        .map_err(|e| format!("failed to check extract cancellation key {cancel_key}: {e}"))?;
    if cancel_before.is_none() {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE axon_extract_jobs SET status=$2,updated_at=NOW(),finished_at=NOW() WHERE id=$1",
    )
    .bind(id)
    .bind(JobStatus::Canceled.as_str())
    .execute(pool)
    .await?;
    Ok(true)
}

fn update_parser_hits(map: &mut serde_json::Map<String, serde_json::Value>, run: &ExtractRun) {
    for (name, count) in &run.parser_hits {
        let current = map.get(name.as_str()).and_then(|v| v.as_u64()).unwrap_or(0);
        map.insert(name.clone(), serde_json::json!(current + *count as u64));
    }
}

fn append_extract_success(agg: &mut ExtractAggregation, run: ExtractRun) {
    agg.pages_visited += run.pages_visited;
    agg.pages_with_data += run.pages_with_data;
    agg.deterministic_pages += run.metrics.deterministic_pages;
    agg.llm_fallback_pages += run.metrics.llm_fallback_pages;
    agg.llm_requests += run.metrics.llm_requests;
    agg.prompt_tokens += run.metrics.prompt_tokens;
    agg.completion_tokens += run.metrics.completion_tokens;
    agg.total_tokens += run.metrics.total_tokens;
    agg.estimated_cost_usd += run.metrics.estimated_cost_usd;
    update_parser_hits(&mut agg.parser_hits, &run);
    agg.all_results.extend(run.results.clone());
    agg.runs.push(serde_json::json!({
        "url": run.start_url,
        "pages_visited": run.pages_visited,
        "pages_with_data": run.pages_with_data,
        "deterministic_pages": run.metrics.deterministic_pages,
        "llm_fallback_pages": run.metrics.llm_fallback_pages,
        "llm_requests": run.metrics.llm_requests,
        "prompt_tokens": run.metrics.prompt_tokens,
        "completion_tokens": run.metrics.completion_tokens,
        "total_tokens": run.metrics.total_tokens,
        "estimated_cost_usd": run.metrics.estimated_cost_usd,
        "parser_hits": run.parser_hits,
        "total_items": run.results.len(),
        "results": run.results
    }));
}

fn append_extract_error(agg: &mut ExtractAggregation, url: String, err: String) {
    agg.runs.push(serde_json::json!({
        "url": url,
        "error": err,
        "pages_visited": 0,
        "pages_with_data": 0,
        "total_items": 0,
        "results": []
    }));
}

async fn execute_extract_runs(
    cfg: &Config,
    urls: Vec<String>,
    prompt: String,
    max_pages: u32,
) -> ExtractAggregation {
    let engine = Arc::new(DeterministicExtractionEngine::with_default_parsers());

    let custom_headers = cfg.custom_headers.clone();

    let results: Vec<_> = futures_util::stream::iter(urls)
        .map(|url| {
            let engine = Arc::clone(&engine);
            let wcfg = ExtractWebConfig {
                start_url: url.clone(),
                prompt: prompt.clone(),
                limit: max_pages,
                openai_base_url: cfg.openai_base_url.clone(),
                openai_api_key: cfg.openai_api_key.clone(),
                openai_model: cfg.openai_model.clone(),
                custom_headers: custom_headers.clone(),
            };
            async move {
                log_debug(&format!("extract llm_call url={url}"));
                let run = run_extract_with_engine(wcfg, engine).await;
                (url, run)
            }
        })
        .buffer_unordered(16)
        .collect()
        .await;

    let mut agg = ExtractAggregation::new();
    for (url, run_result) in results {
        match run_result {
            Ok(run) => {
                log_debug(&format!(
                    "extract llm_ok url={} fields_extracted={}",
                    run.start_url,
                    run.results.len()
                ));
                append_extract_success(&mut agg, run);
            }
            Err(err) => {
                log_warn(&format!("extract llm_failed url={url} error={err}"));
                append_extract_error(&mut agg, url, err.to_string());
            }
        }
    }
    agg
}

fn extract_result_json(
    prompt: String,
    model: String,
    agg: ExtractAggregation,
) -> serde_json::Value {
    serde_json::json!({
        "prompt": prompt,
        "model": model,
        "pages_visited": agg.pages_visited,
        "pages_with_data": agg.pages_with_data,
        "deterministic_pages": agg.deterministic_pages,
        "llm_fallback_pages": agg.llm_fallback_pages,
        "llm_requests": agg.llm_requests,
        "prompt_tokens": agg.prompt_tokens,
        "completion_tokens": agg.completion_tokens,
        "total_tokens": agg.total_tokens,
        "estimated_cost_usd": agg.estimated_cost_usd,
        "parser_hits": agg.parser_hits,
        "total_items": agg.all_results.len(),
        "runs": agg.runs,
        "results": agg.all_results,
    })
}

async fn process_extract_job(cfg: &Config, pool: &PgPool, id: Uuid) -> Result<(), Box<dyn Error>> {
    let job_start = std::time::Instant::now();
    let run_result = async {
        let Some((urls, job_cfg)) = load_extract_job_inputs(pool, id).await? else {
            return Ok::<Option<serde_json::Value>, Box<dyn Error>>(None);
        };
        let redis_client = redis::Client::open(cfg.redis_url.clone())?;
        let mut redis_conn = tokio::time::timeout(
            Duration::from_secs(3),
            redis_client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| -> Box<dyn Error> { "redis connect timeout (3s) in extract worker".into() })?
        .map_err(|e| -> Box<dyn Error> {
            format!("redis connect failed in extract worker: {e}").into()
        })?;
        if mark_extract_canceled(&mut redis_conn, pool, id).await? {
            return Ok(None);
        }

        log_info(&format!(
            "extract start job_id={id} url_count={}",
            urls.len()
        ));

        let prompt = job_cfg
            .prompt
            .ok_or("extract prompt is required; pass --query")?;
        let (heartbeat_stop_tx, heartbeat_task) =
            spawn_heartbeat_task(pool.clone(), TABLE, id, EXTRACT_HEARTBEAT_INTERVAL_SECS);
        let agg = execute_extract_runs(cfg, urls, prompt.clone(), job_cfg.max_pages).await;
        let _ = heartbeat_stop_tx.send(true);
        if let Err(err) = heartbeat_task.await {
            log_warn(&format!(
                "extract heartbeat_task panicked for job {id}: {err:?}"
            ));
        }
        Ok(Some(extract_result_json(
            prompt,
            cfg.openai_model.clone(),
            agg,
        )))
    }
    .await;
    let run_result = run_result.map_err(|e| e.to_string());

    match run_result {
        Ok(Some(result_json)) => {
            // Retry the completion UPDATE to guard against transient DB errors
            // (e.g. PG restart mid-job). A lost UPDATE would leave the job stuck
            // in 'running' until the watchdog reclaims it.
            let mut last_err = None;
            for attempt in 1u32..=3 {
                match sqlx::query(
                    "UPDATE axon_extract_jobs SET status=$2,updated_at=NOW(),finished_at=NOW(),result_json=$3,error_text=NULL WHERE id=$1 AND status=$4",
                )
                .bind(id)
                .bind(JobStatus::Completed.as_str())
                .bind(&result_json)
                .bind(JobStatus::Running.as_str())
                .execute(pool)
                .await
                {
                    Ok(done) => {
                        if done.rows_affected() == 0 {
                            log_warn(&format!(
                                "worker extract job {id} completion update skipped: job no longer running (likely reclaimed/canceled)"
                            ));
                        }
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        log_warn(&format!(
                            "worker extract job {id} completion UPDATE attempt {attempt} failed: {e}"
                        ));
                        last_err = Some(e);
                        if attempt < 3 {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
            if let Some(e) = last_err {
                return Err(e.into());
            }
            let urls_count = result_json["runs"].as_array().map(|a| a.len()).unwrap_or(0);
            log_done(&format!(
                "worker completed extract job {id} urls_count={urls_count} duration_ms={}",
                job_start.elapsed().as_millis()
            ));
        }
        Ok(None) => {}
        Err(error_text) => {
            return Err(error_text.into());
        }
    }

    Ok(())
}

async fn process_claimed_extract_job(cfg: Config, pool: PgPool, id: Uuid) {
    let fail_msg = match process_extract_job(&cfg, &pool, id).await {
        Ok(()) => None,
        Err(err) => Some(err.to_string()),
    };
    if let Some(error_text) = fail_msg {
        let _ = mark_job_failed(&pool, TABLE, id, &error_text).await;
        log_warn(&format!("worker failed extract job {id}: {error_text}"));
    }
}

pub async fn run_extract_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Validate required environment variables before attempting any connections.
    if let Err(msg) = crate::crates::jobs::worker_lane::validate_worker_env_vars() {
        return Err(msg.into());
    }

    log_info(&format!(
        "worker_start worker=extract queue={}",
        cfg.extract_queue
    ));

    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let wc = WorkerConfig {
        table: TABLE,
        queue_name: cfg.extract_queue.clone(),
        job_kind: "extract",
        consumer_tag_prefix: "axon-rust-extract-worker",
        lane_count: resolve_lane_count("AXON_EXTRACT_LANES", 1, 8),
    };

    let process_fn: ProcessFn =
        Arc::new(|cfg, pool, id| Box::pin(process_claimed_extract_job(cfg, pool, id)));

    run_job_worker(cfg, pool, &wc, process_fn).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_run(
        url: &str,
        pages_visited: usize,
        pages_with_data: usize,
        parser_hits: HashMap<String, usize>,
    ) -> ExtractRun {
        ExtractRun {
            start_url: url.to_string(),
            pages_visited,
            pages_with_data,
            results: vec![],
            metrics: Default::default(),
            parser_hits,
        }
    }

    #[test]
    fn append_extract_error_records_url_and_error() {
        let mut agg = ExtractAggregation::new();
        append_extract_error(
            &mut agg,
            "https://example.com".to_string(),
            "timeout".to_string(),
        );
        assert_eq!(agg.runs.len(), 1);
        let entry = &agg.runs[0];
        assert_eq!(
            entry.get("url").and_then(|v| v.as_str()),
            Some("https://example.com")
        );
        assert_eq!(entry.get("error").and_then(|v| v.as_str()), Some("timeout"));
        assert_eq!(entry.get("pages_visited").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(entry.get("total_items").and_then(|v| v.as_u64()), Some(0));
    }

    #[test]
    fn update_parser_hits_accumulates_counts() {
        let mut map = serde_json::Map::new();
        // First run: json=3, table=1
        let run1 = make_run(
            "u1",
            1,
            1,
            [("json".to_string(), 3), ("table".to_string(), 1)].into(),
        );
        update_parser_hits(&mut map, &run1);
        assert_eq!(map.get("json").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(map.get("table").and_then(|v| v.as_u64()), Some(1));
        // Second run: adds json=2; table is unchanged
        let run2 = make_run("u2", 1, 1, [("json".to_string(), 2)].into());
        update_parser_hits(&mut map, &run2);
        assert_eq!(map.get("json").and_then(|v| v.as_u64()), Some(5));
        assert_eq!(map.get("table").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn append_extract_success_aggregates_page_counts() {
        let mut agg = ExtractAggregation::new();
        let run = make_run("https://example.com", 5, 3, HashMap::new());
        append_extract_success(&mut agg, run);
        assert_eq!(agg.pages_visited, 5);
        assert_eq!(agg.pages_with_data, 3);
        assert_eq!(agg.runs.len(), 1);
        let entry = &agg.runs[0];
        assert_eq!(
            entry.get("url").and_then(|v| v.as_str()),
            Some("https://example.com")
        );
        assert_eq!(entry.get("pages_visited").and_then(|v| v.as_u64()), Some(5));
    }

    #[test]
    fn extract_result_json_has_expected_shape() {
        let agg = ExtractAggregation::new();
        let result = extract_result_json("Find prices".to_string(), "gpt-4o".to_string(), agg);
        assert_eq!(
            result.get("prompt").and_then(|v| v.as_str()),
            Some("Find prices")
        );
        assert_eq!(result.get("model").and_then(|v| v.as_str()), Some("gpt-4o"));
        assert_eq!(
            result.get("pages_visited").and_then(|v| v.as_u64()),
            Some(0)
        );
        assert_eq!(result.get("total_items").and_then(|v| v.as_u64()), Some(0));
        assert!(result.get("results").and_then(|v| v.as_array()).is_some());
        assert!(result.get("runs").and_then(|v| v.as_array()).is_some());
    }
}
