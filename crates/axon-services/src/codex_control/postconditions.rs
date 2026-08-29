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
    if matches!(action, ControlAction::MarketplaceUpgrade) {
        return verify_marketplace_upgrade(request, after);
    }
    if expected_revision.is_some() && expected_revision == current_revision {
        return EffectProof::Absent("canonical state is unchanged".to_string());
    }
    match action {
        ControlAction::ConfigValueWrite => verify_config_write(request, after),
        ControlAction::ConfigBatchWrite => verify_config_batch(request, after),
        ControlAction::AccountLogout => verify_logout(before, after),
        ControlAction::PluginInstall => {
            verify_entity(request, before, after, EntityKind::Plugin, true)
        }
        ControlAction::PluginUninstall => {
            verify_entity(request, before, after, EntityKind::Plugin, false)
        }
        ControlAction::MarketplaceAdd => {
            verify_entity(request, before, after, EntityKind::Marketplace, true)
        }
        ControlAction::MarketplaceRemove => {
            verify_entity(request, before, after, EntityKind::Marketplace, false)
        }
        ControlAction::ExternalAgentConfigImport => {
            verify_entity(request, before, after, EntityKind::Skill, true)
        }
        ControlAction::MarketplaceUpgrade => unreachable!("handled before revision comparison"),
        ControlAction::SkillConfigWrite => verify_skill_config(request, after),
        ControlAction::AccountLoginStart | ControlAction::AccountLoginCancel => {
            EffectProof::Unknown("action has no durable action-specific canonical readback".into())
        }
        ControlAction::McpServerReload | ControlAction::McpServerOauthLogin => {
            if find_entity(request, after, EntityKind::Mcp).is_some() {
                EffectProof::Unknown(
                    "MCP server is present, but the requested effect is not durable".into(),
                )
            } else {
                EffectProof::Absent("requested MCP server is absent".into())
            }
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
    kind: EntityKind,
    present: bool,
) -> EffectProof {
    let Some(_) = entity_target(request, kind) else {
        return EffectProof::Unknown("entity mutation has no canonical target".into());
    };
    let after_matches = find_entity(request, after, kind).is_some();
    if present {
        return if after_matches {
            EffectProof::Applied
        } else {
            EffectProof::Absent("requested entity is absent".into())
        };
    }
    if after_matches {
        EffectProof::Absent("removed entity is still present".into())
    } else if before.is_some_and(|value| find_entity(request, value, kind).is_some()) {
        EffectProof::Applied
    } else {
        EffectProof::Unknown("entity was absent before execution".into())
    }
}

#[derive(Clone, Copy)]
enum EntityKind {
    Plugin,
    Marketplace,
    Skill,
    Mcp,
}

fn find_entity<'a>(request: &Value, state: &'a Value, kind: EntityKind) -> Option<&'a Value> {
    let target = entity_target(request, kind)?;
    let (collections, keys): (&[&str], &[&str]) = match kind {
        EntityKind::Plugin => (
            &["plugins", "installedPlugins"],
            &["plugin", "pluginId", "id", "name"],
        ),
        EntityKind::Marketplace => (
            &["marketplaces"],
            &["marketplace", "marketplaceName", "id", "name"],
        ),
        EntityKind::Skill => (&["skills"], &["skill", "skillId", "id", "name"]),
        EntityKind::Mcp => (
            &["servers", "mcpServers"],
            &["server", "serverName", "id", "name"],
        ),
    };
    find_in_collections(state, collections, keys, target)
}

fn entity_target(request: &Value, kind: EntityKind) -> Option<&Value> {
    request.get("target").or_else(|| match kind {
        EntityKind::Marketplace => request.get("marketplaceName"),
        EntityKind::Skill | EntityKind::Mcp => request.get("name").or_else(|| request.get("path")),
        EntityKind::Plugin => request.get("plugin").or_else(|| request.get("pluginId")),
    })
}

fn find_in_collections<'a>(
    value: &'a Value,
    collections: &[&str],
    identity_keys: &[&str],
    expected: &Value,
) -> Option<&'a Value> {
    match value {
        Value::Object(values) => {
            for collection in collections {
                if let Some(value) = values.get(*collection)
                    && let Some(entity) = recursively_find_entity(value, identity_keys, expected)
                {
                    return Some(entity);
                }
            }
            values
                .values()
                .find_map(|value| find_in_collections(value, collections, identity_keys, expected))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_in_collections(value, collections, identity_keys, expected)),
        _ => None,
    }
}

fn recursively_find_entity<'a>(
    value: &'a Value,
    keys: &[&str],
    expected: &Value,
) -> Option<&'a Value> {
    match value {
        Value::Object(values) => {
            if keys.iter().any(|key| values.get(*key) == Some(expected)) {
                Some(value)
            } else {
                values
                    .values()
                    .find_map(|value| recursively_find_entity(value, keys, expected))
            }
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| recursively_find_entity(value, keys, expected)),
        _ => None,
    }
}

fn verify_marketplace_upgrade(request: &Value, after: &Value) -> EffectProof {
    let Some(_) = find_entity(request, after, EntityKind::Marketplace) else {
        return EffectProof::Absent("requested marketplace is absent".into());
    };
    EffectProof::Unknown(
        "Codex does not expose a durable marketplace version or revision readback".into(),
    )
}

fn verify_skill_config(request: &Value, after: &Value) -> EffectProof {
    let Some(entity) = find_entity(request, after, EntityKind::Skill) else {
        return EffectProof::Absent("requested skill is absent".into());
    };
    let requested = ["enabled"]
        .into_iter()
        .filter_map(|key| request.get(key).map(|value| (key, value)))
        .collect::<Vec<_>>();
    if requested.is_empty() {
        return EffectProof::Unknown("skill config write has no requested enabled value".into());
    }
    if requested
        .iter()
        .all(|(key, expected)| entity.get(*key) == Some(*expected))
    {
        EffectProof::Applied
    } else {
        EffectProof::Absent("skill enabled value differs from the request".into())
    }
}
