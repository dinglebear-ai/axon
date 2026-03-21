use super::helpers::build_integrity;
use crate::crates::services::types::{
    ExportIntegrity, ExportManifest, ExportMetadata, ExportVerifyMismatch, ExportVerifyReport,
    IngestExports, QdrantSummary, RebuildSeedsExport, RefreshExports, SettingsSnapshot,
};
use anyhow::Result;
use std::collections::HashMap;

pub(super) fn verify_manifest_value(value: &serde_json::Value) -> Result<ExportVerifyReport> {
    let mut missing_required_keys = Vec::new();
    let mut parse_error = None;
    let mut hash_mismatches = Vec::new();
    let mut count_mismatches = Vec::new();

    let Some(top) = value.as_object() else {
        return Ok(ExportVerifyReport {
            valid: false,
            version: None,
            required_keys_checked: super::REQUIRED_TOP_LEVEL_KEYS
                .iter()
                .map(|v| (*v).to_string())
                .collect(),
            missing_required_keys: vec!["<top-level object>".to_string()],
            parse_error: Some("manifest root must be an object".to_string()),
            hash_mismatches,
            count_mismatches,
        });
    };

    for key in super::REQUIRED_TOP_LEVEL_KEYS {
        if !top.contains_key(*key) {
            missing_required_keys.push((*key).to_string());
        }
    }

    let version = top
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());

    let mut valid_version = version == Some(super::EXPORT_SCHEMA_VERSION);

    let manifest = match serde_json::from_value::<ExportManifest>(value.clone()) {
        Ok(manifest) => manifest,
        Err(err) => {
            parse_error = Some(err.to_string());
            build_default_failed_manifest()
        }
    };

    if parse_error.is_none() {
        check_integrity_mismatches(&manifest, &mut hash_mismatches, &mut count_mismatches);
    } else {
        valid_version = false;
    }

    let valid = missing_required_keys.is_empty()
        && parse_error.is_none()
        && valid_version
        && hash_mismatches.is_empty()
        && count_mismatches.is_empty();

    Ok(ExportVerifyReport {
        valid,
        version,
        required_keys_checked: super::REQUIRED_TOP_LEVEL_KEYS
            .iter()
            .map(|v| (*v).to_string())
            .collect(),
        missing_required_keys,
        parse_error,
        hash_mismatches,
        count_mismatches,
    })
}

fn build_default_failed_manifest() -> ExportManifest {
    ExportManifest {
        version: 0,
        exported_at: String::new(),
        collection: String::new(),
        metadata: ExportMetadata {
            schema_version: 0,
            generated_by: String::new(),
            generated_by_version: String::new(),
            history_included: false,
        },
        settings_snapshot: SettingsSnapshot {
            collection: String::new(),
            performance_profile: String::new(),
            render_mode: String::new(),
            max_pages: 0,
            max_depth: 0,
            include_subdomains: false,
            respect_robots: false,
            min_markdown_chars: 0,
            drop_thin_markdown: false,
            discover_sitemaps: false,
            sitemap_since_days: 0,
            request_timeout_ms: None,
            fetch_retries: 0,
            retry_backoff_ms: 0,
            batch_concurrency: 0,
            crawl_queue: String::new(),
            extract_queue: String::new(),
            embed_queue: String::new(),
            ingest_queue: String::new(),
            graph_queue: String::new(),
        },
        integrity: ExportIntegrity {
            counts: HashMap::new(),
            hashes: HashMap::new(),
        },
        rebuild_seeds: RebuildSeedsExport {
            crawl_seed_urls: vec![],
            scrape_urls: vec![],
            scrape_requests: vec![],
            github_repos: vec![],
            github_requests: vec![],
            reddit_targets: vec![],
            youtube_targets: vec![],
            session_targets: vec![],
            local_paths: vec![],
            extraction_requests: vec![],
            search_requests: vec![],
            research_requests: vec![],
            search_queries: vec![],
            research_queries: vec![],
        },
        crawls: vec![],
        scrapes: vec![],
        extractions: vec![],
        embeds: vec![],
        ingests: IngestExports {
            github: vec![],
            reddit: vec![],
            youtube: vec![],
            sessions: vec![],
        },
        refreshes: RefreshExports {
            schedules: vec![],
            jobs: vec![],
        },
        watches: vec![],
        qdrant_summary: QdrantSummary {
            total_points: 0,
            source_type_counts: HashMap::new(),
            domain_counts: HashMap::new(),
        },
    }
}

fn check_integrity_mismatches(
    manifest: &ExportManifest,
    hash_mismatches: &mut Vec<ExportVerifyMismatch>,
    count_mismatches: &mut Vec<ExportVerifyMismatch>,
) {
    let expected_integrity = build_integrity(&manifest.rebuild_seeds);

    for (key, expected) in &expected_integrity.hashes {
        let actual = manifest
            .integrity
            .hashes
            .get(key)
            .cloned()
            .unwrap_or_default();
        if actual != *expected {
            hash_mismatches.push(ExportVerifyMismatch {
                key: key.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }

    for (key, expected) in &expected_integrity.counts {
        match manifest.integrity.counts.get(key).copied() {
            Some(actual) if actual == *expected => {}
            actual_opt => {
                count_mismatches.push(ExportVerifyMismatch {
                    key: key.clone(),
                    expected: expected.to_string(),
                    actual: actual_opt.unwrap_or(0).to_string(),
                });
            }
        }
    }
}
