#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvClassification {
    KeepEnv,
    ComposeEnv,
    MoveToml,
    TrustedOperatorBootstrap,
    /// Stale or removed env var — should be stripped from live environments.
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlacement {
    HostOnly,
    ContainerRequired,
    ComposeInterpolation,
    Both,
    NotRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyBehavior {
    Canonical,
    WarnEnvOverride,
    Advanced,
    /// Key should be deleted from the environment during migration (setup repair).
    DeleteOnMigration,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct EnvKeySpec {
    pub key: &'static str,
    pub classification: EnvClassification,
    pub placement: RuntimePlacement,
    pub toml_destination: Option<&'static str>,
    pub legacy_behavior: LegacyBehavior,
    pub secret: bool,
}

#[path = "env_registry/advanced.rs"]
mod advanced;
#[path = "env_registry/migration.rs"]
mod migration;
#[path = "env_registry/runtime.rs"]
mod runtime;

pub(crate) const fn spec(
    key: &'static str,
    classification: EnvClassification,
    placement: RuntimePlacement,
    toml_destination: Option<&'static str>,
    legacy_behavior: LegacyBehavior,
    secret: bool,
) -> EnvKeySpec {
    EnvKeySpec {
        key,
        classification,
        placement,
        toml_destination,
        legacy_behavior,
        secret,
    }
}

pub(crate) fn all_specs() -> impl Iterator<Item = &'static EnvKeySpec> {
    [
        runtime::RUNTIME_ENV_KEY_SPECS,
        advanced::ADVANCED_ENV_KEY_SPECS,
        migration::MIGRATION_ENV_KEY_SPECS,
    ]
    .into_iter()
    .flatten()
}

pub fn spec_for(key: &str) -> Option<&'static EnvKeySpec> {
    all_specs().find(|spec| spec.key == key)
}

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;
