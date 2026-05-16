//! DOM extraction baseline benchmark.
//!
//! Measures `axon::core::content::bytes_to_markdown` — the single-pass
//! DOM extractor used by primary crawls, Chrome recovery, sitemap
//! backfill, and URL embed fetches — against a fixture corpus of
//! thin / medium / dense HTML pages.
//!
//! Unblocks bead axon_rust-4j1n:
//! - p50/p95/p99 baseline for jh32 (DOM retry ladder gate <2x baseline)
//! - throughput context for 1jto walker scope decision
//!
//! Run: `cargo bench --bench dom_extraction`
//! Quick smoke: `cargo bench --bench dom_extraction -- --quick`

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde::Deserialize;

use axon::core::content::bytes_to_markdown;

#[derive(Debug, Deserialize)]
struct Manifest {
    fixtures: Vec<FixtureEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct FixtureEntry {
    filename: String,
    #[allow(dead_code)]
    url: String,
    #[allow(dead_code)]
    expected_word_count: usize,
    page_type: String,
}

struct LoadedFixture {
    name: String,
    page_type: String,
    bytes: Vec<u8>,
}

fn fixtures_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("pages")
}

fn load_fixtures() -> Vec<LoadedFixture> {
    let dir = fixtures_dir();
    let manifest_path = dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
    let manifest: Manifest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse manifest.json: {e}"));

    manifest
        .fixtures
        .into_iter()
        .map(|entry| {
            let path = dir.join(&entry.filename);
            let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            LoadedFixture {
                name: entry.filename,
                page_type: entry.page_type,
                bytes,
            }
        })
        .collect()
}

/// Per-page Criterion benchmarks, grouped by page_type. Criterion produces
/// per-fixture mean/median plus the canonical HTML reports.
fn bench_per_page(c: &mut Criterion) {
    let fixtures = load_fixtures();
    let mut group = c.benchmark_group("bytes_to_markdown");
    group.sample_size(50);

    for fx in &fixtures {
        group.throughput(Throughput::Bytes(fx.bytes.len() as u64));
        let id = BenchmarkId::new(&fx.page_type, &fx.name);
        group.bench_with_input(id, &fx.bytes, |b, html_bytes| {
            b.iter(|| {
                let md = bytes_to_markdown(html_bytes, None);
                std::hint::black_box(md);
            });
        });
    }

    group.finish();
}

/// Manual aggregate pass: walks all fixtures, runs N iterations each, and
/// prints p50/p95/p99 in ms grouped by page_type plus overall. The numbers
/// land in stderr so we can capture them in `docs/perf/results-dom-baseline.json`.
fn bench_aggregate(c: &mut Criterion) {
    let fixtures = load_fixtures();
    let iters_per_fx: usize = 200;

    let mut per_type: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    let mut overall: Vec<f64> = Vec::with_capacity(fixtures.len() * iters_per_fx);

    for fx in &fixtures {
        for _ in 0..iters_per_fx {
            let start = Instant::now();
            let md = bytes_to_markdown(&fx.bytes, None);
            std::hint::black_box(md);
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            per_type
                .entry(fx.page_type.clone())
                .or_default()
                .push(elapsed_ms);
            overall.push(elapsed_ms);
        }
    }

    eprintln!("\n=== bytes_to_markdown aggregate ({iters_per_fx} iters/fixture) ===");
    for (kind, mut samples) in per_type {
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        eprintln!(
            "  {kind:>6}: p50={:.3}ms p95={:.3}ms p99={:.3}ms (n={})",
            pct(&samples, 0.50),
            pct(&samples, 0.95),
            pct(&samples, 0.99),
            samples.len()
        );
    }
    overall.sort_by(|a, b| a.partial_cmp(b).unwrap());
    eprintln!(
        "  overall: p50={:.3}ms p95={:.3}ms p99={:.3}ms (n={})\n",
        pct(&overall, 0.50),
        pct(&overall, 0.95),
        pct(&overall, 0.99),
        overall.len()
    );

    // Cheap criterion entry so this function shows up in bench output.
    c.bench_function("aggregate_smoke", |b| {
        let fx = &fixtures[0];
        b.iter(|| {
            let md = bytes_to_markdown(&fx.bytes, None);
            std::hint::black_box(md);
        });
    });
}

fn pct(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

criterion_group!(benches, bench_per_page, bench_aggregate);
criterion_main!(benches);
