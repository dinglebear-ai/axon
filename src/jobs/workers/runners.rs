mod crawl;
mod embed;
mod extract;
mod ingest;

pub(super) use crawl::run_crawl_job;
pub(super) use embed::run_embed_job;
pub(super) use extract::run_extract_job;
pub(super) use ingest::run_ingest_job;

pub(super) type JobResult =
    Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
#[path = "runners_tests.rs"]
mod tests;
