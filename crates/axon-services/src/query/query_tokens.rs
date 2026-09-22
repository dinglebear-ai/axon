//! Shared query token policy for retrieval ranking and answer validation.

use std::collections::HashSet;

const STOP_WORDS: &[&str] = &[
    "a", "am", "an", "and", "any", "are", "as", "at", "be", "but", "by", "can", "do", "does",
    "for", "from", "had", "has", "have", "he", "her", "him", "his", "how", "if", "in", "into",
    "is", "it", "its", "me", "my", "no", "not", "of", "on", "or", "our", "out", "she", "so",
    "than", "that", "the", "their", "them", "then", "they", "this", "to", "too", "up", "us", "via",
    "was", "we", "were", "what", "when", "where", "who", "why", "you", "your",
];

pub(crate) fn query_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2 && !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

pub(crate) fn tokenize_text_set(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3 && !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

pub(crate) fn tokenize_path_set(path_or_url: &str) -> HashSet<String> {
    path_or_url
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(str::to_string)
        .collect()
}

pub(crate) fn query_wants_low_signal_sources(tokens: &[String], raw_query: &str) -> bool {
    if raw_query.to_ascii_lowercase().contains("docs/sessions") {
        return true;
    }
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "session"
                | "sessions"
                | "transcript"
                | "transcripts"
                | "log"
                | "logs"
                | "history"
                | "histories"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_tokens_drop_stop_words_and_short_terms() {
        assert_eq!(
            query_tokens("how do I use the API for this"),
            vec!["use", "api"]
        );
    }

    #[test]
    fn low_signal_intent_is_explicit() {
        assert!(query_wants_low_signal_sources(
            &query_tokens("show session history"),
            "show session history"
        ));
        assert!(!query_wants_low_signal_sources(
            &query_tokens("configure hooks"),
            "configure hooks"
        ));
    }
}
