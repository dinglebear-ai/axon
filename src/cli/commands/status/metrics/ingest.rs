use crate::core::ui::{accent, metric, muted, subtle};
use serde_json::Value;

#[allow(dead_code)]
pub fn ingest_metrics_suffix(status: &str, result_json: Option<&Value>) -> String {
    let sep = subtle(" | ");
    if matches!(status, "pending" | "running" | "processing") {
        return ingest_active_metrics_suffix(result_json, &sep);
    }
    ingest_completed_metrics_suffix(result_json, &sep)
}

fn ingest_active_metrics_suffix(result_json: Option<&Value>, sep: &str) -> String {
    let Some(r) = result_json else {
        return String::new();
    };
    let chunks = r
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let tasks_done = r.get("tasks_done").and_then(|v| v.as_u64());
    let tasks_total = r.get("tasks_total").and_then(|v| v.as_u64());
    let phase = r.get("phase").and_then(|v| v.as_str());

    if tasks_done.is_some() || tasks_total.is_some() || phase.is_some() {
        return build_rich_active_suffix(r, chunks, tasks_done, tasks_total, phase, sep);
    }

    if let Some(line) =
        progress_with_chunks(r, "videos_done", "videos_total", "videos", chunks, sep)
    {
        return line;
    }
    if let Some(line) = progress_with_chunks(r, "files_done", "files_total", "files", chunks, sep) {
        return line;
    }

    let enumerating = r
        .get("enumerating")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    match () {
        _ if enumerating => format!("{sep}{}", muted("enumerating…")),
        _ if chunks > 0 => format!("{sep}{}", metric(chunks, "chunks")),
        _ => String::new(),
    }
}

fn build_rich_active_suffix(
    r: &Value,
    chunks: u64,
    tasks_done: Option<u64>,
    tasks_total: Option<u64>,
    phase: Option<&str>,
    sep: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(frag) = format_fraction(r, "videos_done", "videos_total", "videos") {
        parts.push(frag);
    } else if let Some(frag) = format_fraction(r, "files_done", "files_total", "files") {
        parts.push(frag);
    }

    if chunks > 0 {
        parts.push(metric(chunks, "chunks"));
    }

    if let (Some(done), Some(total)) = (tasks_done, tasks_total) {
        parts.push(format!(
            "{}{}{} {}",
            accent(&done.to_string()),
            subtle("/"),
            accent(&total.to_string()),
            accent("tasks"),
        ));
    }

    if let Some(p) = phase {
        parts.push(muted(p));
    }

    if let Some(detail) = phase_detail(r, phase) {
        parts.push(detail);
    }

    if parts.is_empty() {
        return String::new();
    }
    format!("{sep}{}", parts.join(sep))
}

fn format_fraction(r: &Value, done_key: &str, total_key: &str, label: &str) -> Option<String> {
    let done = r.get(done_key).and_then(|v| v.as_u64())?;
    let total = r.get(total_key).and_then(|v| v.as_u64())?;
    Some(format!(
        "{}{}{} {}",
        accent(&done.to_string()),
        subtle("/"),
        accent(&total.to_string()),
        accent(label),
    ))
}

fn phase_detail(r: &Value, phase: Option<&str>) -> Option<String> {
    match phase? {
        "fetching_issues" => fetch_detail(r, "issues_fetched", "issues_page", "issues"),
        "fetching_prs" => fetch_detail(r, "prs_fetched", "issues_page", "PRs"),
        "embedding_issues" | "embedding_prs" | "embedding_wiki" => {
            let total = r
                .get("issues_total")
                .or_else(|| r.get("prs_total"))
                .or_else(|| r.get("wiki_pages"))
                .and_then(|v| v.as_u64());
            total.map(|n| format!("{} items", accent(&n.to_string())))
        }
        _ => None,
    }
}

fn fetch_detail(r: &Value, count_key: &str, page_key: &str, label: &str) -> Option<String> {
    let fetched = r.get(count_key).and_then(|v| v.as_u64());
    let page = r.get(page_key).and_then(|v| v.as_u64());
    match (fetched, page) {
        (Some(n), Some(p)) => Some(format!(
            "{} {label}, page {}",
            accent(&n.to_string()),
            accent(&p.to_string()),
        )),
        (Some(n), None) => Some(format!("{} {label}", accent(&n.to_string()))),
        (None, Some(p)) => Some(format!("page {}", accent(&p.to_string()))),
        (None, None) => None,
    }
}

fn ingest_completed_metrics_suffix(result_json: Option<&Value>, sep: &str) -> String {
    let Some(r) = result_json else {
        return String::new();
    };
    let chunks = r
        .get("chunks_embedded")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if chunks == 0 {
        return String::new();
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "videos_done", "videos_total", "videos", chunks, sep)
    {
        return line;
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "files_done", "files_total", "files", chunks, sep)
    {
        return line;
    }
    if let Some(line) =
        completed_progress_with_chunks(r, "tasks_done", "tasks_total", "tasks", chunks, sep)
    {
        return line;
    }
    format!("{sep}{}", metric(chunks, "chunks"))
}

fn progress_with_chunks(
    payload: &Value,
    done_key: &str,
    total_key: &str,
    label: &str,
    chunks: u64,
    sep: &str,
) -> Option<String> {
    let done = payload.get(done_key).and_then(|v| v.as_u64())?;
    let total = payload.get(total_key).and_then(|v| v.as_u64())?;
    Some(format_progress_with_chunks(done, total, label, chunks, sep))
}

fn completed_progress_with_chunks(
    payload: &Value,
    done_key: &str,
    total_key: &str,
    label: &str,
    chunks: u64,
    sep: &str,
) -> Option<String> {
    let total = payload.get(total_key).and_then(|v| v.as_u64())?;
    let done = payload
        .get(done_key)
        .and_then(|v| v.as_u64())
        .unwrap_or(total);
    Some(format_progress_with_chunks(done, total, label, chunks, sep))
}

fn format_progress_with_chunks(
    done: u64,
    total: u64,
    label: &str,
    chunks: u64,
    sep: &str,
) -> String {
    format!(
        "{sep}{}{}{} {label}{sep}{}",
        accent(&done.to_string()),
        subtle("/"),
        accent(&total.to_string()),
        metric(chunks, "chunks"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        console::strip_ansi_codes(s).into_owned()
    }

    #[test]
    fn ingest_suffix_shows_phase_and_tasks_done() {
        let result = serde_json::json!({
            "files_done": 150, "files_total": 155, "chunks_embedded": 1700,
            "tasks_done": 3, "tasks_total": 5,
            "phase": "fetching_issues", "issues_fetched": 42, "issues_page": 2,
        });
        let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
        assert!(
            suffix.contains("fetching_issues"),
            "should show phase: {suffix}"
        );
        assert!(
            suffix.contains("3/5"),
            "should show task progress: {suffix}"
        );
    }

    #[test]
    fn ingest_suffix_shows_embedding_issues_phase() {
        let result = serde_json::json!({
            "tasks_done": 2, "tasks_total": 5,
            "phase": "embedding_issues", "issues_total": 100, "chunks_embedded": 2400,
        });
        let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
        assert!(
            suffix.contains("embedding_issues"),
            "should show phase: {suffix}"
        );
    }

    #[test]
    fn ingest_suffix_backward_compatible_files_only() {
        let result = serde_json::json!({
            "files_done": 10, "files_total": 20, "chunks_embedded": 500,
        });
        let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
        assert!(
            suffix.contains("10/20"),
            "should show file progress: {suffix}"
        );
        assert!(suffix.contains("500"), "should show chunk count: {suffix}");
    }

    #[test]
    fn ingest_suffix_phase_specific_detail_fetching_issues() {
        let result = serde_json::json!({
            "tasks_done": 1, "tasks_total": 4,
            "phase": "fetching_issues", "issues_fetched": 42, "issues_page": 2,
        });
        let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
        assert!(
            suffix.contains("42"),
            "should show issues fetched: {suffix}"
        );
        assert!(
            suffix.contains("page 2"),
            "should show page number: {suffix}"
        );
    }
}
