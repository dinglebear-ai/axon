use super::{validate_mcp_collection, validate_mcp_embed_input_with_config};
use crate::core::config::Config;
use std::path::PathBuf;
use tempfile::TempDir;

fn embed_cfg(roots: Vec<PathBuf>, max_bytes: u64) -> Config {
    let mut cfg = Config::test_default();
    cfg.mcp_embed_allowed_roots = roots;
    cfg.mcp_embed_max_local_bytes = max_bytes;
    cfg
}

#[test]
fn mcp_collection_validation_accepts_safe_names() {
    assert_eq!(
        validate_mcp_collection("docs_v2-2026.main").unwrap(),
        "docs_v2-2026.main"
    );
}

#[test]
fn mcp_collection_validation_rejects_path_and_query_chars() {
    assert!(validate_mcp_collection("../secrets").is_err());
    assert!(validate_mcp_collection("docs/v1").is_err());
    assert!(validate_mcp_collection("docs?token=abc").is_err());
    assert!(validate_mcp_collection("docs#frag").is_err());
    assert!(validate_mcp_collection(".hidden").is_err());
    assert!(validate_mcp_collection("trailing.").is_err());
    assert!(validate_mcp_collection("a..b").is_err());
    assert!(validate_mcp_collection("").is_err());
}

#[test]
fn mcp_embed_accepts_url_and_text_without_local_roots() {
    let cfg = embed_cfg(vec![], 10 * 1024 * 1024);
    assert_eq!(
        validate_mcp_embed_input_with_config(&cfg, "https://example.com/docs").unwrap(),
        "https://example.com/docs"
    );
    assert_eq!(
        validate_mcp_embed_input_with_config(&cfg, "plain text to embed").unwrap(),
        "plain text to embed"
    );
}

#[test]
fn mcp_embed_rejects_existing_local_path_without_roots() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("doc.md");
    std::fs::write(&file, "hello").unwrap();

    let cfg = embed_cfg(vec![], 10 * 1024 * 1024);
    assert!(validate_mcp_embed_input_with_config(&cfg, &file.to_string_lossy()).is_err());
}

#[test]
fn mcp_embed_allows_file_under_explicit_root() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("doc.md");
    std::fs::write(&file, "hello").unwrap();

    let cfg = embed_cfg(vec![temp.path().to_path_buf()], 10 * 1024 * 1024);
    let resolved = validate_mcp_embed_input_with_config(&cfg, &file.to_string_lossy()).unwrap();

    assert_eq!(
        resolved,
        std::fs::canonicalize(file).unwrap().to_string_lossy()
    );
}

#[test]
fn mcp_embed_rejects_dotfiles_secret_names_and_oversized_files() {
    let temp = TempDir::new().unwrap();
    let dotfile = temp.path().join(".env");
    let secret = temp.path().join("api-token.txt");
    let large = temp.path().join("large.md");
    std::fs::write(&dotfile, "OPENAI_API_KEY=secret").unwrap();
    std::fs::write(&secret, "secret").unwrap();
    std::fs::write(&large, "0123456789").unwrap();
    let cfg = embed_cfg(vec![temp.path().to_path_buf()], 4);
    assert!(validate_mcp_embed_input_with_config(&cfg, &dotfile.to_string_lossy()).is_err());
    assert!(validate_mcp_embed_input_with_config(&cfg, &secret.to_string_lossy()).is_err());
    assert!(validate_mcp_embed_input_with_config(&cfg, &large.to_string_lossy()).is_err());
}

#[test]
fn mcp_embed_rejects_nested_secret_names() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let secret = nested.join("secret-token.txt");
    std::fs::write(&secret, "secret").unwrap();

    let cfg = embed_cfg(vec![temp.path().to_path_buf()], 10 * 1024 * 1024);
    assert!(validate_mcp_embed_input_with_config(&cfg, &temp.path().to_string_lossy()).is_err());
}

#[cfg(unix)]
#[test]
fn mcp_embed_rejects_symlink_inputs() {
    use std::os::unix::fs::symlink;

    let allowed = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("outside.md");
    let link = allowed.path().join("link.md");
    std::fs::write(&target, "outside").unwrap();
    symlink(&target, &link).unwrap();

    let cfg = embed_cfg(vec![allowed.path().to_path_buf()], 10 * 1024 * 1024);
    assert!(validate_mcp_embed_input_with_config(&cfg, &link.to_string_lossy()).is_err());
}
