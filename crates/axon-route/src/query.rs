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
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("signature")
        || key.contains("credential")
        || key == "access_key"
        || key == "awsaccesskeyid"
        || key.starts_with("x-amz-")
        || key == "sig"
        || key == "jwt"
        || key == "key"
        || key == "api_key"
        || key == "apikey"
        || key == "auth"
        || key == "authorization"
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
