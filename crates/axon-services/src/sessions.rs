//! Neutral session path-validation utilities used by service callers.

pub use axon_adapters::sessions::{
    SessionProvider, SessionRoots, ValidatedSessionPath, has_supported_session_extension,
    validate_event_path_missing_ok, validate_session_file_path, validate_session_source_path,
};
