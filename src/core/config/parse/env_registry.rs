#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvClassification {
    KeepEnv,
    ComposeEnv,
    MoveToml,
    Delete,
    TrustedOperatorBootstrap,
    CompatibilityShim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimePlacement {
    HostOnly,
    ContainerRequired,
    ComposeInterpolation,
    Both,
    NotRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyBehavior {
    Canonical,
    WarnEnvOverride,
    #[allow(dead_code)]
    // No active env keys use this behavior, but match arms still handle it defensively.
    WarnAndIgnore,
    DeleteOnMigration,
    Advanced,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct EnvKeySpec {
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

pub(crate) fn spec_for(key: &str) -> Option<&'static EnvKeySpec> {
    all_specs().find(|spec| spec.key == key)
}

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;
