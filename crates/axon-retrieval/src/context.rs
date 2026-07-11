//! Context budget helpers for the retrieval boundary fake.

use axon_api::source::ChunkId;

use crate::query::RetrievalMatch;

pub const MODULE_NAME: &str = "context";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundle {
    pub text: String,
    pub chunk_ids: Vec<ChunkId>,
    pub bytes_used: u64,
    pub token_estimate: u32,
    pub truncated: bool,
}

impl ContextBundle {
    pub(crate) fn from_chunks(
        chunks: Vec<(ChunkId, String)>,
        byte_budget: u64,
        token_budget: u32,
    ) -> Self {
        Self::from_items(
            chunks.into_iter().map(|(chunk_id, text)| (chunk_id, text)),
            byte_budget,
            token_budget,
        )
    }

    pub(crate) fn from_matches(
        matches: &[RetrievalMatch],
        byte_budget: u64,
        token_budget: u32,
    ) -> Self {
        Self::from_items(
            matches
                .iter()
                .map(|item| (item.chunk_id.clone(), item.text.clone())),
            byte_budget,
            token_budget,
        )
    }

    fn from_items(
        items: impl IntoIterator<Item = (ChunkId, String)>,
        byte_budget: u64,
        token_budget: u32,
    ) -> Self {
        let mut text_parts = Vec::new();
        let mut chunk_ids = Vec::new();
        let mut bytes_used = 0_u64;
        let mut token_estimate = 0_u32;
        let mut truncated = false;

        for (chunk_id, text) in items {
            let text = defang_chunk_text(&text);
            let separator_bytes = if text_parts.is_empty() { 0 } else { 2 };
            let next_bytes_used = bytes_used + separator_bytes + text.len() as u64;
            let next_token_estimate = estimate_tokens(next_bytes_used);
            if next_bytes_used > byte_budget || next_token_estimate > token_budget {
                truncated = true;
                break;
            }
            bytes_used = next_bytes_used;
            token_estimate = next_token_estimate;
            chunk_ids.push(chunk_id);
            text_parts.push(text);
        }

        Self {
            text: text_parts.join("\n\n"),
            chunk_ids,
            bytes_used,
            token_estimate,
            truncated,
        }
    }
}

fn estimate_tokens(bytes: u64) -> u32 {
    bytes.div_ceil(4).try_into().unwrap_or(u32::MAX)
}

fn defang_chunk_text(text: &str) -> String {
    let text = text
        .replace("## Sources", "## \u{200b}Sources")
        .replace("## Source Document", "## \u{200b}Source Document")
        .replace("## Top Chunk", "## \u{200b}Top Chunk")
        .replace("## Supplemental Chunk", "## \u{200b}Supplemental Chunk");
    defang_citation_patterns(&text)
}

fn defang_citation_patterns(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(pos) = rest.find("[S") {
        result.push_str(&rest[..pos]);
        let tail = &rest[pos + 2..];
        let digit_end = tail.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digit_end > 0 && tail[digit_end..].starts_with(']') {
            result.push_str("[\u{200b}S");
            result.push_str(&tail[..digit_end]);
            result.push(']');
            rest = &tail[digit_end + 1..];
        } else {
            result.push_str("[S");
            rest = tail;
        }
    }
    result.push_str(rest);
    result
}
