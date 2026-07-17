use super::*;
use crate::reset::planning::physical_chunk_checksum;

pub(super) fn validate_resumable_inventory(
    saved: &ResetResult,
    current: &PreparedReset,
    receipt: Option<&ResetReceipt>,
) -> Result<(), Box<dyn Error>> {
    let Some(receipt) = receipt else {
        if current.inventory_checksum != saved.inventory_checksum {
            return Err("reset.inventory_changed: create and review a new reset plan".into());
        }
        return Ok(());
    };
    if receipt.plan_id != saved.plan_id || receipt.reset_id != saved.reset_id {
        return Err("reset.receipt_plan_mismatch: refusing to resume another plan".into());
    }
    let expected_chunks = execution::planned_chunks(&saved.stores);
    if receipt.chunks.len() != expected_chunks.len()
        || receipt
            .chunks
            .iter()
            .zip(&expected_chunks)
            .any(|(actual, expected)| {
                actual.chunk_id != expected.chunk_id || actual.store != expected.store
            })
    {
        return Err("reset.receipt_shape_invalid: chunk inventory does not match plan".into());
    }

    for chunk in &receipt.chunks {
        match chunk.status.as_str() {
            "completed" => validate_completed_chunk(current, chunk)?,
            "pending" => {
                if physical_chunk_checksum(&current.plan, &chunk.store)
                    != physical_chunk_checksum(&saved.plan, &chunk.store)
                {
                    return Err(format!(
                        "reset.inventory_changed: pending chunk {} changed since review",
                        chunk.store
                    )
                    .into());
                }
            }
            "failed" => validate_failed_chunk(saved, current, chunk)?,
            status => {
                return Err(format!(
                    "reset.receipt_shape_invalid: unknown chunk status {status:?}"
                )
                .into());
            }
        }
    }
    Ok(())
}

fn validate_failed_chunk(
    saved: &ResetResult,
    current: &PreparedReset,
    chunk: &ResetChunkReceipt,
) -> Result<(), Box<dyn Error>> {
    let within_reviewed_bound = match chunk.store.as_str() {
        "sqlite" => current.sqlite_inv.content_rows <= saved.estimates.sqlite_rows,
        RESET_STORE_VECTORS => current.qdrant_inv.as_ref().is_some_and(|inventory| {
            !inventory.unreachable && inventory.points <= saved.estimates.qdrant_points
        }),
        RESET_STORE_ARTIFACTS => {
            artifacts::count_files(&current.artifact_root) as u64 <= saved.estimates.artifact_files
        }
        _ => false,
    };
    if !within_reviewed_bound {
        return Err(format!(
            "reset.inventory_changed: failed chunk {} expanded beyond reviewed impact",
            chunk.store
        )
        .into());
    }
    Ok(())
}

fn validate_completed_chunk(
    current: &PreparedReset,
    chunk: &ResetChunkReceipt,
) -> Result<(), Box<dyn Error>> {
    match chunk.store.as_str() {
        "sqlite" => {
            let expected_checksum = checkpoint_value(&chunk.checkpoint, "schema_checksum")
                .ok_or("reset.receipt_invalid: sqlite checkpoint lacks schema checksum")?;
            if current.sqlite_inv.content_rows != 0
                || current.sqlite_inv.schema_identity.checksum != expected_checksum
            {
                return Err(
                    "reset.completed_chunk_changed: sqlite postcondition no longer holds".into(),
                );
            }
        }
        RESET_STORE_VECTORS => {
            let inventory = current
                .qdrant_inv
                .as_ref()
                .ok_or("reset.receipt_invalid: vectors checkpoint lacks inventory")?;
            if inventory.unreachable || inventory.points != 0 {
                return Err(
                    "reset.completed_chunk_changed: vector postcondition no longer holds".into(),
                );
            }
            if checkpoint_value(&chunk.checkpoint, "created") == Some("true") && !inventory.exists {
                return Err(
                    "reset.completed_chunk_changed: recreated collection is missing".into(),
                );
            }
        }
        RESET_STORE_ARTIFACTS => {
            if artifacts::count_files(&current.artifact_root) != 0 {
                return Err(
                    "reset.completed_chunk_changed: artifact postcondition no longer holds".into(),
                );
            }
        }
        _ => return Err(format!("reset.receipt_invalid: unknown chunk {}", chunk.store).into()),
    }
    Ok(())
}

fn checkpoint_value<'a>(checkpoint: &'a str, key: &str) -> Option<&'a str> {
    checkpoint.split(';').find_map(|field| {
        let (field_key, value) = field.split_once('=')?;
        (field_key == key).then_some(value)
    })
}
