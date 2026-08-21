use axon_api::source::ApiError;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WebUrlParts {
    pub normalized_url: String,
    pub item_key: String,
    pub domain: String,
    pub origin: String,
    pub path: String,
}

impl WebUrlParts {
    pub(super) fn parse(raw: &str) -> Result<Self, ApiError> {
        let mut url = Url::parse(raw).map_err(|err| {
            ApiError::new(
                "adapter.web.url.invalid",
                axon_error::ErrorStage::Normalizing,
                err.to_string(),
            )
            .with_context("url", redacted_url(raw))
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ApiError::new(
                "adapter.web.url.unsupported_scheme",
                axon_error::ErrorStage::Normalizing,
                "web adapter only supports http and https URLs",
            )
            .with_context("url", redacted_url(raw)));
        }
        url.set_fragment(None);
        let _ = url.set_username("");
        let _ = url.set_password(None);
        normalize_query(&mut url);

        let domain = url.host_str().unwrap_or_default().to_string();
        let origin = match url.port() {
            Some(port) => format!("{}://{}:{port}", url.scheme(), domain),
            None => format!("{}://{}", url.scheme(), domain),
        };
        let path = clean_path(url.path());
        url.set_path(&path);
        let normalized_url = url.to_string().trim_end_matches('/').to_string();
        let item_key = web_item_key(&path, url.query());
        Ok(Self {
            normalized_url,
            item_key,
            domain,
            origin,
            path,
        })
    }
}

fn normalize_query(url: &mut Url) {
    let mut pairs = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_or_sensitive_query(key))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    if !pairs.is_empty() {
        let mut serializer = url.query_pairs_mut();
        for (key, value) in pairs {
            serializer.append_pair(&key, &value);
        }
    }
}

fn is_tracking_or_sensitive_query(key: &str) -> bool {
    let normalized = axon_core::redact::normalize_field_name(key);
    if normalized.starts_with("utm_")
        || matches!(
            normalized.as_str(),
            "fbclid" | "gclid" | "mc_cid" | "mc_eid"
        )
    {
        return true;
    }
    if matches!(
        normalized.as_str(),
        "page_token"
            | "next_page_token"
            | "continuation_token"
            | "pagination_token"
            | "cursor_token"
            | "token_count"
            | "token_estimate"
            | "tokenizer"
            | "tokenization"
            | "token_budget"
            | "x_amz_algorithm"
            | "x_amz_date"
            | "x_amz_expires"
            | "x_amz_signedheaders"
            | "x_goog_algorithm"
            | "x_goog_date"
            | "x_goog_expires"
            | "x_goog_signedheaders"
    ) {
        return false;
    }
    axon_core::redact::is_secret_like(key)
        || matches!(
            normalized.as_str(),
            "auth"
                | "authorization"
                | "code"
                | "jwt"
                | "key"
                | "access_key"
                | "awsaccesskeyid"
                | "awsaccess_key_id"
                | "session"
                | "session_id"
                | "signature"
                | "sig"
                | "x_amz_signature"
                | "x_amz_credential"
                | "x_amz_security_token"
                | "x_goog_signature"
                | "x_goog_credential"
        )
        || normalized.ends_with("_signature")
}

fn clean_path(path: &str) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let mut collapsed = String::with_capacity(path.len());
    let mut previous_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if !previous_slash {
                collapsed.push(ch);
            }
            previous_slash = true;
        } else {
            collapsed.push(ch);
            previous_slash = false;
        }
    }
    if collapsed == "/" {
        collapsed
    } else {
        collapsed.trim_end_matches('/').to_string()
    }
}

fn web_item_key(path: &str, query: Option<&str>) -> String {
    let key = path.trim_matches('/');
    let path_key = if key.is_empty() {
        "index".to_string()
    } else {
        key.to_string()
    };
    match query {
        Some(query) if !query.trim().is_empty() => format!("{path_key}?{query}"),
        _ => path_key,
    }
}

fn redacted_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            if url.query().is_some() {
                let pairs = url
                    .query_pairs()
                    .map(|(key, _)| (key.to_string(), "REDACTED".to_string()))
                    .collect::<Vec<_>>();
                url.set_query(None);
                let mut serializer = url.query_pairs_mut();
                for (key, value) in pairs {
                    serializer.append_pair(&key, &value);
                }
            }
            url.to_string()
        }
        Err(_) => {
            let without_query = raw.split('?').next().unwrap_or(raw);
            match without_query.rfind('@') {
                Some(at) => {
                    let authority_start = without_query.find("://").map_or(0, |idx| idx + 3);
                    format!(
                        "{}REDACTED@{}",
                        &without_query[..authority_start],
                        &without_query[at + 1..]
                    )
                }
                None => without_query.to_string(),
            }
        }
    }
}
