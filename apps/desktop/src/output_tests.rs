use super::*;

#[test]
fn strip_ansi_csi_colours() {
    let input = "\x1b[1;31merror\x1b[0m: thing";
    assert_eq!(strip_ansi(input), "error: thing");
}

#[test]
fn strip_ansi_csi_with_punctuation_final_byte() {
    // CSI final byte may be punctuation such as `~` or `@`
    let input = "before\x1b[2~after";
    assert_eq!(strip_ansi(input), "beforeafter");
}

#[test]
fn strip_ansi_osc_bel_terminated() {
    // OSC 0; set window title <BEL>
    let input = "pre\x1b]0;hello world\x07post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_osc_st_terminated() {
    // OSC terminated by ST = ESC '\\'
    let input = "pre\x1b]0;hello world\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_dcs_st_terminated() {
    // DCS = ESC 'P' … ST
    let input = "pre\x1bPq#0;2;0;0;0\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_apc_st_terminated() {
    // APC = ESC '_' … ST
    let input = "pre\x1b_some app cmd\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_pm_st_terminated() {
    // PM = ESC '^' … ST
    let input = "pre\x1b^private\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_sos_st_terminated() {
    // SOS = ESC 'X' … ST
    let input = "pre\x1bXstring\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_malformed_lone_esc() {
    // a lone trailing ESC is silently dropped
    let input = "tail\x1b";
    assert_eq!(strip_ansi(input), "tail");
}

#[test]
fn strip_ansi_malformed_unterminated_osc() {
    // OSC with no terminator before EOF — drop the rest
    let input = "pre\x1b]0;never ends";
    assert_eq!(strip_ansi(input), "pre");
}

#[test]
fn strip_ansi_plain_text_passthrough() {
    let input = "no escapes here\nmultiple\tlines";
    assert_eq!(strip_ansi(input), input);
}

#[test]
fn strip_ansi_multiple_mixed_sequences() {
    let input = "\x1b[31mred\x1b[0m \x1b]0;title\x07 \x1b_apc\x1b\\ done";
    assert_eq!(strip_ansi(input), "red   done");
}

#[test]
fn strip_ansi_short_two_byte_escape() {
    // ESC c (reset) — a two-byte Fp/Fs escape — should drop both bytes
    let input = "before\x1bcafter";
    assert_eq!(strip_ansi(input), "beforeafter");
}

#[test]
fn strip_ansi_handles_unicode_around_escapes() {
    let input = "café\x1b[1m → \x1b[0mok ✓";
    assert_eq!(strip_ansi(input), "café → ok ✓");
}

#[test]
fn strip_ansi_dcs_does_not_terminate_on_bel() {
    // Per ECMA-48, DCS terminates ONLY on ST (ESC \). An embedded BEL is
    // payload data, not a terminator, and must NOT short-circuit stripping.
    // If BEL were (incorrectly) treated as a DCS terminator, the trailing
    // "after\x1b\\post" would survive as "afterpost" — we'd see "preafterpost".
    let input = "pre\x1bPq#0;2;0;0;0\x07after\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_apc_does_not_terminate_on_bel() {
    // APC payload may contain BEL bytes; only ST terminates.
    let input = "pre\x1b_cmd\x07with\x07bels\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_pm_does_not_terminate_on_bel() {
    let input = "pre\x1b^private\x07message\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_sos_does_not_terminate_on_bel() {
    let input = "pre\x1bXstring\x07with\x07bels\x1b\\post";
    assert_eq!(strip_ansi(input), "prepost");
}

#[test]
fn strip_ansi_osc_still_terminates_on_bel() {
    // Regression guard: OSC must KEEP its BEL-terminator behaviour
    // (xterm legacy convention) even though DCS/APC/PM/SOS reject BEL.
    let input = "pre\x1b]0;title\x07keep";
    assert_eq!(strip_ansi(input), "prekeep");
}

#[test]
fn map_url_listing_strips_cli_preamble() {
    let input = "\
◐ Mapping https://code.claude.com
  Options:
    maxDepth: 10
    discoverSitemaps: true

Map Results for https://code.claude.com
Showing 2 (source: sitemap)

  • https://code.claude.com/docs/en/agent-sdk/agent-loop
  • https://code.claude.com/docs/en/agent-sdk/claude-code-features";

    assert_eq!(
        map_url_listing(input),
        "https://code.claude.com/docs/en/agent-sdk/agent-loop\nhttps://code.claude.com/docs/en/agent-sdk/claude-code-features"
    );
}

#[test]
fn map_url_listing_keeps_non_map_output_when_no_urls_found() {
    let input = "Map failed before producing URLs";
    assert_eq!(map_url_listing(input), input);
}

#[test]
fn scrape_body_removes_cli_header() {
    let input = "\
Scrape Results for https://example.com
As of: now

# Example

Body text";

    assert_eq!(scrape_body(input), "# Example\n\nBody text");
}

#[test]
fn ask_answer_keeps_only_assistant_response() {
    let input = "\
Conversation
  You: what changed?
  Assistant:
The direct answer.

  Timing: retrieval=1ms | context=2ms | llm=3ms | total=6ms
  Session: default";

    assert_eq!(ask_answer(input), "The direct answer.");
}

#[test]
fn crawl_summary_keeps_result_target_and_job() {
    let input = "\
● Crawl queued

  https://docs.example.com

  Strategy  HTTP first, Chrome fallback
  Scope     same domain, depth 5, uncapped pages
  Pipeline  crawl -> sitemap -> embed
  Runtime   background workers

  Job       018f-example

Follow progress
  axon crawl status 018f-example
  axon status";

    assert_eq!(
        crawl_summary(input),
        "Crawl queued\nhttps://docs.example.com\nJob 018f-example\nNext: axon status"
    );
}

#[test]
fn ingest_summary_suggests_status_for_async_job() {
    let output = BoundedProcessOutput {
        status: success_status(),
        stdout: b"  Ingest Job 018f-ingest\nJob ID: 018f-ingest\n".to_vec(),
        stderr: Vec::new(),
    };

    let rendered = CommandOutput::from_process("axon --local ingest owner/repo", "ingest", output);
    assert_eq!(
        rendered
            .stdout
            .as_ref()
            .map(|section| section.text.as_str()),
        Some("Job ID: 018f-ingest\nNext: axon status")
    );
}

#[test]
fn successful_process_output_drops_progress_stderr() {
    let output = BoundedProcessOutput {
        status: success_status(),
        stdout: b"Search Results for \"axon\"\nFound 1\n\n1. Axon\n   https://example.com\n"
            .to_vec(),
        stderr: b"02:50:24 INFO command=search query_len=4\n".to_vec(),
    };

    let rendered = CommandOutput::from_process("axon --local search axon", "search", output);
    assert!(rendered.stderr.is_none());
    assert_eq!(
        rendered
            .stdout
            .as_ref()
            .map(|section| section.text.as_str()),
        Some("1. Axon\n   https://example.com")
    );
}

#[test]
fn drop_cli_scaffolding_keeps_pending_status_rows() {
    let input = "\
Crawl
  ◐ pending https://example.com 018f-example

Extract
  None.";

    assert_eq!(
        drop_cli_scaffolding(input),
        "Crawl\n  ◐ pending https://example.com 018f-example\nExtract\n  None."
    );
}

#[test]
fn actionable_error_text_prefers_final_error_line() {
    let input = "\
02:50:24  WARN  tei_embed retry transport_error attempt=1/6
02:50:27  WARN  tei_embed retry transport_error attempt=2/6
Error: ServiceError { message: \"TEI unavailable\" }";

    assert_eq!(
        actionable_error_text(input),
        "Error: ServiceError { message: \"TEI unavailable\" }"
    );
}

#[test]
fn actionable_error_text_drops_log_lines_when_no_error_prefix() {
    let input = "\
02:50:24  WARN  tei_embed retry transport_error
failed to connect to service";

    assert_eq!(actionable_error_text(input), "failed to connect to service");
}

#[test]
fn truncate_output_preserves_multibyte_boundary() {
    let input = format!("{}étail", "a".repeat(OUTPUT_LIMIT - 1));

    let output = truncate_output(input);

    assert!(output.ends_with(TRUNCATED_MESSAGE));
    assert!(!output.contains('\u{fffd}'));
    assert_eq!(
        output.trim_end_matches(TRUNCATED_MESSAGE),
        "a".repeat(OUTPUT_LIMIT - 1)
    );
}

#[test]
fn bounded_buffer_does_not_mark_exact_limit_as_truncated() {
    let mut buffer = BoundedByteBuffer::new(4);

    buffer.push(b"abcd");

    assert_eq!(buffer.into_bytes(), b"abcd");
}

#[test]
fn bounded_buffer_truncates_oversized_output_at_utf8_boundary() {
    let mut buffer = BoundedByteBuffer::new(4);

    buffer.push("abcétail".as_bytes());
    let output = String::from_utf8(buffer.into_bytes()).expect("valid utf8");

    assert_eq!(output, format!("abc{TRUNCATED_MESSAGE}"));
    assert!(!output.contains('\u{fffd}'));
}

#[test]
fn output_section_precomputes_markdown_for_markdown_commands() {
    let section =
        OutputSection::from_bytes_for_command("stdout", "ask", b"# Title\n\nBody", true).unwrap();

    let markdown = section.markdown.as_ref().expect("cached markdown");

    assert_eq!(markdown.block_count(), 2);
}

#[test]
fn output_section_keeps_raw_command_without_markdown_cache() {
    let section =
        OutputSection::from_bytes_for_command("stdout", "query", b"# not markdown", false).unwrap();

    assert!(section.markdown.is_none());
    assert_eq!(section.rendered_lines.len(), 1);
}
