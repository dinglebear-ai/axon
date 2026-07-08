use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn source_doc_audit_forbids_manual_chunking_in_adapters() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_root
        .parent()
        .and_then(Path::parent)
        .expect("axon-vector lives under crates/");
    let mut files = collect_files(&workspace_root.join("crates/axon-ingest/src"));
    files.extend(collect_files(
        &workspace_root.join("crates/axon-document/src"),
    ));
    // `scrape.rs` was removed from axon-cli's command surface at the Phase 10
    // clean-break cutover (embed/ingest/scrape/crawl/code-search); the unified
    // `axon source` command in `source.rs` is its replacement.
    files.push(workspace_root.join("crates/axon-cli/src/commands/source.rs"));
    files.push(workspace_root.join("crates/axon-services/src/memory.rs"));
    files.push(workspace_root.join("crates/axon-services/src/scrape.rs"));
    files.push(workspace_root.join("crates/axon-vector/src/ops/source_doc.rs"));
    files.push(workspace_root.join("crates/axon-vector/src/ops/tei/prepare.rs"));
    files.push(workspace_root.join("crates/axon-web/src/server/handlers/rest/sync_post.rs"));

    let forbidden = [
        "use crate::ops::{chunk_text",
        "use crate::ops::{chunk_markdown",
        "use crate::ops::{chunk_file",
        "use crate::ops::chunk_text",
        "use crate::ops::chunk_markdown",
        "use crate::ops::chunk_file",
        "crate::ops::chunk_text(",
        "crate::ops::chunk_markdown(",
        "crate::ops::chunk_file(",
        "crate::ops::input::chunk_text(",
        "crate::ops::input::chunk_markdown(",
        "crate::ops::file_ingest::chunk_file(",
        "PreparedDoc {",
        "PreparedDoc::ingest(",
        "PreparedDoc::from_planned_chunks(",
        "crate::ops::tei::PreparedDoc::from_planned_chunks(",
        "build_point(",
        "qdrant_upsert(",
    ];

    let mut violations = Vec::new();
    for file in files {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        for needle in forbidden {
            if content.contains(needle) && !is_allowed_violation(&file, needle, &content) {
                violations.push(format!("{} contains {needle}", file.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "source document audit violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn source_doc_adapter_declares_axon_document_bridge() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_doc_path = crate_root.join("src/ops/source_doc.rs");
    let source_doc = fs::read_to_string(&source_doc_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", source_doc_path.display()));
    let bridge_path = crate_root.join("src/ops/source_doc/document_bridge.rs");
    let bridge = fs::read_to_string(&bridge_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", bridge_path.display()));
    let manifest_path = crate_root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));

    assert!(
        manifest.contains("axon-document"),
        "axon-vector must declare the axon-document preparation boundary"
    );
    assert!(
        source_doc.contains("prepare_atomic_source"),
        "source_doc should dispatch safely-owned preparation through its bridge module"
    );
    assert!(
        bridge.contains("DocumentPreparer::default().prepare"),
        "source_doc bridge should call axon-document"
    );
    assert!(
        bridge.contains("ChunkingProfile::AtomicMetadata"),
        "memory/atomic source preparation should use axon-document's atomic profile"
    );
}

fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_file() {
            if path.extension().and_then(|v| v.to_str()) == Some("rs") {
                out.push(path);
            }
            continue;
        }
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            stack.push(entry.path());
        }
    }
    out
}

fn is_allowed_violation(path: &Path, needle: &str, content: &str) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.contains("_tests.rs") {
        return true;
    }
    if normalized.ends_with("crates/axon-vector/src/ops/tei.rs") {
        return true;
    }
    normalized.ends_with("crates/axon-vector/src/ops/source_doc.rs")
        && matches!(
            needle,
            "PreparedDoc {" | "PreparedDoc::from_planned_chunks("
        )
        && content.contains("TODO(PR8/#298)")
}
