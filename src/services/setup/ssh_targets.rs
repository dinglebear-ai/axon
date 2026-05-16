use serde::Serialize;
use ssh2_config::{ParseRule, SshConfig};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SshTarget {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
}

pub fn list_ssh_targets() -> io::Result<Vec<SshTarget>> {
    let path = default_ssh_config_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is unset; cannot read ~/.ssh/config",
        )
    })?;
    list_ssh_targets_from_path(&path)
}

pub fn list_ssh_targets_from_path(path: &Path) -> io::Result<Vec<SshTarget>> {
    let file = std::fs::File::open(path)?;
    list_ssh_targets_from_reader(BufReader::new(file))
}

pub fn list_ssh_targets_from_reader(reader: impl BufRead) -> io::Result<Vec<SshTarget>> {
    let mut reader = reader;
    let config = SshConfig::default()
        .parse(
            &mut reader,
            ParseRule::ALLOW_UNKNOWN_FIELDS | ParseRule::ALLOW_UNSUPPORTED_FIELDS,
        )
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

    let mut targets = BTreeMap::new();
    for host in config.get_hosts() {
        for clause in &host.pattern {
            if clause.negated || !is_concrete_alias(&clause.pattern) {
                continue;
            }
            let params = config.query(&clause.pattern);
            targets.entry(clause.pattern.clone()).or_insert(SshTarget {
                alias: clause.pattern.clone(),
                host_name: params.host_name.clone(),
                user: params.user.clone(),
                port: params.port,
            });
        }
    }

    Ok(targets.into_values().collect())
}

fn default_ssh_config_path() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".ssh/config"))
}

fn is_concrete_alias(pattern: &str) -> bool {
    let pattern = pattern.trim();
    !pattern.is_empty()
        && pattern != "*"
        && !pattern.contains('*')
        && !pattern.contains('?')
        && !pattern.contains('!')
}

#[cfg(test)]
#[path = "ssh_targets_tests.rs"]
mod tests;
