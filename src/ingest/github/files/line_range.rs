/// Compute line range (1-indexed, inclusive) for a chunk at a known byte offset
/// within content.
///
/// Uses the caller-provided `byte_offset` (from chunking) as the source of truth
/// rather than searching for the chunk text, which would always match the first
/// occurrence for duplicate chunks and produce wrong line ranges.
pub(super) fn line_range_for_chunk(content: &str, chunk: &str, byte_offset: usize) -> (u32, u32) {
    let clamped = byte_offset.min(content.len());
    // Lines before this chunk (1-indexed).
    let start_line = content[..clamped].bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let lines_in_chunk = chunk.bytes().filter(|&b| b == b'\n').count() as u32;
    let end_line = start_line + lines_in_chunk;
    (start_line, end_line)
}

#[cfg(test)]
#[path = "line_range_tests.rs"]
mod tests;
