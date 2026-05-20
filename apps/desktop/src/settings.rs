use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;

mod settings_io;
mod settings_schema;
use settings_io::{
    flatten_toml, read_env_entries, read_toml_document, resolve_env_path, resolve_toml_path,
    write_env_updates, write_toml_updates,
};
use settings_schema::{ENV_FIELDS, TOML_FIELDS};

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SettingsSource {
    Env,
    Toml,
}

impl SettingsSource {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Env => ".env",
            Self::Toml => "config.toml",
        }
    }
}

#[derive(Clone)]
pub(crate) struct SettingsEntry {
    pub(crate) section: &'static str,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) original_value: String,
    pub(crate) source: SettingsSource,
    pub(crate) secret: bool,
    pub(crate) description: &'static str,
}

impl SettingsEntry {
    pub(crate) fn dirty(&self) -> bool {
        self.value != self.original_value
    }
}

#[derive(Clone)]
pub(crate) struct AxonSettings {
    pub(crate) env_path: PathBuf,
    pub(crate) toml_path: PathBuf,
    pub(crate) entries: Vec<SettingsEntry>,
    pub(crate) selected: usize,
    pub(crate) reveal_secrets: bool,
    pub(crate) status: Option<SettingsStatus>,
}

#[derive(Clone)]
pub(crate) struct SettingsStatus {
    pub(crate) kind: SettingsStatusKind,
    pub(crate) message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsStatusKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Copy)]
pub(crate) struct FieldSpec {
    source: SettingsSource,
    section: &'static str,
    key: &'static str,
    description: &'static str,
    secret: bool,
}

pub(crate) const fn env(
    section: &'static str,
    key: &'static str,
    description: &'static str,
    secret: bool,
) -> FieldSpec {
    FieldSpec {
        source: SettingsSource::Env,
        section,
        key,
        description,
        secret,
    }
}

pub(crate) const fn toml(
    section: &'static str,
    key: &'static str,
    description: &'static str,
) -> FieldSpec {
    FieldSpec {
        source: SettingsSource::Toml,
        section,
        key,
        description,
        secret: false,
    }
}

impl AxonSettings {
    pub(crate) fn load_default() -> Self {
        match Self::load(resolve_env_path(), resolve_toml_path()) {
            Ok(settings) => settings,
            Err(err) => Self::with_error(err),
        }
    }

    pub(crate) fn load(env_path: PathBuf, toml_path: PathBuf) -> io::Result<Self> {
        let env_values = read_env_entries(&env_path)?;
        let toml_doc = read_toml_document(&toml_path)?;
        let toml_values = flatten_toml(&toml_doc);
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();

        for spec in ENV_FIELDS.iter().chain(TOML_FIELDS.iter()) {
            let value = match spec.source {
                SettingsSource::Env => env_values.get(spec.key),
                SettingsSource::Toml => toml_values.get(spec.key),
            }
            .cloned()
            .unwrap_or_default();
            seen.insert((spec.source, spec.key.to_string()));
            entries.push(entry_from_spec(*spec, value));
        }

        for (key, value) in env_values {
            if seen.insert((SettingsSource::Env, key.clone())) {
                entries.push(SettingsEntry {
                    section: "Other .env",
                    key,
                    value: value.clone(),
                    original_value: value,
                    source: SettingsSource::Env,
                    secret: false,
                    description: "Existing .env entry",
                });
            }
        }

        for (key, value) in toml_values {
            if seen.insert((SettingsSource::Toml, key.clone())) {
                entries.push(SettingsEntry {
                    section: "Other config.toml",
                    key,
                    value: value.clone(),
                    original_value: value,
                    source: SettingsSource::Toml,
                    secret: false,
                    description: "Existing config.toml entry",
                });
            }
        }

        Ok(Self {
            env_path,
            toml_path,
            entries,
            selected: 0,
            reveal_secrets: false,
            status: Some(SettingsStatus {
                kind: SettingsStatusKind::Info,
                message: "Edit a value, then save from File > Save Settings.".to_string(),
            }),
        })
    }

    pub(crate) fn save(&mut self) {
        match self.save_inner() {
            Ok(count) => {
                for entry in &mut self.entries {
                    entry.original_value.clone_from(&entry.value);
                }
                self.status = Some(SettingsStatus {
                    kind: SettingsStatusKind::Success,
                    message: format!(
                        "Saved {count} changed setting(s). Restart Axon for server-side changes."
                    ),
                });
            }
            Err(err) => {
                self.status = Some(SettingsStatus {
                    kind: SettingsStatusKind::Error,
                    message: format!("Save failed: {err}"),
                });
            }
        }
    }

    pub(crate) fn reload(&mut self) {
        match Self::load(self.env_path.clone(), self.toml_path.clone()) {
            Ok(next) => *self = next,
            Err(err) => {
                self.status = Some(SettingsStatus {
                    kind: SettingsStatusKind::Error,
                    message: format!("Reload failed: {err}"),
                });
            }
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.entries.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub(crate) fn push_char(&mut self, ch: char) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.value.push(ch);
            self.status = None;
        }
    }

    pub(crate) fn backspace(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.value.pop();
            self.status = None;
        }
    }

    pub(crate) fn clear_selected(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.value.clear();
            self.status = None;
        }
    }

    pub(crate) fn dirty_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.dirty()).count()
    }

    fn save_inner(&self) -> io::Result<usize> {
        let mut env_updates = BTreeMap::new();
        let mut toml_updates = BTreeMap::new();
        for entry in self.entries.iter().filter(|entry| entry.dirty()) {
            match entry.source {
                SettingsSource::Env => {
                    env_updates.insert(entry.key.clone(), entry.value.clone());
                }
                SettingsSource::Toml => {
                    toml_updates.insert(entry.key.clone(), entry.value.clone());
                }
            }
        }
        if !env_updates.is_empty() {
            write_env_updates(&self.env_path, &env_updates)?;
        }
        if !toml_updates.is_empty() {
            write_toml_updates(&self.toml_path, &toml_updates)?;
        }
        Ok(env_updates.len() + toml_updates.len())
    }

    fn with_error(err: io::Error) -> Self {
        let env_path = resolve_env_path();
        let toml_path = resolve_toml_path();
        Self {
            env_path,
            toml_path,
            entries: Vec::new(),
            selected: 0,
            reveal_secrets: false,
            status: Some(SettingsStatus {
                kind: SettingsStatusKind::Error,
                message: format!("Load failed: {err}"),
            }),
        }
    }
}

fn entry_from_spec(spec: FieldSpec, value: String) -> SettingsEntry {
    SettingsEntry {
        section: spec.section,
        key: spec.key.to_string(),
        value: value.clone(),
        original_value: value,
        source: spec.source,
        secret: spec.secret,
        description: spec.description,
    }
}
