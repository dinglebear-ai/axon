use super::constants::{ALLOWED_FLAGS, ASYNC_MODES, NO_JSON_MODES};

fn evaluate_events_mode(flags: &serde_json::Value) -> bool {
    flags
        .as_object()
        .and_then(|obj| obj.get("responses_mode"))
        .and_then(serde_json::Value::as_str)
        .map(|value| value.eq_ignore_ascii_case("events"))
        .unwrap_or(false)
}

pub(super) fn build_args(mode: &str, input: &str, flags: &serde_json::Value) -> Vec<String> {
    let is_async = ASYNC_MODES.contains(&mode);
    let mut args: Vec<String> = vec![mode.to_string()];

    let trimmed = input.trim();
    if !trimmed.is_empty() {
        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let is_job_subcmd = matches!(
            parts[0],
            "cancel" | "status" | "errors" | "list" | "cleanup" | "clear" | "worker" | "recover"
        );
        if is_job_subcmd {
            for part in parts {
                let p = part.trim().trim_start_matches('-');
                if !p.is_empty() {
                    args.push(p.to_string());
                }
            }
        } else {
            let sanitized = trimmed.trim_start_matches('-');
            if !sanitized.is_empty() {
                args.push(sanitized.to_string());
            }
        }
    }

    let disable_json_for_evaluate_events = mode == "evaluate" && evaluate_events_mode(flags);
    if !NO_JSON_MODES.contains(&mode) && !disable_json_for_evaluate_events {
        args.push("--json".to_string());
    }
    if mode == "scrape" {
        args.push("--embed".to_string());
        args.push("false".to_string());
    }

    if let Some(obj) = flags.as_object() {
        for (json_key, cli_flag) in ALLOWED_FLAGS {
            if is_async && *json_key == "wait" {
                continue;
            }
            if let Some(val) = obj.get(*json_key) {
                match val {
                    serde_json::Value::Bool(true) => {
                        args.push(cli_flag.to_string());
                    }
                    serde_json::Value::Bool(false) => {
                        args.push(cli_flag.to_string());
                        args.push("false".to_string());
                    }
                    serde_json::Value::Number(n) => {
                        args.push(cli_flag.to_string());
                        args.push(n.to_string());
                    }
                    serde_json::Value::String(s) if !s.is_empty() => {
                        // Guard output-dir values against path traversal attacks.
                        // Any value containing a `..` component is rejected before it
                        // reaches the subprocess, preventing a caller from redirecting
                        // output outside the expected output root.
                        if cli_flag.contains("output") && cli_flag.contains("dir") {
                            let p = std::path::Path::new(s.as_str());
                            if p.components().any(|c| c == std::path::Component::ParentDir) {
                                tracing::warn!(context = "execute", path = %s, "rejecting output-dir with path traversal");
                                continue;
                            }
                        }
                        args.push(cli_flag.to_string());
                        args.push(s.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── output-dir traversal guard ──────────────────────────────────────────

    #[test]
    fn output_dir_traversal_dotdot_is_rejected() {
        let args = build_args(
            "scrape",
            "https://example.com",
            &serde_json::json!({"output_dir": "../../etc/passwd"}),
        );
        assert!(
            !args.contains(&"--output-dir".to_string()),
            "expected --output-dir to be blocked for '../../etc/passwd', got: {args:?}"
        );
    }

    #[test]
    fn output_dir_traversal_nested_dotdot_is_rejected() {
        let args = build_args(
            "scrape",
            "https://example.com",
            &serde_json::json!({"output_dir": "../sibling/dir"}),
        );
        assert!(
            !args.contains(&"--output-dir".to_string()),
            "expected --output-dir to be blocked for '../sibling/dir', got: {args:?}"
        );
    }

    #[test]
    fn output_dir_valid_path_passes_through() {
        let args = build_args(
            "scrape",
            "https://example.com",
            &serde_json::json!({"output_dir": "output/subdir"}),
        );
        let idx = args
            .iter()
            .position(|a| a == "--output-dir")
            .expect("--output-dir should be present for a safe path");
        assert_eq!(
            args.get(idx + 1).map(String::as_str),
            Some("output/subdir"),
            "expected 'output/subdir' immediately after --output-dir"
        );
    }

    #[test]
    fn unknown_flag_key_is_silently_dropped() {
        let args = build_args(
            "query",
            "test query",
            &serde_json::json!({"unknown_key": "value"}),
        );
        assert!(
            !args.contains(&"unknown_key".to_string()),
            "unknown_key should not appear in args"
        );
        assert!(
            !args.contains(&"value".to_string()),
            "value from unknown_key should not appear in args"
        );
    }

    #[test]
    fn empty_string_value_is_suppressed() {
        let args = build_args("query", "test query", &serde_json::json!({"limit": ""}));
        assert!(
            !args.contains(&"--limit".to_string()),
            "empty-string value should suppress the flag; got: {args:?}"
        );
    }
}
