//! Predict the on-disk output directory for a crawl job before it runs.

use axon_core::content::url_to_domain;
use std::path::{Path, PathBuf};

/// `<base>/domains/<domain>/<job_id>` — the canonical crawl output directory.
pub fn predict_crawl_output_dir(base_output_dir: &Path, url: &str, job_id: &str) -> PathBuf {
    base_output_dir
        .join("domains")
        .join(url_to_domain(url))
        .join(job_id)
}
