use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level export manifest for rebuilding indexed knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub version: u32,
    pub exported_at: String,
    pub collection: String,
    pub crawls: Vec<CrawlExport>,
    pub scrapes: Vec<ScrapeExport>,
    pub extractions: Vec<ExtractionExport>,
    pub embeds: Vec<EmbedExport>,
    pub ingests: IngestExports,
    pub refreshes: RefreshExports,
    pub qdrant_summary: QdrantSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlExport {
    pub job_id: String,
    pub seed_url: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub pages_crawled: Option<u64>,
    pub pages_discovered: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeExport {
    pub url: String,
    pub scraped_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub total_items: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedExport {
    pub job_id: String,
    pub input: String,
    pub collection: String,
    pub status: String,
    pub source_type: Option<String>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestExports {
    pub github: Vec<IngestSourceExport>,
    pub reddit: Vec<IngestSourceExport>,
    pub youtube: Vec<IngestSourceExport>,
    pub sessions: Vec<IngestSourceExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSourceExport {
    pub job_id: String,
    pub target: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshExports {
    pub schedules: Vec<RefreshScheduleExport>,
    pub jobs: Vec<RefreshJobExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshScheduleExport {
    pub id: String,
    pub name: String,
    pub seed_url: Option<String>,
    pub urls: Vec<String>,
    pub every_seconds: i64,
    pub enabled: bool,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshJobExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub checked: Option<u64>,
    pub changed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantSummary {
    pub total_points: u64,
    pub source_type_counts: HashMap<String, u64>,
    pub domain_counts: HashMap<String, u64>,
    pub indexed_urls: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_manifest_serializes_to_json() {
        let manifest = ExportManifest {
            version: 1,
            exported_at: "2026-03-19T12:00:00Z".into(),
            collection: "cortex".into(),
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
            qdrant_summary: QdrantSummary {
                total_points: 0,
                source_type_counts: HashMap::new(),
                domain_counts: HashMap::new(),
                indexed_urls: vec![],
            },
        };

        let json = serde_json::to_string(&manifest).expect("manifest serializes");
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"crawls\":[]"));
    }
}
