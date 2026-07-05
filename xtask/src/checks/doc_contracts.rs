//! `cargo xtask check-doc-contracts` — generated reference docs must not reference
//! removed public surfaces.
//!
//! The schema generator's removed-surface drift check (`schemas generate --check`)
//! matches JSON-quoted tokens against the generated **JSON** artifacts. It does not
//! look at the prose markdown under `docs/reference/**`. This check closes that gap
//! for the distinctive removed DTO **type names** (e.g. `EmbedRequest`): if one of
//! them reappears as a word in a generated markdown doc, that doc is stale and CI
//! fails. It is the `check-doc-contracts` deliverable from
//! `docs-generator-contract.md` ("generated doc references removed surfaces").
//!
//! Only removed API DTO definition names are used here. The command/route/config
//! tokens (`"embed"`, `/v1/scrape`, …) are ordinary words in prose and are
//! intentionally excluded to avoid false positives — they remain covered against
//! the JSON artifacts by the schema generator.

use anyhow::{Result, bail};
use std::path::Path;

use walkdir::WalkDir;

/// Removed type-name tokens that must not appear in generated markdown. The
/// source list is shared with the API schema removed-definition check so the two
/// guardrails cannot drift apart.
pub fn removed_doc_type_tokens() -> Vec<String> {
    axon_api::schema_registry::removed_dto_names()
        .iter()
        .map(|token| (*token).to_string())
        .collect()
}

/// A removed type name found in a generated markdown doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocContractViolation {
    pub source: String,
    pub token: String,
}

pub fn check(root: &Path) -> Result<()> {
    let docs_root = root.join("docs/reference");
    if !docs_root.is_dir() {
        println!("check-doc-contracts: docs/reference absent, skipping.");
        return Ok(());
    }
    let tokens = removed_doc_type_tokens();
    let mut violations = Vec::new();
    let mut checked = 0usize;
    for entry in WalkDir::new(&docs_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        checked += 1;
        let content = std::fs::read_to_string(path)?;
        for token in &tokens {
            if contains_word(&content, token) {
                violations.push(DocContractViolation {
                    source: path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned(),
                    token: token.clone(),
                });
            }
        }
    }
    if !violations.is_empty() {
        let mut msg = format!(
            "check-doc-contracts: {} removed-surface reference(s) in generated docs:\n",
            violations.len()
        );
        for v in &violations {
            msg.push_str(&format!(
                "  {} references removed `{}`\n",
                v.source, v.token
            ));
        }
        bail!(msg);
    }
    println!("check-doc-contracts: {checked} markdown file(s), no removed-surface references.");
    Ok(())
}

/// True when `needle` appears in `haystack` bounded by non-identifier characters
/// (so `EmbedRequest` does not match inside `MyEmbedRequestExt`).
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let n = needle.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while let Some(pos) = haystack[i..].find(needle) {
        let start = i + pos;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        i = start + n.len();
    }
    false
}

#[cfg(test)]
#[path = "doc_contracts_tests.rs"]
mod tests;
