#[derive(Clone)]
pub(crate) struct QdrantEndpoint {
    base: String,
    api_key: Option<String>,
    pub(crate) valid: bool,
}

impl std::fmt::Debug for QdrantEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QdrantEndpoint")
            .field("base", &self.base)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl QdrantEndpoint {
    pub(crate) fn parse(url: &str) -> Self {
        match url::Url::parse(url.trim()) {
            Ok(parsed)
                if matches!(parsed.scheme(), "http" | "https") && parsed.host().is_some() =>
            {
                let mut api_key = parsed
                    .password()
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        (!parsed.username().is_empty()).then(|| parsed.username().to_string())
                    });
                if api_key.is_none() {
                    api_key = parsed
                        .query_pairs()
                        .find(|(key, _)| key == "api_key")
                        .map(|(_, value)| value.into_owned());
                }
                let mut redacted = parsed;
                let _ = redacted.set_username("");
                let _ = redacted.set_password(None);
                redacted.set_query(None);
                redacted.set_fragment(None);
                let api_key = api_key.or_else(|| {
                    std::env::var("QDRANT_API_KEY")
                        .ok()
                        .filter(|value| !value.is_empty())
                });
                Self {
                    base: redacted.as_str().trim_end_matches('/').to_string(),
                    api_key,
                    valid: true,
                }
            }
            _ => Self {
                base: "http://invalid.invalid".to_string(),
                api_key: None,
                valid: false,
            },
        }
    }

    pub(crate) fn collection_path(&self, collection: &str, suffix: &str) -> String {
        let mut url = url::Url::parse(&format!("{}/", self.base)).expect("validated endpoint");
        let (path, query) = suffix
            .trim_start_matches('/')
            .split_once('?')
            .unwrap_or((suffix.trim_start_matches('/'), ""));
        {
            let mut segments = url.path_segments_mut().expect("HTTP URL");
            segments.pop_if_empty().push("collections").push(collection);
            for segment in path.split('/').filter(|value| !value.is_empty()) {
                segments.push(segment);
            }
        }
        if !query.is_empty() {
            url.set_query(Some(query));
        }
        url.into()
    }

    pub(crate) fn service_path(&self, suffix: &str) -> String {
        let mut url = url::Url::parse(&format!("{}/", self.base)).expect("validated endpoint");
        {
            let mut segments = url.path_segments_mut().expect("HTTP URL");
            for segment in suffix
                .trim_matches('/')
                .split('/')
                .filter(|value| !value.is_empty())
            {
                segments.push(segment);
            }
        }
        url.into()
    }

    pub(crate) fn root(&self) -> &str {
        &self.base
    }
    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
    pub(crate) fn grpc_origin(&self) -> String {
        let Ok(mut url) = url::Url::parse(&self.base) else {
            return self.base.clone();
        };
        url.set_path("");
        url.set_query(None);
        url.set_fragment(None);
        url.as_str().trim_end_matches('/').to_string()
    }
    pub(crate) fn credentials_use_safe_transport(&self) -> bool {
        self.transport_is_safe_for_credentials(self.api_key.is_some())
    }
    pub(crate) fn transport_is_safe_for_credentials(&self, present: bool) -> bool {
        !present
            || url::Url::parse(&self.base).is_ok_and(|url| {
                url.scheme() == "https"
                    || matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
            })
    }
}
