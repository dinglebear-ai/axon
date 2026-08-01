use axon_api::source::{PipelinePhase, SourceKind};
use axon_jobs::store::RECLAIMED_ERROR_TEXT;
use axon_services::types::ServiceJob;
use serde_json::Value;

pub(crate) fn source_progress_summary(job: &ServiceJob) -> Option<String> {
    if !matches!(
        job.status.as_str(),
        "pending" | "running" | "completed" | "completed_degraded"
    ) {
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
        "completed" | "completed_degraded" => source_completed_progress(metrics),
        _ => None,
    }
}

fn source_running_progress(job: &ServiceJob, metrics: Option<&Value>) -> Option<String> {
    let Some(metrics) = metrics else {
        return Some(phase_fallback(job.phase));
    };
    if has_any(metrics, &["pages_crawled", "md_created", "error_pages"]) {
        return page_source_running_progress(job, metrics);
    }
    if has_unified_counts(metrics) {
        return phase_progress(job, metrics).or_else(|| Some(phase_fallback(job.phase)));
    }
    provider_source_progress(job.status.as_str(), Some(metrics), false)
        .or_else(|| Some(phase_fallback(job.phase)))
}

fn source_completed_progress(metrics: Option<&Value>) -> Option<String> {
    let metrics = metrics?;
    if has_any(metrics, &["md_created", "elapsed_ms", "pages_crawled"]) {
        return page_source_completed_progress(metrics);
    }
    if has_unified_counts(metrics) {
        return document_source_progress("completed", Some(metrics));
    }
    provider_source_progress("completed", Some(metrics), true)
}

fn has_unified_counts(metrics: &Value) -> bool {
    has_any(
        metrics,
        &[
            "docs_embedded",
            "docs_completed",
            "docs_total",
            "documents_done",
            "documents_total",
            "items_done",
            "items_total",
            "chunks_done",
            "chunks_total",
        ],
    )
}

fn phase_progress(job: &ServiceJob, metrics: &Value) -> Option<String> {
    match job.phase {
        PipelinePhase::Fetching => fetching_progress(job.source_kind, metrics),
        PipelinePhase::Enriching => {
            metric_progress(metrics, "items_done", "items_total", "item", "items")
        }
        PipelinePhase::Normalizing => {
            metric_progress(metrics, "documents_done", "documents_total", "doc", "docs")
        }
        PipelinePhase::Preparing => preparing_progress(metrics),
        PipelinePhase::Batching | PipelinePhase::Embedding => {
            metric_progress(metrics, "chunks_done", "chunks_total", "chunk", "chunks")
        }
        PipelinePhase::Vectorizing | PipelinePhase::Upserting => {
            metric_progress(metrics, "chunks_done", "chunks_total", "vector", "vectors")
        }
        PipelinePhase::Publishing => publishing_progress(metrics),
        _ => document_source_progress("running", Some(metrics)),
    }
}

fn fetching_progress(source_kind: Option<SourceKind>, metrics: &Value) -> Option<String> {
    let (singular, plural) = source_unit(source_kind);
    metric_progress(metrics, "items_done", "items_total", singular, plural)
}

fn source_unit(source_kind: Option<SourceKind>) -> (&'static str, &'static str) {
    match source_kind {
        Some(SourceKind::Web) => ("page", "pages"),
        Some(SourceKind::Local | SourceKind::Git | SourceKind::Upload) => ("file", "files"),
        Some(SourceKind::Registry) => ("version", "versions"),
        Some(SourceKind::Feed) => ("entry", "entries"),
        Some(SourceKind::Youtube) => ("video", "videos"),
        Some(SourceKind::Session) => ("transcript", "transcripts"),
        Some(SourceKind::CliTool | SourceKind::McpTool) => ("tool call", "tool calls"),
        Some(SourceKind::Memory) => ("memory", "memories"),
        Some(SourceKind::Reddit) | None => ("item", "items"),
    }
}

fn preparing_progress(metrics: &Value) -> Option<String> {
    let documents = metric_progress(metrics, "documents_done", "documents_total", "doc", "docs")?;
    let chunks = first_u64(metrics, &["chunks_done"])
        .filter(|chunks| *chunks > 0)
        .map(|chunks| format!(" · {chunks} chunks"))
        .unwrap_or_default();
    Some(format!("{documents}{chunks}"))
}

fn publishing_progress(metrics: &Value) -> Option<String> {
    let total = first_u64(metrics, &["items_total"]);
    let done = first_u64(metrics, &["items_done"]).unwrap_or(0);
    match total {
        Some(total) if total > 0 && done >= total => Some("published generation".to_string()),
        Some(total) if total > 0 => Some("committing generation".to_string()),
        _ => None,
    }
}

fn metric_progress(
    metrics: &Value,
    done_key: &str,
    total_key: &str,
    singular: &str,
    plural: &str,
) -> Option<String> {
    let done = metrics.get(done_key)?.as_u64()?;
    let total = metrics.get(total_key).and_then(Value::as_u64);
    match total {
        Some(total) if total > 0 => Some(format!(
            "{done}/{total} {} · {}",
            if total == 1 { singular } else { plural },
            percentage(done, total)
        )),
        Some(0) if done == 0 => None,
        _ => Some(format!(
            "{done} {}",
            if done == 1 { singular } else { plural }
        )),
    }
}

fn percentage(done: u64, total: u64) -> String {
    let percent = ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
    if percent < 99.95 {
        format!("{percent:.1}%")
    } else {
        "100%".to_string()
    }
}

fn phase_fallback(phase: PipelinePhase) -> String {
    format!("{}…", phase.as_str())
}

fn has_any(metrics: &Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| metrics.get(*key).is_some())
}

fn document_source_progress(status: &str, metrics: Option<&Value>) -> Option<String> {
    let Some(metrics) = metrics else {
        return (status == "running").then(|| "starting…".to_string());
    };
    let docs = first_u64(
        metrics,
        &["docs_embedded", "docs_completed", "documents_done"],
    )
    .unwrap_or(0);
    let chunks = first_u64(metrics, &["chunks_embedded", "chunks_done"]).unwrap_or(0);
    let docs_total = first_u64(metrics, &["docs_total", "documents_total"]);
    let items = first_u64(metrics, &["items_done"]).unwrap_or(0);
    let items_total = first_u64(metrics, &["items_total"]);
    if docs == 0 && chunks == 0 {
        if let Some(total) = items_total.filter(|total| *total > 0) {
            if status == "running" {
                return Some(format!("{items}/{total} items · preparing"));
            }
            return Some(format!(
                "{items}/{total} items · {}",
                percentage(items, total)
            ));
        }
        if status != "running" {
            return None;
        }
        return docs_total
            .filter(|total| *total > 0)
            .map(|total| format!("0/{total} docs · initializing"))
            .or_else(|| Some("initializing".to_string()));
    }
    if let Some(total) = docs_total.filter(|total| *total > 0) {
        return Some(format!(
            "{docs}/{total} docs · {} · {chunks} chunks",
            percentage(docs, total)
        ));
    }
    (docs > 0)
        .then(|| format!("{docs} docs · {chunks} chunks"))
        .or_else(|| Some(format!("{chunks} chunks")))
}

fn first_u64(metrics: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| metrics.get(*key).and_then(Value::as_u64))
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
        return (job.status == "running").then(|| "starting…".to_string());
    };
    let items = metrics
        .get("total_items")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if items == 0 {
        return (job.status == "running").then(|| "extracting…".to_string());
    }
    Some(format!("{items} items"))
}

fn page_source_running_progress(job: &ServiceJob, metrics: &Value) -> Option<String> {
    let crawled = metrics
        .get("pages_crawled")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let docs = metrics
        .get("md_created")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if crawled == 0 && docs == 0 {
        return Some("crawling…".to_string());
    }
    let errors = metrics
        .get("error_pages")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let error_suffix = if errors > 0 {
        format!(" · {errors} errors")
    } else {
        String::new()
    };
    let page_count = metrics
        .get("pages_discovered")
        .and_then(Value::as_u64)
        .filter(|total| *total > 0)
        .map(|total| format!("{crawled}/{total} pages · {}", percentage(crawled, total)))
        .unwrap_or_else(|| format!("{crawled} crawled"));
    let docs_suffix = if docs > 0 {
        format!(" · {docs} docs")
    } else {
        String::new()
    };
    Some(format!(
        "{page_count}{docs_suffix}{error_suffix}{}",
        reclaimed_suffix(job)
    ))
}

fn live_progress_metrics(job: &ServiceJob) -> Option<&Value> {
    job.progress_json.as_ref().or(job.result_json.as_ref())
}

fn page_source_completed_progress(metrics: &Value) -> Option<String> {
    let docs = metrics
        .get("md_created")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut summary = format!("{docs} docs");
    if metrics
        .get("coverage_status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "partial")
    {
        if let Some(reason) = metrics.get("coverage_reason").and_then(Value::as_str) {
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
        return (status == "running" && include_running_fallback).then(|| "starting…".to_string());
    };
    let chunks = metrics
        .get("chunks")
        .or_else(|| metrics.get("chunks_embedded"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    for (done_key, total_key, label) in [
        ("videos_done", "videos_total", "videos"),
        ("files_done", "files_total", "files"),
    ] {
        if let (Some(done), Some(total)) = (
            metrics.get(done_key).and_then(Value::as_u64),
            metrics.get(total_key).and_then(Value::as_u64),
        ) {
            return Some(format!(
                "{done} / {total} {label}, {chunks} chunks embedded"
            ));
        }
    }
    if let (Some(done), Some(total)) = (
        metrics.get("tasks_done").and_then(Value::as_u64),
        metrics.get("tasks_total").and_then(Value::as_u64),
    ) {
        let phase = metrics
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("working");
        return Some(if chunks == 0 {
            format!("{phase} ({done} / {total} tasks)")
        } else {
            format!("{phase} ({done} / {total} tasks), {chunks} chunks embedded")
        });
    }
    if chunks == 0 {
        return (status == "running" && include_running_fallback).then(|| "indexing…".to_string());
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
#[path = "job_progress_tests.rs"]
mod tests;
