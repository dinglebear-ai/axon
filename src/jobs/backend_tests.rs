use super::*;
use std::sync::Arc;

// Compile-time test: verify Arc<dyn JobBackend> is object-safe
fn _assert_object_safe(_: Arc<dyn JobBackend>) {}
