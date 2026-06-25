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
    files.push(workspace_root.join("crates/axon-cli/src/commands/scrape.rs"));
    files.push(workspace_root.join("crates/axon-services/src/memory.rs"));
    files.push(workspace_root.join("crates/axon-services/src/scrape.rs"));
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
        if is_allowed(&file) {
            continue;
        }
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        for needle in forbidden {
            if content.contains(needle) {
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

fn is_allowed(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized.ends_with("src/vector/ops/source_doc.rs")
        || normalized.ends_with("src/vector/ops/tei.rs")
        || normalized.contains("_tests.rs")
}
