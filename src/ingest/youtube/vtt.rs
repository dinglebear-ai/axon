/// Parse a WebVTT transcript string into clean plain text.
///
/// Strips the WEBVTT header (including BOM and optional title), timestamp lines,
/// cue identifiers (both numeric and string), NOTE/STYLE/REGION block directives,
/// position cues, and deduplicates consecutive identical lines that arise from
/// overlapping subtitle windows.
pub fn parse_vtt_to_text(vtt: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut last: Option<String> = None;
    // When true, we are inside a block directive (NOTE/STYLE/REGION) and skip
    // lines until a blank line terminates it.
    let mut in_block_directive = false;
    // When true, the next non-blank line is a cue body (not a cue identifier).
    // Set after we see a timestamp line ("-->").
    let mut next_is_cue_body = false;

    for raw_line in vtt.lines() {
        // Strip UTF-8 BOM on first line (\u{FEFF})
        let line = raw_line.trim_start_matches('\u{feff}');

        // Blank line — terminates any active block directive; resets cue-body flag
        if line.trim().is_empty() {
            in_block_directive = false;
            next_is_cue_body = false;
            continue;
        }

        // Skip lines inside NOTE/STYLE/REGION block directives
        if in_block_directive {
            continue;
        }

        // Detect block directive opening lines — only valid at block level, not inside a cue body
        let trimmed = line.trim();
        if !next_is_cue_body
            && (trimmed == "WEBVTT"
                || trimmed.starts_with("WEBVTT ")
                || trimmed.starts_with("NOTE")
                || trimmed.starts_with("STYLE")
                || trimmed.starts_with("REGION"))
        {
            in_block_directive = true;
            continue;
        }

        // Timestamp line — any line containing "-->"
        if line.contains("-->") {
            next_is_cue_body = true;
            continue;
        }

        // If we haven't seen a timestamp yet for this cue, the line is a cue identifier.
        // Cue identifiers may be numeric ("1") or string ("intro-cue") — skip them all.
        if !next_is_cue_body {
            continue;
        }

        // Strip HTML tags from content lines
        let mut clean = String::new();
        let mut inside_tag = false;
        for ch in line.chars() {
            match ch {
                '<' => inside_tag = true,
                '>' => inside_tag = false,
                _ if !inside_tag => clean.push(ch),
                _ => {}
            }
        }
        let clean = clean.trim().to_string();

        if clean.is_empty() {
            continue;
        }

        // Deduplicate consecutive identical lines
        if last.as_deref() == Some(&clean) {
            continue;
        }

        last = Some(clean.clone());
        result.push(clean);
    }

    result.join("\n")
}

#[cfg(test)]
#[path = "vtt_tests.rs"]
mod tests;
