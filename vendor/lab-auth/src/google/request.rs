use super::*;

pub(super) struct GoogleRequestTrace<'a> {
    pub(super) operation: &'static str,
    pub(super) method: &'static str,
    pub(super) endpoint: &'a Url,
    pub(super) started: Instant,
}

impl<'a> GoogleRequestTrace<'a> {
    pub(super) fn start(operation: &'static str, method: &'static str, endpoint: &'a Url) -> Self {
        info!(
            provider = "google",
            operation,
            method,
            host = endpoint.host_str().unwrap_or_default(),
            path = endpoint.path(),
            "request.start"
        );
        Self {
            operation,
            method,
            endpoint,
            started: Instant::now(),
        }
    }

    pub(super) fn finish(&self, status: reqwest::StatusCode) {
        info!(
            provider = "google",
            operation = self.operation,
            method = self.method,
            host = self.endpoint.host_str().unwrap_or_default(),
            path = self.endpoint.path(),
            status = status.as_u16(),
            elapsed_ms = self.started.elapsed().as_millis(),
            "request.finish"
        );
    }

    pub(super) fn error(&self, status: Option<reqwest::StatusCode>, error: &reqwest::Error) {
        if let Some(status) = status {
            warn!(
                provider = "google",
                operation = self.operation,
                method = self.method,
                host = self.endpoint.host_str().unwrap_or_default(),
                path = self.endpoint.path(),
                status = status.as_u16(),
                elapsed_ms = self.started.elapsed().as_millis(),
                error = %error,
                "request.error"
            );
        } else {
            warn!(
                provider = "google",
                operation = self.operation,
                method = self.method,
                host = self.endpoint.host_str().unwrap_or_default(),
                path = self.endpoint.path(),
                elapsed_ms = self.started.elapsed().as_millis(),
                error = %error,
                "request.error"
            );
        }
    }
}

pub(super) struct GoogleRequestErrors {
    pub(super) transport_context: &'static str,
    pub(super) transport_log: &'static str,
    pub(super) status_context: &'static str,
    pub(super) status_log: &'static str,
    pub(super) decode_context: &'static str,
    pub(super) decode_log: &'static str,
}

pub(super) async fn read_json_response<T: DeserializeOwned>(
    trace: GoogleRequestTrace<'_>,
    request: reqwest::RequestBuilder,
    errors: GoogleRequestErrors,
) -> Result<T, AuthError> {
    let response = request.send().await.map_err(|error| {
        let auth_error = AuthError::Network(format!("{}: {error}", errors.transport_context));
        trace.error(None, &error);
        warn!(
            provider = "google",
            error = %error,
            kind = auth_error.kind(),
            "{}",
            errors.transport_log
        );
        auth_error
    })?;
    let status = response.status();
    let retry_after_ms = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000));
    let response = response.error_for_status().map_err(|error| {
        let auth_error = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            AuthError::RateLimited {
                message: format!("{}: {}", errors.status_context, status),
                retry_after_ms: retry_after_ms.unwrap_or(1_000),
            }
        } else if status.is_server_error() {
            AuthError::Server(format!("{}: {error}", errors.status_context))
        } else {
            AuthError::AuthFailed(format!("{}: {error}", errors.status_context))
        };
        trace.error(Some(status), &error);
        warn!(
            provider = "google",
            error = %error,
            kind = auth_error.kind(),
            "{}",
            errors.status_log
        );
        auth_error
    })?;
    trace.finish(status);
    response.json::<T>().await.map_err(|error| {
        let auth_error = AuthError::Decode(format!("{}: {error}", errors.decode_context));
        warn!(
            provider = "google",
            error = %error,
            kind = auth_error.kind(),
            "{}",
            errors.decode_log
        );
        auth_error
    })
}
