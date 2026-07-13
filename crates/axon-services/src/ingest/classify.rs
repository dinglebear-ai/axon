use axon_jobs::ingest::IngestSource;
use std::error::Error;

pub fn classify_target(target: &str, include_source: bool) -> Result<IngestSource, Box<dyn Error>> {
    crate::ingest::classify_target::classify_target(target, include_source)
}
