use axon_jobs::store::RECLAIMED_ERROR_TEXT;
use axon_services::types::ServiceJob;
use serde_json::Value;

pub(crate) fn source_progress_summary(job: &ServiceJob) -> Option<String> {
    if !matches!(job.status.as_str(), "pending" | "running" | "completed") {
        return None;
    }
    let metrics = if job.status == "running" {
        live_progress_metrics(job)
    } else {
        job.result_json.as_ref()
    };
    match job.status.as_str() {
        "pending" => reclaimed_suffix(job)
            .strip_prefix(" · ")
            .map(ToOwned::to_owned),
        "running" => source_running_progress(job, metrics),
        "completed" => source_completed_progress(metrics),
        _ => None,
    }
}

fn source_running_progress(job: &ServiceJob, metrics: Option<&Value>) -> Option<String> {
    let Some(metrics) = metrics else {
        return Some("starting...".to_string());
    };
    if has_any(metrics, &["pages_crawled", "md_created", "error_pages"]) {
        return page_source_running_progress(job, metrics);
    }
    if has_any(metrics, &["docs_embedded", "docs_completed", "docs_total"]) {
        return document_source_progress(job.status.as_str(), Some(metrics));
    }
    provider_source_progress(job.status.as_str(), Some(metrics), true)
}

fn source_completed_progress(metrics: Option<&Value>) -> Option<String> {
    let metrics = metrics?;
    if has_any(metrics, &["md_created", "elapsed_ms", "pages_crawled"]) {
        return page_source_completed_progress(metrics);
    }
    if has_any(metrics, &["docs_embedded", "docs_completed", "docs_total"]) {
        return document_source_progress("completed", Some(metrics));
    }
    provider_source_progress("completed", Some(metrics), true)
}

fn has_any(metrics: &Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| metrics.get(*key).is_some())
}

fn document_source_progress(status: &str, metrics: Option<&Value>) -> Option<String> {
    let Some(metrics) = metrics else {
        return if status == "running" {
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
    let docs_total = metrics.get("docs_total").and_then(|v| v.as_u64());
    if docs == 0 && chunks == 0 {
        if status != "running" {
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

pub(crate) fn ingest_progress(result_json: &Option<Value>) -> Option<String> {
    provider_source_progress("running", result_json.as_ref(), false)
}

fn page_source_running_progress(job: &ServiceJob, metrics: &Value) -> Option<String> {
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

fn page_source_completed_progress(metrics: &Value) -> Option<String> {
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
    Some(summary)
}

fn provider_source_progress(
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
            Some("indexing…".to_string())
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
