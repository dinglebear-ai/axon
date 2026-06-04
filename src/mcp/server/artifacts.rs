#[path = "artifacts/path.rs"]
mod path;
#[path = "artifacts/respond.rs"]
mod respond;
#[path = "artifacts/shape.rs"]
mod shape;

pub(super) use path::{
    artifact_handle_for_path, artifact_root, client_context_name, ensure_artifact_root,
    resolve_artifact_output_path,
};
pub(super) use respond::InlineHint;
pub(super) use respond::respond_with_mode;
