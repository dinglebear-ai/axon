use axon_codex::api::{ControlAction, account_summary};
use serde_json::Value;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum EffectProof {
    Applied,
    Absent(String),
    Unknown(String),
}

pub(super) fn verify_intended_effect(
    action: &ControlAction,
    request: &Value,
    before: Option<&Value>,
    after: &Value,
    expected_revision: Option<&str>,
    current_revision: Option<&str>,
) -> EffectProof {
    if expected_revision.is_some() && expected_revision == current_revision {
        return EffectProof::Absent("canonical state is unchanged".to_string());
    }
    match action {
        ControlAction::ConfigValueWrite => verify_config_write(request, after),
        ControlAction::ConfigBatchWrite => verify_config_batch(request, after),
        ControlAction::AccountLogout => verify_logout(before, after),
        ControlAction::PluginInstall
        | ControlAction::MarketplaceAdd
        | ControlAction::ExternalAgentConfigImport => verify_entity(request, before, after, true),
        ControlAction::PluginUninstall | ControlAction::MarketplaceRemove => {
            verify_entity(request, before, after, false)
        }
        ControlAction::MarketplaceUpgrade | ControlAction::SkillConfigWrite => {
            if entity_matches(request, after) {
                EffectProof::Applied
            } else {
                EffectProof::Absent("readback does not contain requested target state".into())
            }
        }
        ControlAction::AccountLoginStart
        | ControlAction::AccountLoginCancel
        | ControlAction::McpServerReload
        | ControlAction::McpServerOauthLogin => {
            EffectProof::Unknown("action has no durable action-specific canonical readback".into())
        }
        _ => EffectProof::Unknown("action is not a supported mutation".into()),
    }
}

fn verify_logout(before: Option<&Value>, after: &Value) -> EffectProof {
    if before.is_some_and(|value| account_summary(value).signed_in)
        && !account_summary(after).signed_in
    {
        EffectProof::Applied
    } else if account_summary(after).signed_in {
        EffectProof::Absent("account remains signed in".into())
    } else {
        EffectProof::Unknown("account was already signed out".into())
    }
}

fn verify_config_write(request: &Value, after: &Value) -> EffectProof {
    let Some(path) = request
        .get("keyPath")
        .or_else(|| request.get("key_path"))
        .or_else(|| request.get("key"))
        .and_then(config_path)
    else {
        return EffectProof::Unknown("config request has no readable key path".into());
    };
    let Some(expected) = request.get("value") else {
        return EffectProof::Unknown("config request has no intended value".into());
    };
    if [after.get("persisted"), after.get("active"), Some(after)]
        .into_iter()
        .flatten()
        .any(|root| value_at_path(root, &path) == Some(expected))
    {
        EffectProof::Applied
    } else {
        EffectProof::Absent(format!("config value at {} differs", path.join(".")))
    }
}

fn verify_config_batch(request: &Value, after: &Value) -> EffectProof {
    let Some(writes) = request
        .get("writes")
        .or_else(|| request.get("changes"))
        .or_else(|| request.get("edits"))
        .and_then(Value::as_array)
    else {
        return EffectProof::Unknown("config batch has no readable writes".into());
    };
    if writes.is_empty()
        || writes
            .iter()
            .any(|write| !matches!(verify_config_write(write, after), EffectProof::Applied))
    {
        EffectProof::Absent("not every approved config write is present".into())
    } else {
        EffectProof::Applied
    }
}

fn config_path(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(path) => Some(path.split('.').map(str::to_owned).collect()),
        Value::Array(parts) => parts
            .iter()
            .map(|part| part.as_str().map(str::to_owned))
            .collect(),
        _ => None,
    }
}

fn value_at_path<'a>(mut value: &'a Value, path: &[String]) -> Option<&'a Value> {
    for part in path {
        value = value.get(part)?;
    }
    Some(value)
}

fn verify_entity(
    request: &Value,
    before: Option<&Value>,
    after: &Value,
    present: bool,
) -> EffectProof {
    let after_matches = entity_matches(request, after);
    if present {
        return if after_matches {
            EffectProof::Applied
        } else {
            EffectProof::Absent("requested entity is absent".into())
        };
    }
    if after_matches {
        EffectProof::Absent("removed entity is still present".into())
    } else if before.is_none_or(|value| entity_matches(request, value)) {
        EffectProof::Applied
    } else {
        EffectProof::Unknown("entity was absent before execution".into())
    }
}

fn entity_matches(request: &Value, state: &Value) -> bool {
    const KEYS: &[&str] = &[
        "id",
        "name",
        "plugin",
        "pluginId",
        "marketplace",
        "marketplaceName",
        "skill",
        "skillId",
        "server",
        "serverName",
        "source",
    ];
    KEYS.iter()
        .find_map(|key| request.get(*key).map(|expected| (*key, expected)))
        .is_some_and(|(key, expected)| recursively_matches(state, key, expected))
}

fn recursively_matches(value: &Value, key: &str, expected: &Value) -> bool {
    match value {
        Value::Object(values) => {
            values.get(key) == Some(expected)
                || values
                    .values()
                    .any(|value| recursively_matches(value, key, expected))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| value == expected || recursively_matches(value, key, expected)),
        scalar => scalar == expected,
    }
}
