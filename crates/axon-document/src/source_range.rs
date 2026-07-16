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
    validate_bound("line_start", range.line_start, bounds.line_count)?;
    validate_bound("line_end", range.line_end, bounds.line_count)?;
    validate_bound("byte_start", range.byte_start, bounds.byte_len)?;
    validate_bound("byte_end", range.byte_end, bounds.byte_len)?;
    validate_bound("char_start", range.char_start, bounds.char_count)?;
    validate_bound("char_end", range.char_end, bounds.char_count)?;

    if let (Some(start), Some(end)) = (range.line_start, range.line_end)
        && start > end
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    if let (Some(start), Some(end)) = (range.byte_start, range.byte_end)
        && start > end
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    if let (Some(start), Some(end)) = (range.char_start, range.char_end)
        && start > end
    {
        return Err("invalid source range outside normalized document".to_string());
    }
    if range
        .time_start_ms
        .zip(range.time_end_ms)
        .is_some_and(|(start, end)| start > end)
    {
        return Err("invalid source range time_start_ms > time_end_ms".to_string());
    }
    if range
        .turn_start
        .as_ref()
        .zip(range.turn_end.as_ref())
        .is_some_and(|(start, end)| start > end)
    {
        return Err("invalid source range turn_start > turn_end".to_string());
    }
    Ok(())
}

fn validate_bound<T>(label: &str, value: Option<T>, max: T) -> Result<(), String>
where
    T: Copy + PartialOrd,
{
    if value.is_some_and(|value| value > max) {
        return Err(format!(
            "invalid source range outside normalized document: {label} exceeds bounds"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "source_range_tests.rs"]
mod tests;
