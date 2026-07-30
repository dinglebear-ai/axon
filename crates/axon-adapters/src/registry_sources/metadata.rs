//! Markdown rendering and `SourceDocument` construction for registry
//! packages. The adapter emits the canonical package metadata shape directly;
//! the shared pipeline does not rewrite registry documents after normalize.

use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::dump::{RegistryDump, RegistryDumpVersion};

pub(super) fn package_markdown(dump: &RegistryDump, version: &RegistryDumpVersion) -> String {
    let mut md = format!("# {}@{}\n\n", dump.package, version.version);
    let description = version
        .description
        .as_deref()
        .filter(|text| !text.is_empty())
        .or(dump.description.as_deref());
    if let Some(description) = description {
        md.push_str(description);
        md.push_str("\n\n");
    }
    if let Some(author) = dump.author.as_deref().filter(|value| !value.is_empty()) {
        md.push_str(&format!("**Author:** {author}\n"));
    }
    if let Some(license) = dump.license.as_deref().filter(|value| !value.is_empty()) {
        md.push_str(&format!("**License:** {license}\n"));
    }
    if let Some(homepage) = dump.homepage.as_deref().filter(|value| !value.is_empty()) {
        md.push_str(&format!("**Homepage:** {homepage}\n"));
    }
    if !dump.keywords.is_empty() {
        md.push_str(&format!("**Keywords:** {}\n", dump.keywords.join(", ")));
    }
    md.push_str(&format!("\n**Registry:** {}\n", dump.registry));
    if let Some(readme) = version.readme.as_deref().filter(|text| !text.is_empty()) {
        md.push_str("\n## README\n\n");
        md.push_str(readme);
        md.push('\n');
    }
    md
}

pub(super) fn package_metadata(
    plan: &SourcePlan,
    dump: &RegistryDump,
    version: &RegistryDumpVersion,
) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("package"));
    metadata.insert("source_kind".to_string(), json!("registry"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert("package_ecosystem".to_string(), json!(dump.registry));
    metadata.insert("package_name".to_string(), json!(dump.package));
    metadata.insert("package_version".to_string(), json!(version.version));
    if let Some(license) = dump.license.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("package_license".to_string(), json!(license));
    }
    if let Some(author) = dump.author.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("package_author".to_string(), json!(author));
    }
    if !dump.keywords.is_empty() {
        metadata.insert("package_keywords".to_string(), json!(dump.keywords));
    }
    if let Some(homepage) = dump.homepage.as_deref().filter(|value| !value.is_empty()) {
        metadata.insert("package_homepage".to_string(), json!(homepage));
    }
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    metadata
}

pub(super) fn registry_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_registry_{}",
        stable_token(&format!("{}\0{}", source_id.0, item_key.0))
    ))
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

#[cfg(test)]
#[path = "../registry_sources_metadata_tests.rs"]
mod tests;
