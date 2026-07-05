use axon_api::source::SourceRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRangeBounds {
    pub line_count: u32,
    pub byte_len: u64,
    pub char_count: u64,
}

pub fn bounds_for_text(text: &str) -> SourceRangeBounds {
    SourceRangeBounds {
        line_count: text.lines().count().max(1) as u32,
        byte_len: text.len() as u64,
        char_count: text.chars().count() as u64,
    }
}

pub fn validate_source_range(
    range: &SourceRange,
    bounds: &SourceRangeBounds,
) -> Result<(), String> {
    if let (Some(start), Some(end)) = (range.line_start, range.line_end)
        && (start > end || end > bounds.line_count)
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    if let (Some(start), Some(end)) = (range.byte_start, range.byte_end)
        && (start > end || end > bounds.byte_len)
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    if let (Some(start), Some(end)) = (range.char_start, range.char_end)
        && (start > end || end > bounds.char_count)
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    Ok(())
}
