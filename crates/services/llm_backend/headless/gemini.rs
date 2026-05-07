use super::HeadlessAgent;
use super::common::{HeadlessCommandRequest, HeadlessCommandSpec, PromptTransport, env_or_default};

pub fn build_command(req: &HeadlessCommandRequest) -> Result<HeadlessCommandSpec, String> {
    let mut args = vec![
        "--prompt".to_string(),
        String::new(),
        "--approval-mode".to_string(),
        "plan".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
    ];
    if let Some(model) = req.model.as_ref() {
        args.extend(["--model".to_string(), model.clone()]);
    }
    let spec = HeadlessCommandSpec {
        agent: HeadlessAgent::Gemini,
        program: env_or_default("AXON_HEADLESS_GEMINI_CMD", "gemini"),
        args,
        prompt_transport: PromptTransport::Stdin,
        output_mode: "stream-json",
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
    fn gemini_headless_command_avoids_yolo() {
        let spec = build_command(&HeadlessCommandRequest::new(
            Some("gemini-3-pro".to_string()),
            Some("system".to_string()),
        ))
        .unwrap();
        let joined = spec.args.join(" ");
        assert_eq!(spec.prompt_transport, PromptTransport::Stdin);
        assert!(joined.contains("--approval-mode plan"));
        assert!(!joined.contains("--yolo"));
        assert!(!joined.contains("--approval-mode=yolo"));
    }

    #[test]
    fn gemini_headless_assembles_chunked_stdout() {
        let out = assemble_utf8_chunks(&[b"hello ", b"world"]).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn gemini_headless_assembles_split_multibyte_codepoint() {
        let snowman = "hi \u{2603}".as_bytes();
        let out = assemble_utf8_chunks(&[&snowman[..4], &snowman[4..]]).unwrap();
        assert_eq!(out, "hi \u{2603}");
    }
}
