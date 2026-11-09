use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use serde_json::json;
use tower::util::ServiceExt;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{host_pattern_matches, is_allowed_redirect_uri, validate_scope, wildcard_matches};
use crate::config::{AuthConfig, AuthMode, GoogleConfig};
use crate::google::GoogleProvider;
use crate::routes::router;
use crate::state::AuthState;
use crate::types::{AuthorizationRequestRow, RegisteredClient};

use crate::util::now_unix;

mod fixtures;
pub(crate) use fixtures::test_auth_state_with_mock_google;
use fixtures::{signed_test_id_token, test_jwks};
pub use fixtures::{
    test_auth_config, test_auth_state, test_auth_state_with_config,
    test_auth_state_with_registered_client,
};
mod allowlist_tests;
mod callback;
mod flow;
mod merged_allowlist_callback_tests;
mod registration;
mod validation;

mod callback_logs;
