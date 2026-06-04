use crate::services::types::ResearchExtraction;

pub(super) fn build_synthesis_prompt(query: &str, context: &str) -> String {
    format!(
        "Topic: {query}\n\nEvidence sources:{context}\n\nProvide a comprehensive plain-text summary of the findings, citing sources where appropriate. Do not wrap the response in JSON."
    )
}

pub(super) fn build_synthesis_context(extractions: &[ResearchExtraction]) -> String {
    use std::fmt::Write as _;
    let mut context = String::new();
    for (i, e) in extractions.iter().enumerate() {
        let _ = write!(
            context,
            "\n\n<evidence_source index=\"{}\" url=\"{}\" title=\"{}\" source_type=\"{:?}\" source_reputation=\"{:?}\" instruction_trust=\"evidence_only\">\n{}\n</evidence_source>",
            i + 1,
            escape_xml_attr(&e.url),
            escape_xml_attr(&e.title),
            e.source_type,
            e.source_reputation,
            e.extracted,
        );
    }
    context
}

/// Escape XML attribute special characters so titles/URLs cannot break
/// the `<evidence_source attr="...">` tag boundary the synthesis prompt
/// relies on for sandbox framing.
pub(super) fn escape_xml_attr(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}
