mod dispatchers;
mod dispatchers_brand_diff;
mod helpers;
mod job_ops;

use crate::services::types::ClientActionError;
use uuid::Uuid;

pub(super) use dispatchers::{
    dispatch_crawl, dispatch_embed, dispatch_endpoints, dispatch_extract, dispatch_ingest,
    dispatch_scrape, dispatch_screenshot, dispatch_summarize,
};
pub(super) use dispatchers_brand_diff::{dispatch_brand, dispatch_diff};

pub(super) fn parse_job_id(raw: Option<&str>) -> Result<Uuid, ClientActionError> {
    let raw = raw.ok_or_else(|| {
        ClientActionError::new(
            "invalid_request",
            "job_id is required",
            false,
            Some("include a UUID job_id for this lifecycle action".to_string()),
        )
    })?;
    Uuid::parse_str(raw).map_err(|err| {
        ClientActionError::new(
            "invalid_request",
            format!("invalid job_id: {err}"),
            false,
            Some("job_id must be a UUID returned by a start action".to_string()),
        )
    })
}
