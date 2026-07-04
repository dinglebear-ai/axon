use serde::Serialize;

use super::SchemaFamily;

#[derive(Debug, Clone, Serialize)]
pub struct FamilyReport {
    pub family: SchemaFamily,
    pub ok: bool,
    pub artifacts_checked: usize,
    pub fixtures_validated: usize,
    pub snapshots_checked: usize,
    pub drift: Vec<String>,
    pub warnings: Vec<String>,
}

impl FamilyReport {
    pub fn ok(family: SchemaFamily, artifacts_checked: usize) -> Self {
        Self {
            family,
            ok: true,
            artifacts_checked,
            fixtures_validated: 0,
            snapshots_checked: 0,
            drift: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn from_drift(family: SchemaFamily, artifacts_checked: usize, drift: Vec<String>) -> Self {
        Self {
            family,
            ok: drift.is_empty(),
            artifacts_checked,
            fixtures_validated: 0,
            snapshots_checked: 0,
            drift,
            warnings: Vec::new(),
        }
    }

    pub fn with_validation_counts(mut self, validation: &FamilyReport) -> Self {
        self.fixtures_validated = validation.fixtures_validated;
        self.snapshots_checked = validation.snapshots_checked;
        self.warnings.extend(validation.warnings.iter().cloned());
        self
    }
}
