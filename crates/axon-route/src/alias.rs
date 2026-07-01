//! Alias records used by authority resolution.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasRecord {
    pub alias: String,
    pub authority_id: String,
    pub reason: String,
}

impl AliasRecord {
    pub fn new(
        alias: impl Into<String>,
        authority_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            alias: alias.into(),
            authority_id: authority_id.into(),
            reason: reason.into(),
        }
    }
}
