use super::ControlAction;
use serde_json::Value;
use std::net::IpAddr;
use url::Url;

pub fn validate_mutation_params(action: &ControlAction, params: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(params).map_err(|error| error.to_string())?;
    if encoded.len() > 64 * 1024 {
        return Err("Codex mutation parameters exceed 64 KiB".to_string());
    }
    if !params.is_object() {
        return Err("Codex mutation parameters must be a JSON object".to_string());
    }
    reject_plaintext_secrets(params)?;
    if matches!(action, ControlAction::ConfigValueWrite) {
        validate_config_edit(params)?;
    }
    if matches!(action, ControlAction::ConfigBatchWrite) {
        let edits = params
            .get("edits")
            .and_then(Value::as_array)
            .ok_or("config batch requires an edits array")?;
        if edits.is_empty() {
            return Err("config batch requires at least one edit".to_string());
        }
        for edit in edits {
            validate_config_edit(edit)?;
        }
    }
    if matches!(action, ControlAction::MarketplaceAdd) {
        validate_public_source(
            params
                .get("source")
                .and_then(Value::as_str)
                .ok_or("marketplace source missing")?,
        )?;
    }
    if matches!(action, ControlAction::AccountRateLimitResetCreditConsume)
        && params
            .get("idempotencyKey")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        return Err("reset credit consume requires idempotencyKey".to_string());
    }
    if matches!(action, ControlAction::AccountBedrockSetup) {
        let setup_type = params.get("type").and_then(Value::as_str);
        if !matches!(setup_type, Some("profile" | "environment"))
            || params
                .get("region")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            || (setup_type == Some("profile")
                && params
                    .get("profile")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty))
        {
            return Err(
                "Bedrock setup requires a valid type, region, and optional profile".to_string(),
            );
        }
    }
    if matches!(action, ControlAction::ExperimentalFeatureEnablementSet) {
        let enablement = params
            .get("enablement")
            .and_then(Value::as_object)
            .ok_or("experimental feature update requires an enablement object")?;
        if enablement.values().any(|value| !value.is_boolean()) {
            return Err("experimental feature enablement values must be booleans".to_string());
        }
    }
    Ok(())
}

fn validate_config_edit(edit: &Value) -> Result<(), String> {
    let key_path = edit
        .get("keyPath")
        .and_then(Value::as_str)
        .ok_or("config edit keyPath missing")?;
    let strategy = edit
        .get("mergeStrategy")
        .and_then(Value::as_str)
        .ok_or("config edit mergeStrategy missing")?;
    if !matches!(strategy, "replace" | "upsert") {
        return Err("config edit mergeStrategy must be replace or upsert".to_string());
    }
    let value = edit.get("value").ok_or("config edit value missing")?;
    let secret_target = key_path.split('.').any(is_sensitive_identifier);
    if secret_target && !value.as_str().is_some_and(is_env_reference) {
        return Err(format!("{key_path} must use an env: secret reference"));
    }
    Ok(())
}

fn validate_public_source(source: &str) -> Result<(), String> {
    let parsed = Url::parse(source).map_err(|_| "marketplace source is not a valid URL")?;
    if parsed.scheme() != "https" {
        return Err("marketplace source must use HTTPS".to_string());
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "marketplace source must not contain credentials or a query string".to_string(),
        );
    }
    let host = parsed
        .host_str()
        .ok_or("marketplace source host is missing")?
        .to_ascii_lowercase();
    if host.parse::<IpAddr>().is_ok()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err("marketplace source host is not public".to_string());
    }
    if !matches!(host.as_str(), "github.com" | "gitlab.com" | "bitbucket.org") {
        return Err("marketplace source must use an approved public forge".to_string());
    }
    Ok(())
}

fn reject_plaintext_secrets(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                if is_sensitive_identifier(key) && !value.as_str().is_some_and(is_env_reference) {
                    return Err(format!("{key} must use an env: secret reference"));
                }
                reject_plaintext_secrets(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_plaintext_secrets(value)?;
            }
        }
        Value::String(text) if contains_sensitive_url(text) => {
            return Err("signed or credential-bearing URLs are not accepted".to_string());
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn is_sensitive_identifier(value: &str) -> bool {
    let canonical = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    [
        "apikey",
        "token",
        "secret",
        "password",
        "authorization",
        "cookie",
        "privatekey",
        "accesskey",
        "credential",
        "clientsecret",
        "bearer",
    ]
    .iter()
    .any(|needle| canonical.contains(needle))
}

fn is_env_reference(value: &str) -> bool {
    value.strip_prefix("env:").is_some_and(|name| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

pub(crate) fn contains_sensitive_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    !url.username().is_empty()
        || url.password().is_some()
        || url.query_pairs().any(|(key, _)| {
            is_sensitive_identifier(&key)
                || matches!(key.as_ref(), "X-Amz-Signature" | "X-Goog-Signature")
        })
}
