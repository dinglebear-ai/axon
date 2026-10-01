/// Fixed, non-sensitive classifications for chunks rejected by payload screening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionSkipField {
    Body,
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionSkipDetector {
    AuthorizationValue,
    BearerValue,
    CookieValue,
    SecretAssignment,
    BareSecretToken,
    PemPrivateKey,
    UrlCredentials,
    AbsoluteLocalPath,
    RawHtmlBlob,
    AdapterResponseMarker,
    AdapterResponseBlob,
    ForbiddenFieldName,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct RedactionSkipReason {
    pub field: RedactionSkipField,
    pub detector: RedactionSkipDetector,
}

impl RedactionSkipReason {
    pub(super) fn classified(field: &str, detector: &str) -> Self {
        let field = if field == "chunk_text" {
            RedactionSkipField::Body
        } else {
            RedactionSkipField::Metadata
        };
        let detector = match detector {
            "authorization_value" => RedactionSkipDetector::AuthorizationValue,
            "bearer_value" => RedactionSkipDetector::BearerValue,
            "cookie_value" => RedactionSkipDetector::CookieValue,
            "secret_assignment" => RedactionSkipDetector::SecretAssignment,
            "bare_secret_token" => RedactionSkipDetector::BareSecretToken,
            "pem_private_key" => RedactionSkipDetector::PemPrivateKey,
            "url_credentials" => RedactionSkipDetector::UrlCredentials,
            "absolute_local_path" => RedactionSkipDetector::AbsoluteLocalPath,
            "raw_html_blob" => RedactionSkipDetector::RawHtmlBlob,
            "adapter_response_marker" => RedactionSkipDetector::AdapterResponseMarker,
            "adapter_response_blob" => RedactionSkipDetector::AdapterResponseBlob,
            "forbidden_field_name" => RedactionSkipDetector::ForbiddenFieldName,
            _ => RedactionSkipDetector::Other,
        };
        Self { field, detector }
    }
}

pub type RedactionSkipCounts = std::collections::BTreeMap<RedactionSkipReason, u64>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_detector_and_field_strings_are_never_retained() {
        let reason =
            RedactionSkipReason::classified("metadata.secret.value", "arbitrary-private-value");
        assert_eq!(reason.field, RedactionSkipField::Metadata);
        assert_eq!(reason.detector, RedactionSkipDetector::Other);
        assert_eq!(
            serde_json::to_string(&reason).unwrap(),
            r#"{"field":"metadata","detector":"other"}"#
        );
    }
}
