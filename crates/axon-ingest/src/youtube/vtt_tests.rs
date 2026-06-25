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
    let vtt =
        "WEBVTT\n\nSTYLE\n::cue { color: red; }\n\n00:00:00.000 --> 00:00:02.000\nStyling test\n";
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
