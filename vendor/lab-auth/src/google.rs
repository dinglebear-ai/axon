use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Header, Validation, decode, decode_header};
use reqwest::Url;
use reqwest::header;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::AuthError;
use crate::util::fingerprint;

const GOOGLE_ISSUER: &str = "https://accounts.google.com";
const GOOGLE_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Per-request timeout on the JWKS GET. Bound aggressively (5s) so a slow
/// google.com call cannot starve a tokio worker holding the JWKS write
/// lock. Token exchange / refresh keep the looser 30s bound because they
/// can legitimately take longer.
const GOOGLE_JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const GOOGLE_DEFAULT_JWKS_TTL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizeUrlRequest {
    pub state: String,
    pub scope: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
}

#[derive(Clone)]
pub struct GoogleProvider {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub http: reqwest::Client,
    authorize_endpoint: Url,
    token_endpoint: Url,
    jwks_endpoint: Url,
    issuer: String,
    jwks_cache: Arc<RwLock<Option<CachedGoogleJwks>>>,
}

impl std::fmt::Debug for GoogleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleProvider")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleExchange {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub id_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    id_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleIdTokenClaims {
    iss: String,
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct GoogleJwks {
    keys: Vec<GoogleJwk>,
}

#[derive(Clone, Debug, Deserialize)]
struct GoogleJwk {
    kid: String,
    #[serde(default)]
    alg: Option<String>,
    n: String,
    e: String,
}

#[derive(Clone, Debug)]
struct CachedGoogleJwks {
    jwks: GoogleJwks,
    expires_at: Instant,
}

mod request;
use request::{GoogleRequestErrors, GoogleRequestTrace, read_json_response};

impl GoogleProvider {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: Url,
    ) -> Result<Self, AuthError> {
        let http = reqwest::Client::builder()
            .timeout(GOOGLE_HTTP_TIMEOUT)
            .build()
            .map_err(|error| {
                AuthError::Storage(format!("build google oauth http client: {error}"))
            })?;
        let authorize_endpoint = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
            .map_err(|error| {
                AuthError::Config(format!("parse google authorize endpoint: {error}"))
            })?;
        let token_endpoint = Url::parse("https://oauth2.googleapis.com/token")
            .map_err(|error| AuthError::Config(format!("parse google token endpoint: {error}")))?;
        let jwks_endpoint = Url::parse("https://www.googleapis.com/oauth2/v3/certs")
            .map_err(|error| AuthError::Config(format!("parse google jwks endpoint: {error}")))?;

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            http,
            authorize_endpoint,
            token_endpoint,
            jwks_endpoint,
            issuer: GOOGLE_ISSUER.to_string(),
            jwks_cache: Arc::new(RwLock::new(None)),
        })
    }

    #[must_use]
    pub fn with_identity_provider(
        mut self,
        authorize_endpoint: Url,
        token_endpoint: Url,
        jwks_endpoint: Url,
        issuer: String,
    ) -> Self {
        self.authorize_endpoint = authorize_endpoint;
        self.token_endpoint = token_endpoint;
        self.jwks_endpoint = jwks_endpoint;
        self.issuer = issuer;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_endpoints(mut self, authorize_endpoint: Url, token_endpoint: Url) -> Self {
        self.authorize_endpoint = authorize_endpoint;
        self.token_endpoint = token_endpoint;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_jwks_endpoint(mut self, jwks_endpoint: Url) -> Self {
        self.jwks_endpoint = jwks_endpoint;
        self
    }

    pub fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError> {
        let mut url = self.authorize_endpoint.clone();
        let scope = self.scopes.join(" ");
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("response_type", "code")
            .append_pair("scope", &scope)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("include_granted_scopes", "true")
            .append_pair("state", &request.state)
            .append_pair("code_challenge", &request.code_challenge)
            .append_pair("code_challenge_method", &request.code_challenge_method);
        debug!(
            provider = "google",
            oauth_state_id = %fingerprint(&request.state),
            scope = %scope,
            redirect_uri = %self.redirect_uri,
            "oauth upstream authorize URL constructed"
        );
        Ok(url)
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<GoogleExchange, AuthError> {
        let trace = GoogleRequestTrace::start("code_exchange", "POST", &self.token_endpoint);
        info!(
            provider = "google",
            oauth_code_id = %fingerprint(code),
            redirect_uri = %self.redirect_uri,
            "oauth upstream code exchange started"
        );
        let payload: GoogleTokenResponse = read_json_response(
            trace,
            self.http.post(self.token_endpoint.clone()).form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ]),
            GoogleRequestErrors {
                transport_context: "exchange google auth code",
                transport_log: "oauth upstream code exchange request failed",
                status_context: "google token endpoint error",
                status_log: "oauth upstream code exchange returned error status",
                decode_context: "decode google token response",
                decode_log: "oauth upstream code exchange returned an unreadable payload",
            },
        )
        .await?;
        let claims = self.verify_id_token(&payload.id_token).await?;
        info!(
            provider = "google",
            subject_id = %fingerprint(&claims.sub),
            has_refresh_token = payload.refresh_token.is_some(),
            expires_in_secs = payload.expires_in,
            "oauth upstream code exchange succeeded"
        );
        Ok(GoogleExchange {
            subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified,
            access_token: payload.access_token,
            refresh_token: payload.refresh_token,
            expires_in: payload.expires_in,
            id_token: payload.id_token,
        })
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<GoogleExchange, AuthError> {
        let trace = GoogleRequestTrace::start("refresh", "POST", &self.token_endpoint);
        info!(
            provider = "google",
            refresh_token_id = %fingerprint(refresh_token),
            "oauth upstream refresh started"
        );
        let payload: GoogleTokenResponse = read_json_response(
            trace,
            self.http.post(self.token_endpoint.clone()).form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ]),
            GoogleRequestErrors {
                transport_context: "refresh google token",
                transport_log: "oauth upstream refresh request failed",
                status_context: "google refresh endpoint error",
                status_log: "oauth upstream refresh returned error status",
                decode_context: "decode google refresh response",
                decode_log: "oauth upstream refresh returned an unreadable payload",
            },
        )
        .await?;
        let claims = self.verify_id_token(&payload.id_token).await?;
        info!(
            provider = "google",
            subject_id = %fingerprint(&claims.sub),
            has_refresh_token = payload.refresh_token.is_some(),
            expires_in_secs = payload.expires_in,
            "oauth upstream refresh succeeded"
        );
        Ok(GoogleExchange {
            subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified,
            access_token: payload.access_token,
            refresh_token: payload.refresh_token,
            expires_in: payload.expires_in,
            id_token: payload.id_token,
        })
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdTokenClaims, AuthError> {
        let header = decode_header(id_token)
            .map_err(|error| AuthError::Storage(format!("verify google id_token: {error}")))?;
        validate_id_token_header(&header)?;
        let kid = header
            .kid
            .ok_or_else(|| AuthError::Storage("google id_token is missing a key id".to_string()))?;
        let key = self.find_jwk_for_kid(&kid).await?;
        if let Some(alg) = key.alg.as_deref()
            && alg != "RS256"
        {
            return Err(AuthError::Storage(format!(
                "google JWKS key `{}` uses unsupported algorithm `{alg}`",
                key.kid
            )));
        }

        let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e).map_err(|error| {
            AuthError::Storage(format!("build google id_token decoding key: {error}"))
        })?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        validation.leeway = 0;
        validation.set_audience(&[self.client_id.as_str()]);

        let claims = decode::<GoogleIdTokenClaims>(id_token, &decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|error| AuthError::Storage(format!("invalid google id_token: {error}")))?;

        let legacy_google_issuer =
            self.issuer == GOOGLE_ISSUER && claims.iss == "accounts.google.com";
        if claims.iss != self.issuer && !legacy_google_issuer {
            return Err(AuthError::Storage(format!(
                "invalid google id_token issuer `{}`",
                claims.iss
            )));
        }

        Ok(claims)
    }

    async fn find_jwk_for_kid(&self, kid: &str) -> Result<GoogleJwk, AuthError> {
        let jwks = self.fetch_jwks().await?;
        if let Some(key) = jwks.keys.into_iter().find(|key| key.kid == kid) {
            return Ok(key);
        }

        debug!(
            provider = "google",
            kid, "google jwks cache miss for token key id; refreshing"
        );
        self.refresh_jwks()
            .await?
            .keys
            .into_iter()
            .find(|key| key.kid == kid)
            .ok_or_else(|| {
                AuthError::Storage("google id_token key id was not found in JWKS".to_string())
            })
    }

    async fn fetch_jwks(&self) -> Result<GoogleJwks, AuthError> {
        if let Some(jwks) = self.cached_jwks().await {
            debug!(provider = "google", "google jwks cache hit");
            return Ok(jwks);
        }

        let jwks = {
            let mut cache = self.jwks_cache.write().await;
            if let Some(cached) = cache
                .as_ref()
                .filter(|cached| cached.expires_at > Instant::now())
            {
                debug!(
                    provider = "google",
                    "google jwks cache hit after refresh lock"
                );
                cached.jwks.clone()
            } else {
                Self::refresh_jwks_locked(&self.http, &self.jwks_endpoint, &mut cache).await?
            }
        };
        Ok(jwks)
    }

    async fn refresh_jwks(&self) -> Result<GoogleJwks, AuthError> {
        let jwks = {
            let mut cache = self.jwks_cache.write().await;
            Self::refresh_jwks_locked(&self.http, &self.jwks_endpoint, &mut cache).await?
        };
        Ok(jwks)
    }

    async fn refresh_jwks_locked(
        http: &reqwest::Client,
        jwks_endpoint: &Url,
        cache: &mut Option<CachedGoogleJwks>,
    ) -> Result<GoogleJwks, AuthError> {
        let trace = GoogleRequestTrace::start("fetch_jwks", "GET", jwks_endpoint);
        let response = http
            .get(jwks_endpoint.clone())
            .timeout(GOOGLE_JWKS_FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                trace.error(None, &error);
                warn!(provider = "google", error = %error, "google jwks request failed");
                AuthError::Storage(format!("fetch google jwks: {error}"))
            })?;
        let status = response.status();
        let ttl = google_jwks_ttl(response.headers());
        let response = response.error_for_status().map_err(|error| {
            trace.error(Some(status), &error);
            warn!(provider = "google", error = %error, "google jwks request returned error status");
            AuthError::Storage(format!("google jwks endpoint error: {error}"))
        })?;
        trace.finish(status);
        let jwks = response.json::<GoogleJwks>().await.map_err(|error| {
            warn!(provider = "google", error = %error, "google jwks payload unreadable");
            AuthError::Storage(format!("decode google jwks response: {error}"))
        })?;

        *cache = Some(CachedGoogleJwks {
            jwks: jwks.clone(),
            expires_at: Instant::now() + ttl,
        });

        Ok(jwks)
    }

    async fn cached_jwks(&self) -> Option<GoogleJwks> {
        let cache = self.jwks_cache.read().await;
        cache
            .as_ref()
            .filter(|cached| cached.expires_at > Instant::now())
            .map(|cached| cached.jwks.clone())
    }
}

fn google_jwks_ttl(headers: &header::HeaderMap) -> Duration {
    headers
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_max_age)
        .map_or(GOOGLE_DEFAULT_JWKS_TTL, Duration::from_secs)
}

fn parse_max_age(cache_control: &str) -> Option<u64> {
    cache_control.split(',').find_map(|directive| {
        let directive = directive.trim();
        let value = directive.strip_prefix("max-age=")?;
        value.parse::<u64>().ok()
    })
}

fn validate_id_token_header(header: &Header) -> Result<(), AuthError> {
    if header.alg != Algorithm::RS256 {
        return Err(AuthError::Storage(format!(
            "verify google id_token: unsupported algorithm `{:?}`",
            header.alg
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests;
