//! Shared error taxonomy for the unified Axon pipeline.
//!
//! `axon-error` is the lowest shared error boundary: it owns the typed error
//! taxonomy ([`ApiError`], [`ErrorCode`], [`ErrorStage`], [`ErrorSeverity`],
//! [`ErrorVisibility`]) plus retry / cooling / degradation classifications and
//! redaction-aware context. It depends on no other Axon crate, so every crate
//! can report failures through it without forming a cycle.
//!
//! Contract: `docs/pipeline-unification/crates/axon-error/README.md`; behavior
//! spec: `docs/pipeline-unification/runtime/error-handling.md`.

pub mod api_error;
pub mod code;
pub mod context;
pub mod conversion;
pub mod cooling;
pub mod degradation;
pub mod retry;
pub mod schema_registry;
pub mod severity;
pub mod stage;
pub mod testing;

pub use api_error::ApiError;
pub use code::ErrorCode;
pub use context::{ErrorContext, ErrorContextEntry, ErrorVisibility};
pub use conversion::{IntoApiError, api_error_from_parts, project};
pub use cooling::ProviderCooling;
pub use degradation::DegradationPolicy;
pub use retry::{RetryPolicy, RetryScope};
pub use severity::ErrorSeverity;
pub use stage::ErrorStage;

pub const CRATE_NAME: &str = "axon-error";
