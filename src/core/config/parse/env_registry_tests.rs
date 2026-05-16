use super::*;
use EnvClassification::{KeepEnv, MoveToml};

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
    for spec in all_specs() {
        if spec.classification == MoveToml {
            assert!(
                spec.toml_destination.is_some(),
                "{} is move-toml without destination",
                spec.key
            );
        }
    }
}
