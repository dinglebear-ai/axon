use axon_jobs::store::RECLAIMED_ERROR_TEXT;
use axon_services::types::ServiceJob;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn crawl_progress_summary(
    job: &ServiceJob,
    embed_jobs_by_id: &HashMap<String, &ServiceJob>,
    embed_doc_totals: &HashMap<String, u64>,
) -> Option<String> {
    match job.status.as_str() {
        "running" => {
            let Some(metrics) = live_progress_metrics(job) else {
                return Some("starting…".to_string());
            };
            crawl_running_progress(job, metrics)
        }
        "completed" => {
            let metrics = job.result_json.as_ref()?;
            crawl_completed_progress(metrics, embed_jobs_by_id, embed_doc_totals)
        }
        "pending" => reclaimed_suffix(job)
            .strip_prefix(" · ")
            .map(ToOwned::to_owned),
        _ => None,
    }
}

pub(crate) fn crawl_list_progress_summary(job: &ServiceJob) -> Option<String> {
    let empty_embed_jobs = HashMap::new();
    let empty_embed_totals = HashMap::new();
    crawl_progress_summary(job, &empty_embed_jobs, &empty_embed_totals)
}

pub(crate) fn embed_progress_summary(
    job: &ServiceJob,
    fallback_docs_total: Option<u64>,
) -> Option<String> {
    if !matches!(job.status.as_str(), "running" | "completed") {
        return None;
    }
    let metrics = if job.status == "running" {
        live_progress_metrics(job)
    } else {
        job.result_json.as_ref()
    };
    let Some(metrics) = metrics else {
        return if job.status == "running" {
            Some("starting…".to_string())
        } else {
            None
        };
    };
    let docs = metrics
        .get("docs_embedded")
        .or_else(|| metrics.get("docs_completed"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let chunks = metrics
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let docs_total = metrics
        .get("docs_total")
        .and_then(|v| v.as_u64())
        .or(fallback_docs_total);
    if docs == 0 && chunks == 0 {
        if job.status != "running" {
            return None;
        }
        return if let Some(total) = docs_total.filter(|t| *t > 0) {
            Some(format!("0/{total} docs · initializing"))
        } else {
            Some("initializing".to_string())
        };
    }
    if let Some(total) = docs_total.filter(|total| *total > 0) {
        let percent = ((docs as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
        let percent_text = if percent < 99.95 {
            format!("{percent:.1}%")
        } else {
            "100%".to_string()
        };
        return Some(format!(
            "{docs}/{total} docs · {percent_text} · {chunks} chunks"
        ));
    }
    if docs > 0 {
        Some(format!("{docs} docs · {chunks} chunks"))
    } else {
        Some(format!("{chunks} chunks"))
    }
}

pub(crate) fn extract_progress_summary(job: &ServiceJob) -> Option<String> {
    if !matches!(job.status.as_str(), "running" | "completed") {
        return None;
    }
    let metrics = if job.status == "running" {
        live_progress_metrics(job)
    } else {
        job.result_json.as_ref()
    };
    let Some(metrics) = metrics else {
        return if job.status == "running" {
            Some("starting…".to_string())
        } else {
            None
        };
    };
    let items = metrics
        .get("total_items")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if items == 0 {
        return if job.status == "running" {
            Some("extracting…".to_string())
        } else {
            None
        };
    }
    Some(format!("{items} items"))
}

pub(crate) fn ingest_progress_summary(job: &ServiceJob) -> Option<String> {
    let metrics = if job.status == "running" {
        live_progress_metrics(job)
    } else {
        job.result_json.as_ref()
    };
    format_ingest_progress(job.status.as_str(), metrics, true)
}

pub(crate) fn ingest_progress(result_json: &Option<Value>) -> Option<String> {
    format_ingest_progress("running", result_json.as_ref(), false)
}

fn crawl_running_progress(job: &ServiceJob, metrics: &Value) -> Option<String> {
    let crawled = metrics
        .get("pages_crawled")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let docs = metrics
        .get("md_created")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if crawled == 0 && docs == 0 {
        return Some("crawling…".to_string());
    }
    let errors = metrics
        .get("error_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let error_suffix = if errors > 0 {
        format!(" · {errors} errors")
    } else {
        String::new()
    };
    let reclaim = reclaimed_suffix(job);
    if docs > 0 {
        Some(format!(
            "{crawled} crawled · {docs} docs{error_suffix}{reclaim}"
        ))
    } else {
        Some(format!("{crawled} crawled{error_suffix}{reclaim}"))
    }
}

fn live_progress_metrics(job: &ServiceJob) -> Option<&Value> {
    job.progress_json.as_ref().or(job.result_json.as_ref())
}

fn crawl_completed_progress(
    metrics: &Value,
    embed_jobs_by_id: &HashMap<String, &ServiceJob>,
    embed_doc_totals: &HashMap<String, u64>,
) -> Option<String> {
    let docs = metrics
        .get("md_created")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let elapsed_ms = metrics
        .get("elapsed_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let time = if elapsed_ms >= 1000 {
        format!("{:.1}s", elapsed_ms as f64 / 1000.0)
    } else {
        format!("{elapsed_ms}ms")
    };
    let mut summary = format!("{docs} docs · {time}");
    if metrics
        .get("coverage_status")
        .and_then(|v| v.as_str())
        .is_some_and(|status| status == "partial")
    {
        if let Some(reason) = metrics.get("coverage_reason").and_then(|v| v.as_str()) {
            summary.push_str(&format!(" · partial ({reason})"));
        } else {
            summary.push_str(" · partial");
        }
    }
    if let Some(embed_id) = metrics.get("embed_job_id").and_then(|v| v.as_str()) {
        if let Some(embed_job) = embed_jobs_by_id.get(embed_id) {
            summary.push_str(&format!(" · embed {}", embed_job.status));
            if let Some(embed_progress) =
                embed_progress_summary(embed_job, embed_doc_totals.get(embed_id).copied())
            {
                summary.push_str(&format!(" ({embed_progress})"));
            }
        } else {
            summary.push_str(&format!(" · embed queued {embed_id}"));
        }
    }
    Some(summary)
}

fn format_ingest_progress(
    status: &str,
    result_json: Option<&Value>,
    include_running_fallback: bool,
) -> Option<String> {
    if !matches!(status, "running" | "completed") {
        return None;
    }
    let Some(metrics) = result_json else {
        return if status == "running" && include_running_fallback {
            Some("starting…".to_string())
        } else {
            None
        };
    };
    let chunks = metrics
        .get("chunks")
        .or_else(|| metrics.get("chunks_embedded"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if let (Some(done), Some(total)) = (
        metrics.get("videos_done").and_then(|v| v.as_u64()),
        metrics.get("videos_total").and_then(|v| v.as_u64()),
    ) {
        return Some(format!("{done} / {total} videos, {chunks} chunks embedded"));
    }
    if let (Some(done), Some(total)) = (
        metrics.get("files_done").and_then(|v| v.as_u64()),
        metrics.get("files_total").and_then(|v| v.as_u64()),
    ) {
        return Some(format!("{done} / {total} files, {chunks} chunks embedded"));
    }
    if let (Some(done), Some(total)) = (
        metrics.get("tasks_done").and_then(|v| v.as_u64()),
        metrics.get("tasks_total").and_then(|v| v.as_u64()),
    ) {
        let phase = metrics
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("working");
        if chunks == 0 {
            return Some(format!("{phase} ({done} / {total} tasks)"));
        }
        return Some(format!(
            "{phase} ({done} / {total} tasks), {chunks} chunks embedded"
        ));
    }
    if chunks == 0 {
        return if status == "running" && include_running_fallback {
            Some("ingesting…".to_string())
        } else {
            None
        };
    }
    Some(format!("{chunks} chunks embedded"))
}

fn reclaimed_suffix(job: &ServiceJob) -> String {
    match job.error_text.as_deref().map(str::trim_start) {
        Some(RECLAIMED_ERROR_TEXT) => " · reclaimed retry".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn job(status: &str, result_json: Option<Value>) -> ServiceJob {
        ServiceJob {
            id: Uuid::parse_str("99999999-9999-9999-9999-999999999999").unwrap(),
            status: status.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            finished_at: None,
            error_text: None,
            url: Some("https://example.com".to_string()),
            source_type: Some("github".to_string()),
            target: Some("example/repo".to_string()),
            urls_json: None,
            progress_json: None,
            result_json,
            config_json: None,
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        }
    }

    #[test]
    fn embed_progress_cases_are_table_driven() {
        let cases = [
            ("running", None, None, Some("starting…")),
            (
                "running",
                Some(json!({"docs_total": 10, "docs_embedded": 0, "chunks_embedded": 0})),
                None,
                Some("0/10 docs · initializing"),
            ),
            (
                "running",
                Some(json!({"docs_total": 100, "docs_embedded": 25, "chunks_embedded": 75})),
                None,
                Some("25/100 docs · 25.0% · 75 chunks"),
            ),
            ("failed", None, None, None),
        ];

        for (status, result_json, fallback_total, expected) in cases {
            let actual = embed_progress_summary(&job(status, result_json), fallback_total);
            assert_eq!(actual.as_deref(), expected);
        }
    }

    #[test]
    fn crawl_progress_cases_are_table_driven() {
        let empty_jobs = HashMap::new();
        let empty_totals = HashMap::new();
        let cases = [
            ("running", None, Some("starting…")),
            (
                "running",
                Some(json!({"pages_crawled": 0, "md_created": 0})),
                Some("crawling…"),
            ),
            (
                "running",
                Some(json!({"pages_crawled": 12, "md_created": 10, "error_pages": 1})),
                Some("12 crawled · 10 docs · 1 errors"),
            ),
            ("pending", None, None),
        ];

        for (status, result_json, expected) in cases {
            let actual =
                crawl_progress_summary(&job(status, result_json), &empty_jobs, &empty_totals);
            assert_eq!(actual.as_deref(), expected);
        }
    }

    #[test]
    fn extract_progress_cases_are_table_driven() {
        let cases = [
            ("running", None, Some("starting…")),
            (
                "running",
                Some(json!({"total_items": 0})),
                Some("extracting…"),
            ),
            (
                "completed",
                Some(json!({"total_items": 7})),
                Some("7 items"),
            ),
            ("pending", None, None),
        ];

        for (status, result_json, expected) in cases {
            let actual = extract_progress_summary(&job(status, result_json));
            assert_eq!(actual.as_deref(), expected);
        }
    }

    #[test]
    fn ingest_progress_cases_are_table_driven() {
        let cases = [
            ("running", None, Some("starting…")),
            (
                "running",
                Some(json!({"files_done": 4, "files_total": 10, "chunks_embedded": 12})),
                Some("4 / 10 files, 12 chunks embedded"),
            ),
            (
                "running",
                Some(json!({"tasks_done": 2, "tasks_total": 5, "phase": "fetching_issues"})),
                Some("fetching_issues (2 / 5 tasks)"),
            ),
            (
                "completed",
                Some(json!({"chunks_embedded": 42})),
                Some("42 chunks embedded"),
            ),
            ("pending", None, None),
        ];

        for (status, result_json, expected) in cases {
            let actual = ingest_progress_summary(&job(status, result_json));
            assert_eq!(actual.as_deref(), expected);
        }
    }
}
