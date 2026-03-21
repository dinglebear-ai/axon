use serde_json::Value;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

/// Structured service error with optional diagnostics payload.
///
/// The `message` is safe for user-facing surfaces. `diagnostics` is optional
/// and only attached by call paths that explicitly opt in (for example
/// `--diagnostics`-enabled query/ask flows).
#[derive(Debug, Clone)]
pub struct ServiceError {
    message: String,
    diagnostics: Option<Value>,
}

impl ServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            diagnostics: None,
        }
    }

    pub fn with_diagnostics(message: impl Into<String>, diagnostics: Value) -> Self {
        Self {
            message: message.into(),
            diagnostics: Some(diagnostics),
        }
    }

    pub fn diagnostics(&self) -> Option<&Value> {
        self.diagnostics.as_ref()
    }
}

impl Display for ServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for ServiceError {}

/// Walk an error/source chain and return the first structured diagnostics payload.
pub fn diagnostics_from_error<'a>(err: &'a (dyn StdError + 'static)) -> Option<&'a Value> {
    let mut cursor = Some(err);
    while let Some(current) = cursor {
        if let Some(service_error) = current.downcast_ref::<ServiceError>() {
            return service_error.diagnostics();
        }
        cursor = current.source();
    }
    None
}
