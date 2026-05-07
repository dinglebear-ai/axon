use super::HeadlessAgent;
use super::common::{HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, env_or_default};

pub fn build_command(req: &HeadlessCommandRequest) -> Result<HeadlessCommandSpec, String> {
    let mut args = vec![
        "exec".to_string(),
        "-".to_string(),
        "--sandbox".to_string(),
        "read-only".to_string(),
        "--json".to_string(),
        "--ephemeral".to_string(),
        "--ignore-rules".to_string(),
    ];
    if let Some(model) = req.model.as_ref() {
        args.extend(["--model".to_string(), model.clone()]);
    }
    let spec = HeadlessCommandSpec {
        agent: HeadlessAgent::Codex,
        program: env_or_default("AXON_HEADLESS_CODEX_CMD", "codex"),
        args,
        prompt_transport: PromptTransport::Stdin,
        output_mode: "jsonl",
    };
    spec.validate()?;
    Ok(spec)
}

pub fn safe_posture_available() -> bool {
    false
}

#[cfg(test)]
fn assemble_utf8_chunks(chunks: &[&[u8]]) -> Result<String, std::str::Utf8Error> {
    let bytes = chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect::<Vec<_>>();
    std::str::from_utf8(&bytes).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_headless_command_avoids_deprecated_and_bypass_flags() {
        let spec = build_command(&HeadlessCommandRequest::new(
            Some("gpt-5.5".to_string()),
            Some("system".to_string()),
        ))
        .unwrap();
        let joined = spec.args.join(" ");
        assert_eq!(spec.prompt_transport, PromptTransport::Stdin);
        assert!(joined.contains("--sandbox read-only"));
        assert!(!joined.contains("--full-auto"));
        assert!(!joined.contains("--dangerously-bypass-approvals-and-sandbox"));
        assert!(!joined.contains("danger-full-access"));
    }

    #[test]
    fn codex_headless_assembles_chunked_stdout() {
        let out = assemble_utf8_chunks(&[b"hello ", b"world"]).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn codex_headless_assembles_split_multibyte_codepoint() {
        let smile = "hi \u{263a}".as_bytes();
        let out = assemble_utf8_chunks(&[&smile[..4], &smile[4..]]).unwrap();
        assert_eq!(out, "hi \u{263a}");
    }
}
