use crate::core::http::build_client;
use reqwest::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;

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
}

impl RestClient {
    pub fn new(base_url: reqwest::Url, timeout_secs: u64) -> Result<Self, Box<dyn Error>> {
        let client = build_client(timeout_secs, None)?;
        Ok(Self { base_url, client })
    }

    pub async fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R, Box<dyn Error>>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let response = self.client.post(endpoint.clone()).json(body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "server returned {status}: {text} ({:?})",
                classify_server_status(status, &text)
            )
            .into());
        }
        Ok(response.json().await?)
    }

    pub async fn get_json<R>(&self, path: &str) -> Result<R, Box<dyn Error>>
    where
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let response = self.client.get(endpoint.clone()).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "server returned {status}: {text} ({:?})",
                classify_server_status(status, &text)
            )
            .into());
        }
        Ok(response.json().await?)
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
