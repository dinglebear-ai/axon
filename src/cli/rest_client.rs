use crate::core::http::build_client;
use reqwest::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerFailureClass {
    TransportUnavailable,
    PolicyFailure,
    SchemaMismatch,
    InvalidRequest,
    ServerAcceptedOrUnknown,
}

pub struct RestClient {
    base_url: reqwest::Url,
    client: reqwest::Client,
    bearer_token: Option<String>,
}

#[derive(Debug)]
pub struct RestClientError {
    class: ServerFailureClass,
    message: String,
}

impl RestClientError {
    fn new(class: ServerFailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
        }
    }

    pub fn class(&self) -> ServerFailureClass {
        self.class
    }
}

impl fmt::Display for RestClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for RestClientError {}

impl From<reqwest::Error> for RestClientError {
    fn from(err: reqwest::Error) -> Self {
        Self::new(
            ServerFailureClass::TransportUnavailable,
            format!("server request failed: {err}"),
        )
    }
}

impl RestClient {
    pub fn new(base_url: reqwest::Url, timeout_secs: u64) -> Result<Self, RestClientError> {
        let client = build_client(timeout_secs, None).map_err(|err| {
            RestClientError::new(
                ServerFailureClass::TransportUnavailable,
                format!("build REST client: {err}"),
            )
        })?;
        Ok(Self {
            base_url,
            client,
            bearer_token: std::env::var("AXON_MCP_HTTP_TOKEN")
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty()),
        })
    }

    pub fn with_bearer_token(mut self, bearer_token: Option<String>) -> Self {
        self.bearer_token = bearer_token
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty());
        self
    }

    pub async fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R, RestClientError>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let request = self.authorize(self.client.post(endpoint.clone()).json(body));
        let response = request.send().await?;
        self.decode_response(response).await
    }

    pub async fn get_json<R>(&self, path: &str) -> Result<R, RestClientError>
    where
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let response = self.authorize(self.client.get(endpoint)).send().await?;
        self.decode_response(response).await
    }

    async fn decode_response<R>(&self, response: reqwest::Response) -> Result<R, RestClientError>
    where
        R: DeserializeOwned,
    {
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let class = classify_server_status(status, &text);
            return Err(RestClientError::new(
                class,
                format!("server returned {status}: {text} ({class:?})"),
            ));
        }
        response.json().await.map_err(|err| {
            RestClientError::new(
                ServerFailureClass::SchemaMismatch,
                format!("decode server response: {err}"),
            )
        })
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.bearer_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut endpoint = self.base_url.clone();
        let mut base_path = endpoint.path().trim_end_matches('/').to_string();
        if !base_path.is_empty() {
            base_path.push('/');
        }
        base_path.push_str(path.trim_start_matches('/'));
        endpoint.set_path(&base_path);
        endpoint
    }
}

pub fn classify_server_status(status: StatusCode, body: &str) -> ServerFailureClass {
    match status {
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            ServerFailureClass::TransportUnavailable
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ServerFailureClass::PolicyFailure,
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => ServerFailureClass::InvalidRequest,
        StatusCode::UPGRADE_REQUIRED => ServerFailureClass::SchemaMismatch,
        _ if body.to_ascii_lowercase().contains("schema") => ServerFailureClass::SchemaMismatch,
        _ => ServerFailureClass::ServerAcceptedOrUnknown,
    }
}
