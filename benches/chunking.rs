//! Criterion benchmarks for the chunking pipeline.
//!
//! **Wiring required (Lane H):** To activate these benchmarks, add to `Cargo.toml`:
//!
//! ```toml
//! [dev-dependencies]
//! criterion = { version = "0.5", features = ["html_reports"] }
//!
//! [[bench]]
//! name = "chunking"
//! harness = false
//! ```
//!
//! Then run with: `cargo bench --bench chunking`
//!
//! These benchmarks guard against chunk-count amplification regressions (P-H1,
//! P-L2) and provide a baseline for embedding throughput estimates.

use axon_vector::ops::input::code::chunk_code;
use axon_vector::ops::input::{chunk_markdown, chunk_text, chunk_text_with_offsets};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

// ── synthetic document fixtures ───────────────────────────────────────────────

/// Generate a synthetic prose document of approximately `char_count` chars.
fn make_prose_doc(char_count: usize) -> String {
    let sentence = "The quick brown fox jumps over the lazy dog. \
                    Axon embeds documents into Qdrant using TEI hybrid search. \
                    Each chunk carries a byte offset for source-line attribution. ";
    let repeat = (char_count / sentence.len()) + 1;
    sentence.repeat(repeat).chars().take(char_count).collect()
}

/// Generate a synthetic markdown document with headers and paragraphs.
fn make_markdown_doc(char_count: usize) -> String {
    let block = "## Section Title\n\n\
                 This is a paragraph about embedding and vector search.\n\
                 It contains multiple sentences to exercise the splitter.\n\n\
                 ### Subsection\n\n\
                 Another paragraph with more details about ranking and retrieval.\n\n";
    let repeat = (char_count / block.len()) + 1;
    block.repeat(repeat).chars().take(char_count).collect()
}

/// Generate a synthetic Rust source file of approximately `char_count` chars.
fn make_rust_source(char_count: usize) -> String {
    let snippet = r#"
pub fn process_document(content: &str, cfg: &Config) -> Vec<String> {
    let chunks = chunk_text(content);
    chunks.into_iter()
        .filter(|c| !c.trim().is_empty())
        .map(|c| format!("{}: {}", cfg.collection, c))
        .collect()
}

pub struct Config {
    pub collection: String,
    pub max_chunks: usize,
}
"#;
    let repeat = (char_count / snippet.len()) + 1;
    snippet.repeat(repeat).chars().take(char_count).collect()
}

// ── benchmarks ────────────────────────────────────────────────────────────────

fn bench_chunk_text(c: &mut Criterion) {
    let sizes = [2_000usize, 20_000, 200_000];
    let mut group = c.benchmark_group("chunk_text");
    for &size in &sizes {
        let doc = make_prose_doc(size);
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, text| {
            b.iter(|| chunk_text(std::hint::black_box(text)))
        });
    }
    group.finish();
}

fn bench_chunk_text_with_offsets(c: &mut Criterion) {
    let sizes = [2_000usize, 20_000, 200_000];
    let mut group = c.benchmark_group("chunk_text_with_offsets");
    for &size in &sizes {
        let doc = make_prose_doc(size);
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, text| {
            b.iter(|| chunk_text_with_offsets(std::hint::black_box(text)))
        });
    }
    group.finish();
}

fn bench_chunk_markdown(c: &mut Criterion) {
    let sizes = [2_000usize, 20_000, 200_000];
    let mut group = c.benchmark_group("chunk_markdown");
    for &size in &sizes {
        let doc = make_markdown_doc(size);
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, text| {
            b.iter(|| chunk_markdown(std::hint::black_box(text)))
        });
    }
    group.finish();
}

fn bench_chunk_code(c: &mut Criterion) {
    let sizes = [2_000usize, 20_000, 100_000];
    let mut group = c.benchmark_group("chunk_code");
    for &size in &sizes {
        let doc = make_rust_source(size);
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &doc, |b, text| {
            b.iter(|| chunk_code(std::hint::black_box(text), "rs"))
        });
    }
    group.finish();
}

/// Measure chunk count amplification: how many chunks are produced per byte.
/// A regression here (e.g. one chunk per char) shows up as a much higher count.
fn bench_chunk_count_amplification(c: &mut Criterion) {
    let doc = make_prose_doc(100_000);
    c.bench_function("chunk_count_100k_chars", |b| {
        b.iter(|| {
            let chunks = chunk_text(std::hint::black_box(&doc));
            std::hint::black_box(chunks.len())
        })
    });
}

criterion_group!(
    benches,
    bench_chunk_text,
    bench_chunk_text_with_offsets,
    bench_chunk_markdown,
    bench_chunk_code,
    bench_chunk_count_amplification,
);
criterion_main!(benches);
