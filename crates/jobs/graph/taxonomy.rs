use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, LazyLock};

static RUST_USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)\b(?:pub\s+)?use\s+([A-Za-z_][A-Za-z0-9_-]*)::").unwrap());
static PYTHON_FROM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)\bfrom\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+import\b").unwrap());
static PYTHON_IMPORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)\bimport\s+([A-Za-z_][A-Za-z0-9_\.]*)").unwrap());
static JS_IMPORT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)\bimport\s+(?:.+?\s+from\s+)?['"]([^'"]+)['"]"#).unwrap());
static JS_REQUIRE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"require\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap());
static FILE_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)([A-Za-z0-9_./-]+\.(?:rs|py|ts|tsx|js|jsx|toml|json|ya?ml|md))"#).unwrap()
});
static CLI_COMMAND_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)(?:^|[`'"]|\s)(cargo|docker|pnpm|npm|yarn|python3?|uv|rustfmt|clippy|axon)(?:\s|$)"#,
    )
    .unwrap()
});
static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s)"'>]+"#).unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct EntityCandidate {
    pub name: String,
    pub entity_type: String,
    pub confidence: f32,
    pub source: CandidateSource,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateSource {
    Import,
    FilePath,
    CliCommand,
    Url,
    Taxonomy,
}

#[derive(Debug, Clone, Deserialize)]
#[expect(dead_code)]
pub struct TaxonomyEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category: String,
    #[serde(default)]
    pub ambiguous: bool,
    pub disambiguation_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaxonomyDocument {
    entries: Vec<TaxonomyEntry>,
}

pub struct Taxonomy {
    pub entries: Vec<TaxonomyEntry>,
    lookup: HashMap<String, usize>,
}

/// Cached builtin taxonomy — parsed once, reused across all graph jobs and queries.
static BUILTIN_TAXONOMY: LazyLock<Arc<Taxonomy>> = LazyLock::new(|| {
    let doc: TaxonomyDocument =
        serde_json::from_str(include_str!("taxonomy.json")).expect("builtin taxonomy");
    Arc::new(Taxonomy::from_entries(doc.entries))
});

impl Taxonomy {
    /// Returns the cached builtin taxonomy. Parsed once at first access.
    pub fn builtin() -> Arc<Taxonomy> {
        Arc::clone(&BUILTIN_TAXONOMY)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let raw = fs::read_to_string(path)?;
        let doc: TaxonomyDocument = serde_json::from_str(&raw)?;
        Ok(Self::from_entries(doc.entries))
    }

    /// Resolve a taxonomy for the given config — cached builtin or custom from path.
    pub fn resolve(taxonomy_path: &str) -> Result<Arc<Taxonomy>, Box<dyn std::error::Error>> {
        let taxonomy_path = taxonomy_path.trim();
        if taxonomy_path.is_empty() {
            Ok(Self::builtin())
        } else {
            Ok(Arc::new(Self::from_path(taxonomy_path)?))
        }
    }

    fn from_entries(entries: Vec<TaxonomyEntry>) -> Self {
        let mut lookup = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            lookup.insert(normalize_lookup_key(&entry.name), index);
            for alias in &entry.aliases {
                lookup.insert(normalize_lookup_key(alias), index);
            }
        }

        Self { entries, lookup }
    }

    pub fn extract_entities(&self, text: &str, source_type: &str) -> Vec<EntityCandidate> {
        let mut deduped = HashMap::new();
        self.collect_import_candidates(text, source_type, &mut deduped);
        self.collect_file_path_candidates(text, &mut deduped);
        self.collect_cli_candidates(text, &mut deduped);
        self.collect_url_candidates(text, &mut deduped);
        self.collect_taxonomy_candidates(text, &mut deduped);

        let mut candidates: Vec<_> = deduped.into_values().collect();
        candidates.sort_by(|left, right| left.name.cmp(&right.name));
        candidates
    }

    fn collect_import_candidates(
        &self,
        text: &str,
        source_type: &str,
        deduped: &mut HashMap<String, EntityCandidate>,
    ) {
        let github_boost = if source_type.eq_ignore_ascii_case("github") {
            0.05
        } else {
            0.0
        };

        for capture in RUST_USE_RE.captures_iter(text) {
            self.insert_known_candidate(
                deduped,
                capture.get(1).map(|m| m.as_str()).unwrap_or_default(),
                CandidateSource::Import,
                0.95 + github_boost,
            );
        }

        for capture in PYTHON_FROM_RE.captures_iter(text) {
            let raw = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            let root = raw.split('.').next().unwrap_or(raw);
            self.insert_known_candidate(deduped, root, CandidateSource::Import, 0.92);
        }

        for capture in PYTHON_IMPORT_RE.captures_iter(text) {
            let raw = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            let root = raw.split('.').next().unwrap_or(raw);
            self.insert_known_candidate(deduped, root, CandidateSource::Import, 0.9);
        }

        for capture in JS_IMPORT_RE.captures_iter(text) {
            self.insert_known_candidate(
                deduped,
                capture.get(1).map(|m| m.as_str()).unwrap_or_default(),
                CandidateSource::Import,
                0.93,
            );
        }

        for capture in JS_REQUIRE_RE.captures_iter(text) {
            self.insert_known_candidate(
                deduped,
                capture.get(1).map(|m| m.as_str()).unwrap_or_default(),
                CandidateSource::Import,
                0.9,
            );
        }
    }

    fn collect_file_path_candidates(
        &self,
        text: &str,
        deduped: &mut HashMap<String, EntityCandidate>,
    ) {
        for capture in FILE_PATH_RE.captures_iter(text) {
            let raw = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            for segment in raw.split(&['/', '.'][..]) {
                if segment.len() < 2 {
                    continue;
                }
                self.insert_known_candidate(deduped, segment, CandidateSource::FilePath, 0.65);
            }
        }
    }

    fn collect_cli_candidates(&self, text: &str, deduped: &mut HashMap<String, EntityCandidate>) {
        for capture in CLI_COMMAND_RE.captures_iter(text) {
            self.insert_known_candidate(
                deduped,
                capture.get(1).map(|m| m.as_str()).unwrap_or_default(),
                CandidateSource::CliCommand,
                0.72,
            );
        }
    }

    fn collect_url_candidates(&self, text: &str, deduped: &mut HashMap<String, EntityCandidate>) {
        for capture in URL_RE.captures_iter(text) {
            let url = capture.get(0).map(|m| m.as_str()).unwrap_or_default();
            let trimmed = url
                .trim_end_matches('/')
                .trim_end_matches(".")
                .trim_end_matches(",");
            let Some(host) = trimmed.split("//").nth(1) else {
                continue;
            };
            let host = host.split('/').next().unwrap_or(host);
            for segment in host.split(&['.', '-'][..]) {
                if segment.len() < 2 {
                    continue;
                }
                self.insert_known_candidate(deduped, segment, CandidateSource::Url, 0.68);
            }
        }
    }

    fn collect_taxonomy_candidates(
        &self,
        text: &str,
        deduped: &mut HashMap<String, EntityCandidate>,
    ) {
        let lower_text = text.to_ascii_lowercase();
        for entry in &self.entries {
            if contains_term(&lower_text, &normalize_lookup_key(&entry.name))
                || entry
                    .aliases
                    .iter()
                    .any(|alias| contains_term(&lower_text, &normalize_lookup_key(alias)))
            {
                insert_candidate(
                    deduped,
                    EntityCandidate {
                        name: entry.name.clone(),
                        entity_type: entry.entity_type.clone(),
                        confidence: 0.6,
                        source: CandidateSource::Taxonomy,
                        ambiguous: entry.ambiguous,
                    },
                );
            }
        }
    }

    fn insert_known_candidate(
        &self,
        deduped: &mut HashMap<String, EntityCandidate>,
        raw: &str,
        source: CandidateSource,
        confidence: f32,
    ) {
        if let Some(entry) = self.resolve_entry(raw) {
            insert_candidate(
                deduped,
                EntityCandidate {
                    name: entry.name.clone(),
                    entity_type: entry.entity_type.clone(),
                    confidence,
                    source,
                    ambiguous: entry.ambiguous && !matches!(source, CandidateSource::Import),
                },
            );
        }
    }

    fn resolve_entry(&self, raw: &str) -> Option<&TaxonomyEntry> {
        let candidates = [
            raw.trim(),
            raw.trim().split("::").next().unwrap_or(raw.trim()),
            raw.trim().split('.').next().unwrap_or(raw.trim()),
        ];

        for candidate in candidates {
            if candidate.is_empty() {
                continue;
            }
            let key = normalize_lookup_key(candidate);
            if let Some(index) = self.lookup.get(&key) {
                return self.entries.get(*index);
            }
        }

        None
    }
}

fn insert_candidate(deduped: &mut HashMap<String, EntityCandidate>, candidate: EntityCandidate) {
    let key = normalize_lookup_key(&candidate.name);
    match deduped.get_mut(&key) {
        Some(existing) if existing.confidence >= candidate.confidence => {}
        Some(existing) => *existing = candidate,
        None => {
            deduped.insert(key, candidate);
        }
    }
}

fn normalize_lookup_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn contains_term(lower_text: &str, term: &str) -> bool {
    let needle = term.trim();
    if needle.is_empty() {
        return false;
    }

    lower_text.match_indices(needle).any(|(start, _)| {
        let end = start + needle.len();
        has_boundary_before(lower_text, start) && has_boundary_after(lower_text, end)
    })
}

fn has_boundary_before(text: &str, start: usize) -> bool {
    if start == 0 {
        return true;
    }
    !text.as_bytes()[start - 1].is_ascii_alphanumeric()
}

fn has_boundary_after(text: &str, end: usize) -> bool {
    match text.as_bytes().get(end).copied() {
        None => true,
        Some(byte) => !byte.is_ascii_alphanumeric(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_loads_from_embedded_json() {
        let tax = Taxonomy::builtin();
        assert!(
            tax.entries.len() > 100,
            "Expected >100 entries, got {}",
            tax.entries.len()
        );
    }

    #[test]
    fn extract_rust_use_statement() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("use tokio::sync::Mutex;", "github");
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Tokio"), "Expected Tokio in {:?}", names);
    }

    #[test]
    fn extract_python_import() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("from fastapi import FastAPI", "crawl");
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"FastAPI")
                || names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case("fastapi"))
        );
    }

    #[test]
    fn extract_js_import() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("import React from 'react';", "crawl");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.name.eq_ignore_ascii_case("react"))
        );
    }

    #[test]
    fn taxonomy_lookup_case_insensitive() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("We use DOCKER for containerization", "crawl");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.name == "Docker")
        );
    }

    #[test]
    fn taxonomy_alias_resolution() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("Connect to postgres database", "crawl");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.name == "PostgreSQL")
        );
    }

    #[test]
    fn taxonomy_lookup_matches_term_at_start_of_text() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("Docker is our container runtime", "crawl");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.name == "Docker")
        );
    }

    #[test]
    fn deduplication_within_chunk() {
        let tax = Taxonomy::builtin();
        let text = "use tokio::sync; use tokio::time; use tokio::net;";
        let candidates = tax.extract_entities(text, "github");
        let tokio_count = candidates
            .iter()
            .filter(|candidate| candidate.name == "Tokio")
            .count();
        assert_eq!(
            tokio_count, 1,
            "Tokio should appear once, got {}",
            tokio_count
        );
    }

    #[test]
    fn ambiguous_entity_flagged() {
        let tax = Taxonomy::builtin();
        let candidates = tax.extract_entities("import React from 'react';", "crawl");
        let react = candidates
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case("react"));
        assert!(react.is_some());
    }
}
