use axon_jobs::backend::JobKind;
use rmcp::ErrorData;
use uuid::Uuid;

const TASK_PREFIX: &str = "axon";

pub(super) fn task_id_for(kind: JobKind, job_id: Uuid) -> String {
    format!("{TASK_PREFIX}:{}:{job_id}", kind_name(kind))
}

pub(super) fn parse_task_id(task_id: &str) -> Result<(JobKind, Uuid), ErrorData> {
    let mut parts = task_id.split(':');
    let prefix = parts
        .next()
        .ok_or_else(|| invalid_task_id("missing task prefix"))?;
    if prefix != TASK_PREFIX {
        return Err(invalid_task_id("task ID must start with axon"));
    }
    let kind = parts
        .next()
        .ok_or_else(|| invalid_task_id("missing task kind"))
        .and_then(parse_kind)?;
    let raw_uuid = parts
        .next()
        .ok_or_else(|| invalid_task_id("missing job UUID"))?;
    if parts.next().is_some() {
        return Err(invalid_task_id("task ID has too many segments"));
    }
    let job_id = Uuid::parse_str(raw_uuid).map_err(|_| invalid_task_id("invalid job UUID"))?;
    Ok((kind, job_id))
}

pub(super) fn kind_name(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Crawl => "crawl",
        JobKind::Embed => "embed",
        JobKind::Extract => "extract",
        JobKind::Ingest => "ingest",
    }
}

fn parse_kind(kind: &str) -> Result<JobKind, ErrorData> {
    match kind {
        "crawl" => Ok(JobKind::Crawl),
        "embed" => Ok(JobKind::Embed),
        "extract" => Ok(JobKind::Extract),
        "ingest" => Ok(JobKind::Ingest),
        _ => Err(invalid_task_id("unknown task kind")),
    }
}

fn invalid_task_id(reason: &str) -> ErrorData {
    ErrorData::invalid_params(format!("invalid task_id: {reason}"), None)
}

#[cfg(test)]
#[path = "task_id_tests.rs"]
mod tests;
