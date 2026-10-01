//! Collapse complete, byte-identical file inventories before preparation.
//! Oversized alias groups remain individual items: paths are never truncated.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::adapter::Result;
use axon_api::source::{ApiError, ErrorStage, ManifestItem};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::hex_prefix;

const MAX_ALIAS_COUNT: usize = 256;
const MAX_ALIAS_METADATA_BYTES: usize = 32 * 1024;

pub(super) fn deduplicate_items(items: Vec<ManifestItem>) -> Result<Vec<ManifestItem>> {
    let mut groups = BTreeMap::<String, Vec<ManifestItem>>::new();
    let mut output = Vec::new();
    for item in items {
        let Some(hash) = &item.content_hash else {
            output.push(with_prefixes(item)?);
            continue;
        };
        // Keep exact extensions separate: JSX/TSX and shell dialects can select
        // different parsers even when their broad content kind is identical.
        let path = item
            .display_path
            .as_deref()
            .unwrap_or(&item.source_item_key.0);
        let extension = Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_test = axon_parse::code_path::is_test_path(Some(path));
        let directory_parser = path.ends_with(".devcontainer/devcontainer.json");
        // Non-Markdown parsers and chunk routing can depend on the basename
        // (Compose, environment, schema and session files). Preserve it.
        let basename = if matches!(extension.as_str(), "md" | "markdown" | "mdown") {
            ""
        } else {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
        };
        let key = format!(
            "{hash}\0{:?}\0{extension}\0{is_test}\0{directory_parser}\0{basename}",
            item.content_kind
        );
        groups.entry(key).or_default().push(item);
    }
    for (_, mut group) in groups {
        group.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
        if group.len().saturating_sub(1) > MAX_ALIAS_COUNT {
            for item in group {
                output.push(with_prefixes(item)?);
            }
            continue;
        }
        let paths: Vec<_> = group
            .iter()
            .skip(1)
            .map(|item| item.source_item_key.0.clone())
            .collect();
        let uris: Vec<_> = group
            .iter()
            .skip(1)
            .map(|item| item.canonical_uri.clone())
            .collect();
        let group_paths: Vec<_> = group
            .iter()
            .map(|item| item.source_item_key.0.as_str())
            .collect();
        let Some(prefixes) = path_prefixes(&group_paths) else {
            for item in group {
                output.push(with_prefixes(item)?);
            }
            continue;
        };
        let bytes = serde_json::to_vec(&(&paths, &uris, &prefixes))
            .expect("string arrays serialize")
            .len();
        if paths.len() > MAX_ALIAS_COUNT || bytes > MAX_ALIAS_METADATA_BYTES {
            for item in group {
                output.push(with_prefixes(item)?);
            }
            continue;
        }
        let mut canonical = group.remove(0);
        // A single remaining file must also carry a membership version, so
        // removing its final alias invalidates prior prepared-vector reuse.
        let mut hasher = Sha256::new();
        hasher.update(b"git-file-aliases-v2\0");
        hasher.update(canonical.source_item_key.0.as_bytes());
        for (path, uri) in paths.iter().zip(&uris) {
            hasher.update([0]);
            hasher.update(path.as_bytes());
            hasher.update([0]);
            hasher.update(uri.as_bytes());
        }
        canonical.version = Some(format!(
            "git-aliases-v2:{}",
            hex_prefix(&hasher.finalize(), 64)
        ));
        if !paths.is_empty() {
            canonical
                .metadata
                .insert("source_item_aliases".to_string(), json!(paths));
            canonical
                .metadata
                .insert("item_canonical_uri_aliases".to_string(), json!(uris));
        }
        canonical
            .metadata
            .insert("source_path_prefixes".into(), json!(prefixes));
        output.push(canonical);
    }
    output.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(output)
}

fn path_prefixes(paths: &[&str]) -> Option<BTreeSet<String>> {
    let mut prefixes = BTreeSet::from(["/".to_string()]);
    let mut bytes = 5usize;
    for path in paths {
        let candidates = std::iter::once(*path).chain(
            path.match_indices('/')
                .filter(|(index, _)| *index > 0)
                .map(|(index, _)| &path[..index]),
        );
        for prefix in candidates {
            if !prefixes.contains(prefix) {
                bytes = bytes.saturating_add(
                    serde_json::to_vec(prefix).expect("strings serialize").len() + 1,
                );
                if bytes > MAX_ALIAS_METADATA_BYTES {
                    return None;
                }
                prefixes.insert(prefix.to_string());
            }
        }
    }
    Some(prefixes)
}

fn with_prefixes(mut item: ManifestItem) -> Result<ManifestItem> {
    let prefixes = path_prefixes(&[&item.source_item_key.0]).ok_or_else(|| ApiError::new("adapter.git.path_prefix_budget_exceeded", ErrorStage::Discovering,
            "repository path ancestors exceed the 32 KiB lookup metadata budget; shorten the affected path or exclude it before retrying discovery")
            .with_context("source_item_key", item.source_item_key.0.clone()))?;
    item.metadata
        .insert("source_path_prefixes".into(), json!(prefixes));
    Ok(item)
}
