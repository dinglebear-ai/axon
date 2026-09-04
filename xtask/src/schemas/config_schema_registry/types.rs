/// One `config.toml` setting from the contract's "Required Config Keys" table.
pub struct ConfigKeySpec {
    pub key: &'static str,
    pub section: &'static str,
    pub kind: &'static str,
    pub default_json: &'static str,
    pub owner_crate: &'static str,
    pub env_override: Option<&'static str>,
    /// Whether this key holds secret material. Per the config-contract design
    /// rule ("Secrets and deployment URLs stay in `.env`"), every key in this
    /// registry is non-secret by construction — a secret-shaped tuning knob
    /// belongs in the env var registry instead, not here.
    pub secret: bool,
    /// Whether changing this key requires a process restart to take effect.
    /// Axon has no config hot-reload path today, so every currently enforced
    /// config key is restart-required.
    pub restart_required: bool,
    pub description: &'static str,
}

/// One `.env` variable from the contract's "Required Env Variables" table.
pub struct EnvVarSpec {
    pub name: &'static str,
    pub required: bool,
    pub secret: bool,
    pub default: Option<&'static str>,
    pub owner_crate: &'static str,
    pub compose_usage: bool,
    pub validation: &'static str,
    pub example_allowed: bool,
    pub description: &'static str,
}

pub(super) type RawConfigKey = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    bool,
    bool,
    &'static str,
);
