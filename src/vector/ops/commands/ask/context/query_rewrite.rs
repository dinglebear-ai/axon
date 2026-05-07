use crate::vector::ops::ranking;

pub(super) struct AskQueryForms {
    pub(super) query_tokens: Vec<String>,
    pub(super) keyword_query: String,
    pub(super) use_dual: bool,
}

pub(super) fn build_query_forms(query: &str) -> AskQueryForms {
    let query_tokens = ranking::tokenize_query(query);
    let keyword_query = query_tokens.join(" ");
    let use_dual =
        query_tokens.len() >= 3 && keyword_query.to_lowercase() != query.trim().to_lowercase();
    AskQueryForms {
        query_tokens,
        keyword_query,
        use_dual,
    }
}
