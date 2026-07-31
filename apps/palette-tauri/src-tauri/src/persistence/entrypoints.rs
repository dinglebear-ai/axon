//! Environment-resolving entrypoints for Axon persistence.

use std::collections::HashMap;

use super::{
    default_config_path, default_env_path, read_config_values_at, write_axon_config_values_at,
    write_axon_env_values_at,
};

pub(crate) fn write_axon_env_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_env_path().ok_or("env path unavailable")?;
    write_axon_env_values_at(values, &path)
}

pub(crate) fn read_default_config_values() -> HashMap<String, serde_json::Value> {
    default_config_path().map_or_else(HashMap::new, |path| read_config_values_at(&path))
}

pub(crate) fn write_axon_config_values(
    values: &HashMap<String, serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = default_config_path().ok_or("config path unavailable")?;
    write_axon_config_values_at(values, &path)
}
