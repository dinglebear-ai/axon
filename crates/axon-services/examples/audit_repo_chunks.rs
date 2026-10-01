//! Read-only corpus audit using the production Git adapter and document preparer.
//! Usage: cargo run -p axon-services --example audit_repo_chunks -- REPO_ROOT REPO_URL
use std::collections::BTreeMap;
use std::path::Path;

use axon_adapters::{SourceAdapter, git::GitSourceAdapter};
use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use axon_route::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Default)]
struct Audit {
    documents: usize,
    skipped: usize,
    chunks: usize,
    under_minimum: usize,
    punctuation_only: usize,
    duplicate_texts: usize,
    texts: BTreeMap<String, usize>,
    methods: BTreeMap<String, usize>,
    ranges: BTreeMap<String, Vec<(u64, u64)>>,
}

impl Audit {
    fn document(&mut self, document: PreparedDocument) {
        self.documents += 1;
        for chunk in document.chunks {
            self.chunks += 1;
            self.under_minimum += usize::from(chunk.content.chars().count() < 50);
            self.punctuation_only += usize::from(!chunk.content.chars().any(char::is_alphanumeric));
            let digest = hex::encode(Sha256::digest(chunk.content.as_bytes()));
            let count = self.texts.entry(digest).or_default();
            self.duplicate_texts += usize::from(*count > 0);
            *count += 1;
            let method = chunk
                .metadata
                .get("actual_chunking_method")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&document.chunking_method);
            *self.methods.entry(method.to_owned()).or_default() += 1;
            if let (Some(start), Some(end)) =
                (chunk.source_range.byte_start, chunk.source_range.byte_end)
            {
                self.ranges
                    .entry(document.source_item_key.0.clone())
                    .or_default()
                    .push((start, end));
            }
        }
    }

    fn report(&self, canonical_files: usize, aliases: usize) -> serde_json::Value {
        let mut total = 0_u64;
        let mut union = 0_u64;
        for ranges in self.ranges.values() {
            let mut ranges = ranges.clone();
            ranges.sort_unstable();
            let mut last_end = 0;
            for (start, end) in ranges {
                total += end.saturating_sub(start);
                union += end.saturating_sub(start.max(last_end));
                last_end = last_end.max(end);
            }
        }
        json!({"canonical_files": canonical_files, "alias_files": aliases,
            "documents_prepared": self.documents, "documents_skipped": self.skipped,
            "chunks": self.chunks, "under_50_chars": self.under_minimum,
            "punctuation_only": self.punctuation_only, "duplicate_chunk_texts": self.duplicate_texts,
            "range_bytes": total, "unique_range_bytes": union, "overlap_bytes": total-union,
            "actual_methods": self.methods})
    }
}

fn plan(root: &Path, url: &str) -> anyhow::Result<SourcePlan> {
    let registry = AdapterRegistry::target_defaults();
    let request = SourceRequest::new(url);
    let resolved = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone())
        .resolve(&request)?;
    let mut route = SourceRouter::new(registry).route(&request, resolved)?;
    route
        .validated_options
        .values
        .insert("repo_root".into(), json!(root.canonicalize()?));
    Ok(SourcePlan {
        job_id: JobId::new(Uuid::new_v4()),
        request,
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("chunk-audit"),
        provider_reservations: Vec::new(),
    })
}

fn diff(plan: &SourcePlan, items: &[ManifestItem]) -> SourceManifestDiff {
    SourceManifestDiff {
        header: StageResultHeader {
            job_id: plan.job_id,
            stage_id: StageId::new(Uuid::new_v4()),
            phase: PipelinePhase::Diffing,
            status: LifecycleStatus::Completed,
            started_at: Timestamp("1970-01-01T00:00:00Z".into()),
            completed_at: None,
            counts: StageCounts {
                items_total: Some(items.len() as u64),
                items_done: items.len() as u64,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::new("gen_1"),
        added: items.to_vec(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: items.len() as u64,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: audit_repo_chunks REPO_ROOT REPO_URL"
    );
    let plan = plan(Path::new(&args[1]), &args[2])?;
    let adapter = GitSourceAdapter::new();
    let manifest = adapter.discover(&plan).await?;
    let aliases: usize = manifest
        .items
        .iter()
        .filter_map(|item| {
            item.metadata
                .get("source_item_aliases")
                .and_then(serde_json::Value::as_array)
        })
        .map(Vec::len)
        .sum();
    let preparer = DocumentPreparer::default();
    let mut audit = Audit::default();
    for items in manifest.items.chunks(64) {
        let acquisition = adapter.acquire(&plan, &diff(&plan, items)).await?;
        for document in adapter.normalize(&plan, acquisition).await?.data {
            let result = preparer
                .prepare(PrepareSourceDocumentRequest {
                    document,
                    generation: SourceGenerationId::new("gen_1"),
                    profile: None,
                    parse_facts: Vec::new(),
                    graph_candidates: Vec::new(),
                    warnings: Vec::new(),
                    errors: Vec::new(),
                })
                .map_err(anyhow::Error::msg)?;
            match result {
                PrepareSourceDocumentResult::Prepared(document) => audit.document(*document),
                PrepareSourceDocumentResult::Skipped(_) => audit.skipped += 1,
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&audit.report(manifest.items.len(), aliases))?
    );
    anyhow::ensure!(
        audit.under_minimum == 0 && audit.punctuation_only == 0,
        "chunk quality gate failed"
    );
    Ok(())
}
