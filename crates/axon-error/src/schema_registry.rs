//! Error taxonomy registry used by schema-contract generation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorSpec {
    pub code: &'static str,
    pub stage: &'static str,
    pub http_status: u16,
}

pub fn error_registry() -> &'static [ErrorSpec] {
    &[
        ErrorSpec {
            code: "invalid_request",
            stage: "request",
            http_status: 400,
        },
        ErrorSpec {
            code: "unauthorized",
            stage: "auth",
            http_status: 401,
        },
        ErrorSpec {
            code: "forbidden",
            stage: "auth",
            http_status: 403,
        },
        ErrorSpec {
            code: "not_found",
            stage: "lookup",
            http_status: 404,
        },
        ErrorSpec {
            code: "provider_unavailable",
            stage: "provider",
            http_status: 503,
        },
    ]
}
