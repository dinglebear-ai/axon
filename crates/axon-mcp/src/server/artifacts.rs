#[path = "artifacts/path.rs"]
mod path;
#[path = "artifacts/respond.rs"]
mod respond;
#[path = "artifacts/shape.rs"]
mod shape;

pub(super) use path::{artifact_root, client_context_name};
pub(super) use respond::InlineHint;
pub(super) use respond::respond_with_mode;
