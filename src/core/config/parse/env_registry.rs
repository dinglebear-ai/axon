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
    WarnAndIgnore,
    DeleteOnMigration,
    Advanced,
}

use self::EnvClassification::{
    CompatibilityShim, ComposeEnv, Delete, KeepEnv, MoveToml, TrustedOperatorBootstrap,
};
use self::LegacyBehavior::{
    Advanced, Canonical, DeleteOnMigration, WarnAndIgnore, WarnEnvOverride,
};
use self::RuntimePlacement::{Both, ComposeInterpolation, ContainerRequired, HostOnly, NotRuntime};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatrixEnvSpec {
    pub classification: EnvClassification,
    pub toml_destination: Option<String>,
}

pub(crate) const ENV_KEY_SPECS: &[EnvKeySpec] = &[
    spec("QDRANT_URL", KeepEnv, Both, None, Canonical, false),
    spec("TEI_URL", KeepEnv, Both, None, Canonical, false),
    spec(
        "AXON_CHROME_REMOTE_URL",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec("AXON_SERVER_URL", KeepEnv, HostOnly, None, Canonical, false),
    spec(
        "AXON_MCP_HTTP_TOKEN",
        KeepEnv,
        ContainerRequired,
        None,
        Canonical,
        true,
    ),
    spec("AXON_MCP_AUTH_MODE", KeepEnv, Both, None, Canonical, false),
    spec("AXON_MCP_PUBLIC_URL", KeepEnv, Both, None, Canonical, false),
    spec(
        "AXON_MCP_GOOGLE_CLIENT_ID",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
        KeepEnv,
        Both,
        None,
        Canonical,
        true,
    ),
    spec(
        "AXON_MCP_AUTH_ADMIN_EMAIL",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_MCP_ALLOWED_ORIGINS",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_ENV_FILE",
        TrustedOperatorBootstrap,
        HostOnly,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_CONFIG_PATH",
        TrustedOperatorBootstrap,
        HostOnly,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_HOME",
        TrustedOperatorBootstrap,
        Both,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_DATA_DIR",
        TrustedOperatorBootstrap,
        Both,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_SQLITE_PATH",
        TrustedOperatorBootstrap,
        Both,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_MCP_HTTP_PUBLISH",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_IMAGE",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "GEMINI_HOME",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "TEI_EMBEDDING_MODEL",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "TEI_HTTP_PORT",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "TEI_SERVER_MAX_CLIENT_BATCH_SIZE",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "NVIDIA_VISIBLE_DEVICES",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec(
        "CUDA_VISIBLE_DEVICES",
        ComposeEnv,
        ComposeInterpolation,
        None,
        Canonical,
        false,
    ),
    spec("TAVILY_API_KEY", KeepEnv, Both, None, Canonical, true),
    spec("GITHUB_TOKEN", KeepEnv, Both, None, Canonical, true),
    spec("REDDIT_CLIENT_ID", KeepEnv, Both, None, Canonical, false),
    spec("REDDIT_CLIENT_SECRET", KeepEnv, Both, None, Canonical, true),
    spec(
        "HF_TOKEN",
        KeepEnv,
        ComposeInterpolation,
        None,
        Canonical,
        true,
    ),
    spec(
        "OPENAI_MODEL",
        CompatibilityShim,
        Both,
        None,
        WarnEnvOverride,
        false,
    ),
    spec(
        "OPENAI_BASE_URL",
        CompatibilityShim,
        Both,
        None,
        WarnAndIgnore,
        false,
    ),
    spec(
        "OPENAI_API_KEY",
        CompatibilityShim,
        Both,
        None,
        WarnAndIgnore,
        true,
    ),
    spec(
        "AXON_HEADLESS_GEMINI_CMD",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_HEADLESS_GEMINI_HOME",
        TrustedOperatorBootstrap,
        Both,
        None,
        Advanced,
        false,
    ),
    spec(
        "AXON_HEADLESS_GEMINI_MODEL",
        KeepEnv,
        Both,
        None,
        Canonical,
        false,
    ),
    spec(
        "AXON_LLM_COMPLETION_CONCURRENCY",
        MoveToml,
        NotRuntime,
        Some("llm.completion-concurrency"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_LLM_COMPLETION_TIMEOUT_SECS",
        MoveToml,
        NotRuntime,
        Some("llm.completion-timeout-secs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "TEI_MAX_CLIENT_BATCH_SIZE",
        MoveToml,
        NotRuntime,
        Some("tei.max-client-batch-size"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "TEI_MAX_RETRIES",
        MoveToml,
        NotRuntime,
        Some("tei.max-retries"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "TEI_REQUEST_TIMEOUT_MS",
        MoveToml,
        NotRuntime,
        Some("tei.request-timeout-ms"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_INGEST_LANES",
        MoveToml,
        NotRuntime,
        Some("workers.ingest-lanes"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_EMBED_LANES",
        MoveToml,
        NotRuntime,
        Some("workers.embed-lanes"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_EMBED_DOC_TIMEOUT_SECS",
        MoveToml,
        NotRuntime,
        Some("workers.embed-doc-timeout-secs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_QUEUE_SUMMARY_SECS",
        MoveToml,
        NotRuntime,
        Some("workers.queue-summary-secs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_QDRANT_POINT_BUFFER",
        MoveToml,
        NotRuntime,
        Some("workers.qdrant-point-buffer"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_MAX_PENDING_CRAWL_JOBS",
        MoveToml,
        NotRuntime,
        Some("workers.max-pending-crawl-jobs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_MAX_PENDING_EMBED_JOBS",
        MoveToml,
        NotRuntime,
        Some("workers.max-pending-embed-jobs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_MAX_PENDING_EXTRACT_JOBS",
        MoveToml,
        NotRuntime,
        Some("workers.max-pending-extract-jobs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_MAX_PENDING_INGEST_JOBS",
        MoveToml,
        NotRuntime,
        Some("workers.max-pending-ingest-jobs"),
        WarnEnvOverride,
        false,
    ),
    spec(
        "AXON_BATCH_QUEUE",
        Delete,
        NotRuntime,
        None,
        DeleteOnMigration,
        false,
    ),
    spec(
        "AXON_CRAWL_QUEUE",
        Delete,
        NotRuntime,
        None,
        DeleteOnMigration,
        false,
    ),
    spec(
        "AXON_EMBED_QUEUE",
        Delete,
        NotRuntime,
        None,
        DeleteOnMigration,
        false,
    ),
    spec(
        "AXON_EXTRACT_QUEUE",
        Delete,
        NotRuntime,
        None,
        DeleteOnMigration,
        false,
    ),
    spec(
        "AXON_INGEST_QUEUE",
        Delete,
        NotRuntime,
        None,
        DeleteOnMigration,
        false,
    ),
];

const fn spec(
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

pub(crate) fn spec_for(key: &str) -> Option<&'static EnvKeySpec> {
    ENV_KEY_SPECS.iter().find(|spec| spec.key == key)
}

pub(crate) fn matrix_spec_for(key: &str) -> Option<MatrixEnvSpec> {
    let matrix = include_str!("../../../../docs/config/env-migration-matrix.toml");
    matrix.split("[[env]]").find_map(|block| {
        (matrix_string_field(block, "key")? == key).then(|| {
            let classification =
                matrix_classification(&matrix_string_field(block, "classification")?)?;
            let toml_destination = matrix_string_field(block, "toml_destination")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            Some(MatrixEnvSpec {
                classification,
                toml_destination,
            })
        })?
    })
}

fn matrix_string_field(block: &str, field: &str) -> Option<String> {
    let prefix = format!("{field} = ");
    block.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?;
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .map(ToString::to_string)
    })
}

fn matrix_classification(value: &str) -> Option<EnvClassification> {
    match value {
        "keep-env" => Some(KeepEnv),
        "compose-env" => Some(ComposeEnv),
        "move-toml" => Some(MoveToml),
        "delete" | "hard-default" | "external/test-only" => Some(Delete),
        "trusted-operator-bootstrap" => Some(TrustedOperatorBootstrap),
        "compatibility-shim" => Some(CompatibilityShim),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_urls_are_env_not_toml() {
        for key in ["QDRANT_URL", "TEI_URL", "AXON_CHROME_REMOTE_URL"] {
            let spec = spec_for(key).expect("registered key");
            assert_eq!(spec.classification, KeepEnv);
            assert_eq!(spec.toml_destination, None);
        }
    }

    #[test]
    fn moved_tuning_has_toml_destination() {
        for spec in ENV_KEY_SPECS {
            if spec.classification == MoveToml {
                assert!(
                    spec.toml_destination.is_some(),
                    "{} is move-toml without destination",
                    spec.key
                );
            }
        }
    }

    #[test]
    fn matrix_lookup_covers_scanned_runtime_keys_not_in_static_registry() {
        let gemini = matrix_spec_for("GEMINI_API_KEY").expect("matrix key");
        assert_eq!(gemini.classification, KeepEnv);

        let mcp_host = matrix_spec_for("AXON_MCP_HTTP_HOST").expect("matrix key");
        assert_eq!(mcp_host.classification, TrustedOperatorBootstrap);

        let tei_server = matrix_spec_for("TEI_MAX_CONCURRENT_REQUESTS").expect("matrix key");
        assert_eq!(tei_server.classification, ComposeEnv);
    }
}
