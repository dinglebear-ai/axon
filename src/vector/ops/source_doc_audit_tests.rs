use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn source_doc_audit_forbids_manual_chunking_in_adapters() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = collect_files(&root.join("src/ingest"));
    files.push(root.join("src/services/scrape.rs"));
    files.push(root.join("src/vector/ops/tei/prepare.rs"));

    let forbidden = [
        "use crate::vector::ops::{chunk_text",
        "use crate::vector::ops::{chunk_markdown",
        "use crate::vector::ops::{chunk_file",
        "PreparedDoc {",
        "PreparedDoc::ingest(",
        "PreparedDoc::from_planned_chunks(",
    ];

    let mut violations = Vec::new();
    for file in files {
        if is_allowed(&file) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
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
