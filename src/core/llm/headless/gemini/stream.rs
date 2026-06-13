use serde_json::Value;
use std::error::Error as StdError;

#[derive(Default)]
pub(super) struct GeminiStreamState {
    text: String,
    result_text: Option<String>,
    pub(super) saw_success: bool,
}

impl GeminiStreamState {
    pub(super) fn handle_line<F>(
        &mut self,
        line: &str,
        on_delta: &mut F,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|err| format!("malformed Gemini stream JSON: {err}: {trimmed}"))?;
        match value.get("type").and_then(Value::as_str) {
            Some("tool_use") => self.handle_tool_use(&value)?,
            Some("tool_result") => {}
            Some("error") => return Err(format!("Gemini headless stream error: {value}").into()),
            Some("message") => {
                if value.get("role").and_then(Value::as_str) == Some("assistant")
                    && let Some(delta) = message_content(&value)
                {
                    self.push_delta(&delta, on_delta)?;
                }
            }
            Some("result") => self.handle_result(&value)?,
            _ if contains_tool_event(&value) => {
                return Err("Gemini headless emitted a tool event in synthesis-only mode".into());
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_tool_use(&mut self, value: &Value) -> Result<(), Box<dyn StdError + Send + Sync>> {
        // Permitted tool calls for synthesis mode:
        // - "activate_skill": loads the axon-rag-synthesize skill (intentional)
        // - "update_topic": Gemini 0.41.2+ internal session management (harmless)
        // All other tool_use events indicate unexpected tool execution and are rejected.
        // Field name changed from "name" to "tool_name" in Gemini CLI 0.41.2.
        let tool_name = value
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| value.get("tool_name").and_then(Value::as_str));
        let permitted = matches!(tool_name, Some("activate_skill") | Some("update_topic"));
        if permitted {
            return Ok(());
        }
        Err(format!(
            "Gemini headless emitted unexpected tool call '{}' in synthesis mode; raw event: {value}",
            tool_name.unwrap_or("unknown")
        )
        .into())
    }

    fn handle_result(&mut self, value: &Value) -> Result<(), Box<dyn StdError + Send + Sync>> {
        if value.get("status").and_then(Value::as_str) != Some("success") {
            return Err(format!("Gemini headless returned unsuccessful result: {value}").into());
        }
        // Per-event whitelist (activate_skill, update_topic) is the primary defence.
        // The stats tool_calls count is no longer used as a secondary gate — Gemini
        // 0.41.2+ calls update_topic automatically, making the count unreliable.
        if let Some(text) = value.get("response").and_then(Value::as_str)
            && !text.trim().is_empty()
        {
            self.result_text = Some(text.to_string());
        }
        self.saw_success = true;
        Ok(())
    }

    fn push_delta<F>(
        &mut self,
        delta: &str,
        on_delta: &mut F,
    ) -> Result<(), Box<dyn StdError + Send + Sync>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError + Send + Sync>> + Send,
    {
        if delta.is_empty() {
            return Ok(());
        }
        self.text.push_str(delta);
        on_delta(delta)
    }

    pub(super) fn finish(self) -> Result<String, Box<dyn StdError + Send + Sync>> {
        if !self.saw_success {
            return Err("Gemini headless stream ended without a success result".into());
        }
        if !self.text.trim().is_empty() {
            return Ok(self.text);
        }
        if let Some(result) = self.result_text
            && !result.trim().is_empty()
        {
            return Ok(result);
        }
        Err("Gemini headless returned no answer text".into())
    }
}

fn message_content(value: &Value) -> Option<String> {
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return Some(content.to_string());
    }
    if let Some(parts) = value.get("content").and_then(Value::as_array) {
        let mut out = String::new();
        for part in parts {
            if let Some(text) = part.as_str() {
                out.push_str(text);
            } else if let Some(text) = part.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
        return (!out.is_empty()).then_some(out);
    }
    None
}

fn contains_tool_event(value: &Value) -> bool {
    match value {
        Value::String(s) => matches!(s.as_str(), "tool_use" | "tool_result"),
        Value::Array(items) => items.iter().any(contains_tool_event),
        Value::Object(map) => map.iter().any(|(key, value)| {
            key == "tool_use"
                || key == "tool_result"
                || (key == "type"
                    && value
                        .as_str()
                        .is_some_and(|s| s == "tool_use" || s == "tool_result"))
                || contains_tool_event(value)
        }),
        _ => false,
    }
}
