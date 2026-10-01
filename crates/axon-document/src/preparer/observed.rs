//! Preserve parser outcomes separately from quality and chunk routing.
use super::*;

impl DocumentPreparer {
    pub(super) fn selected_profile(
        &self,
        request: &PrepareSourceDocumentRequest,
        parse: &DocumentParse,
    ) -> Result<ChunkingProfile, String> {
        match request.profile {
            Some(profile) => Ok(profile),
            None => parse
                .routed_profile()
                .map(Ok)
                .unwrap_or_else(|| self.router.route(&request.document)),
        }
    }

    pub fn prepare(
        &self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        self.prepare_observed(request).map(|(result, _)| result)
    }

    /// Prepare once and retain parser outcomes independently of searchable output.
    pub fn prepare_observed(
        &self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<(PrepareSourceDocumentResult, PreparationObservation), String> {
        let mut observation = PreparationObservation::default();
        let result = self.prepare_inner(request, &mut observation)?;
        Ok((result, observation))
    }
}

pub(super) fn record_observation(
    request: &mut PrepareSourceDocumentRequest,
    observation: &mut PreparationObservation,
) {
    *observation = request
        .parse_facts
        .iter()
        .find(|fact| fact.fact_kind == "code_parse_outcome")
        .map(|fact| PreparationObservation::from_value(&fact.value))
        .unwrap_or_default();
    for key in [
        "code_ast_status",
        "code_grammar",
        "code_symbol_count",
        "symbol_extraction_status",
    ] {
        request.document.metadata.remove(key);
    }
    if let Some(outcome) = observation
        .code_parse_outcome
        .as_ref()
        .and_then(serde_json::Value::as_object)
    {
        for key in [
            "code_ast_status",
            "code_grammar",
            "code_symbol_count",
            "symbol_extraction_status",
        ] {
            if let Some(value) = outcome.get(key).filter(|value| !value.is_null()) {
                request
                    .document
                    .metadata
                    .insert(key.to_string(), value.clone());
            }
        }
    }
}
