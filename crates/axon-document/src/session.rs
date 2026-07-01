//! AI-session chunk builders.

use crate::chunk::DocumentChunk;
use crate::transcript::split_on_nonempty_lines;

pub fn session_turns(text: &str) -> Vec<DocumentChunk> {
    split_on_nonempty_lines(text, "session_turn")
        .into_iter()
        .enumerate()
        .map(|(idx, mut chunk)| {
            chunk.range.session_turn_id = Some(format!("turn-{idx}"));
            chunk
        })
        .collect()
}
