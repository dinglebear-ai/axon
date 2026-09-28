//! Job-local byte admission for filesystem acquisitions.
use super::*;
use axon_adapters::acquisition::{ACQUISITION_BATCH_BYTES_KEY, DEFAULT_ACQUISITION_BATCH_BYTES};

pub(super) fn file_backed(plan: &SourcePlan) -> bool {
    matches!(
        plan.route.source.source_kind,
        SourceKind::Git | SourceKind::Local
    )
}

pub(super) struct AcquisitionBudget {
    enabled: bool,
    remaining: Option<u64>,
    item_limit: u64,
}

impl AcquisitionBudget {
    pub(super) fn new(plan: &SourcePlan, preparation_bytes: usize) -> Self {
        Self {
            enabled: file_backed(plan),
            remaining: plan.limits.effective.max_total_bytes,
            item_limit: plan
                .limits
                .effective
                .max_bytes_per_item
                .unwrap_or(u64::MAX)
                .min(u64::try_from(preparation_bytes / 5).unwrap_or(u64::MAX)),
        }
    }

    pub(super) fn plan(&self, original: &SourcePlan) -> SourcePlan {
        let mut plan = original.clone();
        if self.enabled {
            plan.limits.effective.max_total_bytes = self.remaining;
            plan.limits.effective.max_bytes_per_item = Some(self.item_limit);
            plan.route.source.metadata.insert(
                ACQUISITION_BATCH_BYTES_KEY.to_owned(),
                serde_json::json!(DEFAULT_ACQUISITION_BATCH_BYTES),
            );
        }
        plan
    }

    pub(super) fn charge(&mut self, acquired: &SourceAcquisition) -> Result<(), ApiError> {
        if !self.enabled {
            return Ok(());
        }
        let consumed = acquired.header.counts.bytes_done;
        if consumed > DEFAULT_ACQUISITION_BATCH_BYTES {
            return Err(budget_error(
                "source.acquire.batch_bytes_exceeded",
                "acquisition exceeded the resident batch byte limit",
            ));
        }
        if let Some(remaining) = self.remaining {
            self.remaining = Some(remaining.checked_sub(consumed).ok_or_else(|| {
                budget_error(
                    "source.acquire.total_bytes_exceeded",
                    "acquisition exceeded the remaining job byte allowance",
                )
            })?);
        }
        Ok(())
    }
}

fn budget_error(code: &'static str, message: &'static str) -> ApiError {
    ApiError::new(code, ErrorStage::Fetching, message)
}

pub(super) fn changed_batches<'a>(
    plan: &SourcePlan,
    diff: &'a SourceManifestDiff,
    first_size: usize,
    size: usize,
) -> impl Iterator<Item = ChangedBatch> + 'a {
    let batches: Box<dyn Iterator<Item = SourceManifestDiff> + Send + 'a> = if file_backed(plan) {
        Box::new(file_batches(
            diff,
            first_size,
            size,
            DEFAULT_ACQUISITION_BATCH_BYTES,
        ))
    } else {
        Box::new(batch_changed_diff_ramped(diff, first_size, size))
    };
    let mut batches = batches.peekable();
    std::iter::from_fn(move || {
        let diff = batches.next()?;
        Some(ChangedBatch {
            diff,
            is_final: batches.peek().is_none(),
        })
    })
}

fn file_batches(
    diff: &SourceManifestDiff,
    first_size: usize,
    size: usize,
    byte_limit: u64,
) -> impl Iterator<Item = SourceManifestDiff> + '_ {
    let mut items = diff
        .added
        .iter()
        .map(|item| (true, item))
        .chain(diff.modified.iter().map(|item| (false, item)))
        .peekable();
    let mut first = true;
    std::iter::from_fn(move || {
        let target = if first { first_size } else { size }.max(1);
        first = false;
        let mut batch = empty_diff_like(diff);
        let mut bytes = 0_u64;
        let mut count = 0;
        while count < target {
            let Some((_, item)) = items.peek() else { break };
            let weight = item.size_bytes.unwrap_or(byte_limit).min(byte_limit);
            if count > 0 && (item.size_bytes.is_none() || weight > byte_limit.saturating_sub(bytes))
            {
                break;
            }
            let (added, item) = items.next().expect("peeked item");
            if added {
                batch.added.push(item.clone());
            } else {
                batch.modified.push(item.clone());
            }
            bytes = bytes.saturating_add(weight);
            count += 1;
            if item.size_bytes.is_none() || bytes >= byte_limit {
                break;
            }
        }
        if count == 0 {
            return None;
        }
        batch.counts.added = batch.added.len() as u64;
        batch.counts.modified = batch.modified.len() as u64;
        Some(batch)
    })
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
