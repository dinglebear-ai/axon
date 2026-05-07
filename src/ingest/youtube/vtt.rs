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
mod tests {
    use super::*;

    #[test]
    fn parse_vtt_strips_header_and_timestamps() {
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello world\n\n00:00:02.000 --> 00:00:04.000\nThis is a test\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Hello world\nThis is a test");
    }

    #[test]
    fn parse_vtt_deduplicates_overlapping_lines() {
        // VTT often repeats the same line as the window shifts
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello world\n\n00:00:01.000 --> 00:00:03.000\nHello world\n\n00:00:03.000 --> 00:00:05.000\nNext sentence\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Hello world\nNext sentence");
    }

    #[test]
    fn parse_vtt_handles_empty_input() {
        assert_eq!(parse_vtt_to_text("WEBVTT\n\n"), "");
    }

    #[test]
    fn parse_vtt_handles_position_cues() {
        // VTT cues with position/alignment metadata
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000 align:start position:0%\nPositioned line\n\n00:00:02.000 --> 00:00:04.000\nNormal line\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Positioned line\nNormal line");
    }

    #[test]
    fn parse_vtt_strips_html_tags() {
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n<c>Tagged</c> text\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Tagged text");
    }

    #[test]
    fn parse_vtt_strips_numeric_cue_ids() {
        let vtt = "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\nHello world\n\n2\n00:00:02.000 --> 00:00:04.000\nSecond line\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Hello world\nSecond line");
    }

    #[test]
    fn parse_vtt_keeps_lines_with_digits_and_text() {
        // A legitimate line containing digits mixed with text should NOT be stripped
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n3 blind mice\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "3 blind mice");
    }

    #[test]
    fn parse_vtt_keeps_numeric_only_content() {
        // A line that is purely numeric (e.g. "2024") is transcript content, not a cue ID,
        // once a timestamp has been seen — it must be preserved.
        let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\n2024\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "2024");
    }

    #[test]
    fn parse_vtt_strips_string_cue_ids() {
        // String (non-numeric) cue identifiers like "intro-cue" must also be stripped
        let vtt = "WEBVTT\n\nintro-cue\n00:00:00.000 --> 00:00:02.000\nHello\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn parse_vtt_strips_note_blocks() {
        // NOTE blocks are metadata and must not appear in transcript
        let vtt = "WEBVTT\n\nNOTE This is a comment\nstill part of the note\n\n00:00:00.000 --> 00:00:02.000\nReal content\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Real content");
    }

    #[test]
    fn parse_vtt_strips_style_blocks() {
        let vtt = "WEBVTT\n\nSTYLE\n::cue { color: red; }\n\n00:00:00.000 --> 00:00:02.000\nStyling test\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Styling test");
    }

    #[test]
    fn parse_vtt_handles_webvtt_with_title() {
        // WEBVTT header may include a title after a space
        let vtt = "WEBVTT - My Video Transcript\n\n00:00:00.000 --> 00:00:02.000\nContent here\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "Content here");
    }

    #[test]
    fn parse_vtt_handles_bom() {
        // Some tools prepend a UTF-8 BOM to the WEBVTT header
        let vtt = "\u{FEFF}WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nBOM content\n";
        let text = parse_vtt_to_text(vtt);
        assert_eq!(text, "BOM content");
    }
}
