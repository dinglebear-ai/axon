//! Safe diagnostic vocabulary for rejected vector chunks, never matched values.
use super::*;

pub(super) fn warnings(total: u64, by_item: &RedactionSkipsBySourceItem) -> Vec<SourceWarning> {
    let mut warnings = Vec::new();
    let mut attributed = 0_u64;
    for (source_item_key, reasons) in by_item {
        let count = reasons.values().copied().fold(0_u64, u64::saturating_add);
        attributed = attributed.saturating_add(count);
        let reason_counts: Vec<_> = reasons
            .iter()
            .map(|(reason, count)| {
                serde_json::json!({
                    "field": reason.field, "detector": reason.detector, "count": count,
                })
            })
            .collect();
        warnings.push(warning(
            count,
            Some(source_item_key.clone()),
            serde_json::Value::Array(reason_counts).to_string(),
        ));
    }
    if total > attributed {
        warnings.push(warning(total - attributed, None, "[]".into()));
    }
    warnings
}

fn warning(count: u64, source_item_key: Option<SourceItemKey>, reasons: String) -> SourceWarning {
    SourceWarning {
        code: "source.vectorize.redaction_skipped_chunks".into(),
        severity: Severity::Warning,
        message: format!(
            "skipped {count} chunk(s) with secret-redaction-forbidden payload values; reason_counts={reasons} (not indexed; reduced vector point count accordingly)"
        ),
        source_item_key,
        retryable: false,
    }
}
