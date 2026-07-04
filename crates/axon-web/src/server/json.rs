//! Envelope-aware JSON extractor + router fallbacks.
//!
//! axum's built-in [`axum::Json<T>`] extractor rejects a malformed or
//! missing-field body with a *raw* `422`/`400` (`Failed to deserialize the JSON
//! body: missing field ...`) **before** the handler runs, bypassing the REST
//! [`ErrorEnvelope`]. Per `docs/pipeline-unification/surfaces/rest-contract.md`
//! (§Shared Response Envelope) every non-stream REST failure must serialize as
//! the contract envelope with a stable `error.code`/`error.stage`.
//!
//! [`Json<T>`] here is a drop-in replacement newtype: it delegates extraction
//! and serialization to axum's `Json`, but maps every [`JsonRejection`] onto an
//! `ErrorEnvelope` whose code is `route.validation.invalid_body` (stage
//! `validation`) and whose HTTP status matches the rejection class. Handlers use
//! it exactly like `axum::Json`.
//!
//! [`not_found_fallback`] and [`method_not_allowed_fallback`] close the same gap
//! for unrouted paths (404) and wrong-method requests (405), which axum
//! otherwise answers with an empty body.

use axon_api::ApiError;
use axon_error::ErrorStage;
use axum::{
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::api_error::error_envelope_response_with_status;

/// Drop-in replacement for [`axum::Json`] whose rejection serializes as the
/// contract [`axon_api::source::ErrorEnvelope`] instead of a raw axum body.
pub(crate) struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Json(value)),
            Err(rejection) => Err(json_rejection_response(rejection)),
        }
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

/// Map an axum [`JsonRejection`] to a contract `ErrorEnvelope` response.
///
/// All body-shape rejections (missing field, unknown field, syntax error, wrong
/// content-type, body-read failure) collapse to `route.validation.invalid_body`
/// at the `validation` stage. The HTTP status is taken verbatim from the
/// rejection (`400`/`415`/`422`) so clients keep the same code axum would have
/// returned; only the body is upgraded to the contract envelope.
pub(crate) fn json_rejection_response(rejection: JsonRejection) -> Response {
    let status = rejection.status();
    let error = ApiError::new(
        "route.validation.invalid_body",
        ErrorStage::Validation,
        rejection.body_text(),
    );
    error_envelope_response_with_status(error, status)
}

/// Router `.fallback` — enveloped `404` for any unrouted path.
///
/// Distinct from the panel/static asset fallback: this is installed on the
/// `/v1` REST surface so unknown API routes return the contract envelope.
pub(crate) async fn not_found_fallback() -> Response {
    let error = ApiError::new("route.not_found", ErrorStage::Routing, "route not found");
    error_envelope_response_with_status(error, StatusCode::NOT_FOUND)
}

/// Router `.method_not_allowed_fallback` — enveloped `405`.
pub(crate) async fn method_not_allowed_fallback() -> Response {
    let error = ApiError::new(
        "route.method_not_allowed",
        ErrorStage::Routing,
        "method not allowed",
    );
    error_envelope_response_with_status(error, StatusCode::METHOD_NOT_ALLOWED)
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
