use super::*;

#[test]
fn ssh_target_listing_keeps_only_concrete_non_negated_aliases() {
    let config = br#"
Host prod web-?
  HostName prod.example.test
  User axon
  Port 2222

Host !blocked staging
  HostName staging.example.test

Host *
  User shared
"#;
    let targets = list_ssh_targets_from_reader(&config[..]).unwrap();

    assert_eq!(
        targets,
        vec![
            SshTarget {
                alias: "prod".to_string(),
                host_name: Some("prod.example.test".to_string()),
                user: Some("axon".to_string()),
                port: Some(2222),
            },
            SshTarget {
                alias: "staging".to_string(),
                host_name: Some("staging.example.test".to_string()),
                user: Some("shared".to_string()),
                port: None,
            },
        ]
    );
}
