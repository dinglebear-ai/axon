//! Canonical, transport-neutral job-progress derivation.
//!
//! `ServiceJob` carries raw `status` + `result_json`; turning that into a
//! `{ phase, percent, metrics }` view used to be re-derived independently by
//! every surface (the palette's TS `summarizeJob`, and ad-hoc CLI rendering).
//! This is the single source of truth: REST/MCP include it in the job-status
//! response, and the palette/android/CLI consume it instead of re-deriving — so
//! the derivation can't drift across surfaces.
//!
//! Scope: the generic async families (`embed`, `extract`, `ingest`). Crawl keeps
//! its richer client-side snapshot (page frontier / depth / event log) for now;
//! folding it in would need a superset DTO and is tracked separately.

use crate::service_job::ServiceJob;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobFamily {
    Embed,
    Extract,
    Ingest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobPhase {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

impl JobPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, JobPhase::Done | JobPhase::Failed | JobPhase::Canceled)
    }

    fn from_status(status: &str) -> Self {
        match status {
            "pending" => JobPhase::Pending,
            "completed" => JobPhase::Done,
            "failed" => JobPhase::Failed,
            "canceled" | "cancelled" => JobPhase::Canceled,
            // Any other status string (including the live "running" and any
            // future/unknown state) is treated as in-flight. A job we can't
            // classify is safer shown as Running (active) than as a terminal
            // phase that would stop the client from polling.
            _ => JobPhase::Running,
        }
    }
}

/// One labelled, display-formatted counter (e.g. `{ "Chunks", "1,024" }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JobMetric {
    pub label: String,
    /// **Display-formatted, not machine-readable.** Pre-rendered for direct
    /// rendering by clients — integers carry thousands separators (`"1,024"`)
    /// and string fields (e.g. ingest `phase`) pass through verbatim. Do not
    /// parse this back into a number; if a surface needs the raw value, add a
    /// typed field rather than reverse-engineering the formatting here.
    pub value: String,
}

/// Derived, transport-neutral progress for a generic async job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JobProgress {
    pub family: JobFamily,
    pub phase: JobPhase,
    /// 0–100 when determinate; `None` = indeterminate (render a pulsing bar).
    pub percent: Option<f64>,
    pub metrics: Vec<JobMetric>,
    pub error: Option<String>,
}

impl JobProgress {
    pub fn from_service_job(family: JobFamily, job: &ServiceJob) -> Self {
        Self::derive(
            family,
            &job.status,
            job.result_json.as_ref(),
            job.error_text.as_deref(),
        )
    }

    /// Derive from an already-serialized job value (`status` / `result_json` /
    /// `error_text` keys) — used by surfaces that hold the wire JSON rather than
    /// a `ServiceJob` (e.g. the MCP status handlers).
    pub fn from_wire_value(family: JobFamily, value: &Value) -> Self {
        let status = value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending");
        let error = value.get("error_text").and_then(Value::as_str);
        Self::derive(family, status, value.get("result_json"), error)
    }

    pub fn derive(
        family: JobFamily,
        status: &str,
        result_json: Option<&Value>,
        error: Option<&str>,
    ) -> Self {
        let phase = JobPhase::from_status(status);
        let result = result_json.and_then(Value::as_object);

        JobProgress {
            family,
            phase,
            percent: determinate_percent(family, phase, result),
            metrics: metrics_for(family, result),
            error: error.filter(|e| !e.is_empty()).map(str::to_string),
        }
    }
}

type ResultMap = serde_json::Map<String, Value>;

/// Pull a finite integer out of `result[key]` (accepts u64/i64/f64).
fn num(result: Option<&ResultMap>, key: &str) -> Option<i64> {
    let v = result?.get(key)?;
    if let Some(n) = v.as_i64() {
        Some(n)
    } else if let Some(n) = v.as_u64() {
        Some(n as i64)
    } else {
        v.as_f64().filter(|f| f.is_finite()).map(|f| f as i64)
    }
}

fn get_str<'a>(result: Option<&'a ResultMap>, key: &str) -> Option<&'a str> {
    result?.get(key)?.as_str().filter(|s| !s.is_empty())
}

/// Thousands-separated integer, matching the palette's `toLocaleString()`.
fn fmt_int(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    let len = digits.len();
    for (i, c) in digits.chars().enumerate() {
        if i != 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if neg { format!("-{out}") } else { out }
}

fn push_num(metrics: &mut Vec<JobMetric>, label: &str, value: Option<i64>) {
    if let Some(n) = value {
        metrics.push(JobMetric {
            label: label.to_string(),
            value: fmt_int(n),
        });
    }
}

fn determinate_percent(
    family: JobFamily,
    phase: JobPhase,
    result: Option<&ResultMap>,
) -> Option<f64> {
    if phase == JobPhase::Done {
        return Some(100.0);
    }
    if phase != JobPhase::Running && phase != JobPhase::Pending {
        return None;
    }
    if family == JobFamily::Ingest
        && let (Some(done), Some(total)) = (num(result, "tasks_done"), num(result, "tasks_total"))
        && total > 0
    {
        return Some(((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0));
    }
    None
}

fn metrics_for(family: JobFamily, result: Option<&ResultMap>) -> Vec<JobMetric> {
    let mut metrics = Vec::new();
    match family {
        JobFamily::Embed => {
            push_num(&mut metrics, "Docs", num(result, "docs_embedded"));
            push_num(&mut metrics, "Chunks", num(result, "chunks_embedded"));
        }
        JobFamily::Extract => {
            push_num(&mut metrics, "Pages", num(result, "pages_visited"));
            push_num(&mut metrics, "With data", num(result, "pages_with_data"));
            push_num(&mut metrics, "Items", num(result, "total_items"));
        }
        JobFamily::Ingest => {
            if let Some(phase) = get_str(result, "phase") {
                metrics.push(JobMetric {
                    label: "Phase".to_string(),
                    value: phase.to_string(),
                });
            }
            let files_ast = num(result, "files_ast_chunked");
            let files_prose = num(result, "files_prose_fallback");
            if files_ast.is_some() || files_prose.is_some() {
                push_num(
                    &mut metrics,
                    "Files",
                    Some(files_ast.unwrap_or(0) + files_prose.unwrap_or(0)),
                );
            }
            push_num(
                &mut metrics,
                "Chunks",
                num(result, "chunks_embedded").or_else(|| num(result, "chunks")),
            );
        }
    }
    metrics
}

#[cfg(test)]
#[path = "job_progress_tests.rs"]
mod tests;
