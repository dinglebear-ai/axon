use super::PreparedDoc;
use axon_core::config::Config;
use axon_core::logging::log_warn;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub(super) struct ChunkVolumeLimits {
    pub(super) max_chunks_per_doc: Option<usize>,
    pub(super) max_source_chunks_per_doc: Option<usize>,
    pub(super) dedupe_exact_chunks: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ChunkVolumeGuardReport {
    pub(super) docs_deduped: usize,
    pub(super) docs_capped: usize,
    pub(super) duplicate_chunks_removed: usize,
    pub(super) chunks_removed_by_cap: usize,
}

pub(super) struct ChunkVolumeGuardOutcome {
    pub(super) doc: Option<PreparedDoc>,
    pub(super) report: ChunkVolumeGuardReport,
}

pub(super) fn chunk_volume_limits_from_config(cfg: &Config) -> ChunkVolumeLimits {
    ChunkVolumeLimits {
        max_chunks_per_doc: cfg.embed_max_chunks_per_doc,
        max_source_chunks_per_doc: cfg.embed_max_source_chunks_per_doc,
        dedupe_exact_chunks: cfg.embed_dedupe_exact_chunks,
    }
}

#[cfg(test)]
pub(super) fn chunk_volume_limits_from_values(
    max_chunks_per_doc: Option<&str>,
    max_source_chunks_per_doc: Option<&str>,
    dedupe_exact_chunks: Option<&str>,
) -> ChunkVolumeLimits {
    let max_chunks_per_doc = optional_usize_value(max_chunks_per_doc).unwrap_or(None);
    let max_source_chunks_per_doc = optional_usize_value(max_source_chunks_per_doc).unwrap_or(None);
    let dedupe_exact_chunks = dedupe_exact_chunks
        .map(is_truthy_default_true)
        .unwrap_or(true);
    ChunkVolumeLimits {
        max_chunks_per_doc,
        max_source_chunks_per_doc,
        dedupe_exact_chunks,
    }
}

#[cfg(test)]
fn optional_usize_value(value: Option<&str>) -> Option<Option<usize>> {
    value
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map(|parsed| match parsed {
            0 => None,
            value => Some(value.clamp(1, 100_000)),
        })
}

#[cfg(test)]
fn is_truthy_default_true(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn enforce_chunk_volume_limits(
    doc: PreparedDoc,
    limits: ChunkVolumeLimits,
) -> Option<PreparedDoc> {
    enforce_chunk_volume_limits_with_report(doc, limits).doc
}

pub(super) fn enforce_chunk_volume_limits_with_report(
    mut doc: PreparedDoc,
    limits: ChunkVolumeLimits,
) -> ChunkVolumeGuardOutcome {
    let mut report = ChunkVolumeGuardReport::default();
    if limits.dedupe_exact_chunks && doc.chunks.len() > 1 {
        let original_len = doc.chunks.len();
        let mut seen = HashSet::with_capacity(original_len);
        let chunks = std::mem::take(&mut doc.chunks);
        let chunk_extra_aligned = doc.chunk_extra.len() == original_len;
        let chunk_extra = if chunk_extra_aligned {
            std::mem::take(&mut doc.chunk_extra)
        } else {
            doc.chunk_extra.clear();
            Vec::new()
        };
        let point_ids_aligned = doc.chunk_point_ids.len() == original_len;
        let point_ids = if point_ids_aligned {
            std::mem::take(&mut doc.chunk_point_ids)
        } else {
            doc.chunk_point_ids.clear();
            Vec::new()
        };
        for (idx, chunk) in chunks.into_iter().enumerate() {
            if seen.insert(chunk.clone()) {
                doc.chunks.push(chunk);
                if chunk_extra_aligned {
                    doc.chunk_extra.push(chunk_extra[idx].clone());
                }
                if point_ids_aligned {
                    doc.chunk_point_ids.push(point_ids[idx]);
                }
            }
        }
        if doc.chunks.len() < original_len {
            report.docs_deduped = 1;
            report.duplicate_chunks_removed = original_len - doc.chunks.len();
            log_warn(&format!(
                "command=embed dedupe_exact_chunks url={} before={} after={}",
                doc.url,
                original_len,
                doc.chunks.len()
            ));
        }
    }

    let (max_chunks, cap_env) = max_chunks_for_doc(&doc, limits);
    if let Some(max_chunks) = max_chunks
        && doc.chunks.len() > max_chunks
    {
        let original_len = doc.chunks.len();
        doc.chunks.truncate(max_chunks);
        if !doc.chunk_extra.is_empty() {
            doc.chunk_extra.truncate(max_chunks);
        }
        if !doc.chunk_point_ids.is_empty() {
            doc.chunk_point_ids.truncate(max_chunks);
        }
        report.docs_capped = 1;
        report.chunks_removed_by_cap = original_len - max_chunks;
        log_warn(&format!(
            "command=embed cap_chunks_per_doc url={} before={} after={} env={}",
            doc.url, original_len, max_chunks, cap_env
        ));
    }

    if doc.chunks.is_empty() {
        ChunkVolumeGuardOutcome { doc: None, report }
    } else {
        ChunkVolumeGuardOutcome {
            doc: Some(doc),
            report,
        }
    }
}

fn max_chunks_for_doc(
    doc: &PreparedDoc,
    limits: ChunkVolumeLimits,
) -> (Option<usize>, &'static str) {
    if is_source_like_doc(doc) {
        (
            limits.max_source_chunks_per_doc,
            "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        )
    } else {
        (limits.max_chunks_per_doc, "AXON_EMBED_MAX_CHUNKS_PER_DOC")
    }
}

fn is_source_like_doc(doc: &PreparedDoc) -> bool {
    doc.content_type == "text"
        && (matches!(
            doc.source_type.as_str(),
            "github" | "gitlab" | "gitea" | "forgejo" | "git" | "generic_git"
        ) || doc
            .extra
            .as_ref()
            .is_some_and(|extra| extra.get("code_file_path").is_some())
            || doc.chunk_extra.iter().any(|extra| {
                extra
                    .get("chunk_content_kind")
                    .and_then(|value| value.as_str())
                    == Some("code")
            }))
}
