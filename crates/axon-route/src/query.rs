use axon_api::{Severity, SourceWarning};
use url::{Url, form_urlencoded};

pub(crate) struct QueryNormalization {
    pub(crate) query: String,
    pub(crate) warnings: Vec<SourceWarning>,
}

pub(crate) fn normalized_query(url: &Url) -> QueryNormalization {
    let mut kept = Vec::new();
    let mut redacted = false;
    for (key, value) in url.query_pairs() {
        let key = key.to_string();
        let value = value.to_string();
        if is_tracking_param(&key) {
            continue;
        }
        if is_sensitive_param(&key) {
            redacted = true;
            kept.push((key, "REDACTED".to_string()));
        } else {
            kept.push((key, value));
        }
    }
    kept.sort();

    let query = if kept.is_empty() {
        String::new()
    } else {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in kept {
            serializer.append_pair(&key, &value);
        }
        format!("?{}", serializer.finish())
    };

    let warnings = if redacted {
        vec![warning()]
    } else {
        Vec::new()
    };

    QueryNormalization { query, warnings }
}

pub(crate) fn sensitive_query_warnings(url: &Url) -> Vec<SourceWarning> {
    if url.query_pairs().any(|(key, _)| is_sensitive_param(&key)) {
        vec![warning()]
    } else {
        Vec::new()
    }
}

fn is_tracking_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "msclkid" | "mc_cid" | "mc_eid" | "igshid"
        )
}

fn is_sensitive_param(key: &str) -> bool {
    let key = axon_core::redact::normalize_field_name(key);
    if key.ends_with("_count")
        || key.ends_with("_estimate")
        || key.ends_with("_policy")
        || key.ends_with("_status")
        || key.ends_with("_type")
        || key.ends_with("_enabled")
        || key.ends_with("_identifier")
        || matches!(
            key.as_str(),
            "tokenizer"
                | "tokenization"
                | "token_budget"
                | "page_token"
                | "next_page_token"
                | "continuation_token"
                | "pagination_token"
                | "cursor_token"
        )
    {
        return false;
    }
    matches!(
        key.as_str(),
        "token"
            | "secret"
            | "password"
            | "passwd"
            | "credential"
            | "credentials"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "private_key"
            | "secret_key"
            | "client_secret"
            | "access_key"
            | "awsaccesskeyid"
            | "awsaccess_key_id"
            | "sig"
            | "signature"
            | "jwt"
            | "key"
            | "api_key"
            | "apikey"
            | "auth"
            | "authorization"
    ) || key.ends_with("_token")
        || key.ends_with("_secret")
        || key.ends_with("_password")
        || key.ends_with("_passwd")
        || key.ends_with("_credential")
        || key.ends_with("_credentials")
        || key.ends_with("_signature")
}

fn warning() -> SourceWarning {
    SourceWarning {
        code: "source.query.sensitive_redacted".to_string(),
        severity: Severity::Info,
        message: "sensitive query parameter values were redacted in canonical URI".to_string(),
        source_item_key: None,
        retryable: false,
    }
}
