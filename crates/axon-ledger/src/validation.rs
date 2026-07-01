use std::collections::BTreeSet;

use axon_api::source::*;

use crate::store::Result;

pub(crate) fn validate_manifest(manifest: &SourceManifest) -> Result<()> {
    let mut item_keys = BTreeSet::new();
    for item in &manifest.items {
        if item.source_id != manifest.source_id {
            return Err(ApiError::new(
                "source.ledger.manifest_item_source_mismatch",
                ErrorStage::Planning,
                format!(
                    "manifest item {} belongs to source {}, not {}",
                    item.source_item_key.0, item.source_id.0, manifest.source_id.0
                ),
            )
            .with_source_id(manifest.source_id.0.clone()));
        }
        if !item_keys.insert(item.source_item_key.clone()) {
            return Err(ApiError::new(
                "source.ledger.manifest_duplicate_item",
                ErrorStage::Planning,
                format!(
                    "manifest for generation {} contains duplicate item key {}",
                    manifest.generation.0, item.source_item_key.0
                ),
            )
            .with_source_id(manifest.source_id.0.clone()));
        }
    }
    Ok(())
}
