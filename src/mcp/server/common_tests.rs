use super::{validate_mcp_collection, validate_mcp_embed_input};
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[allow(unsafe_code)]
fn with_embed_env<T>(roots: Option<&str>, max_bytes: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _guard = env_lock().lock().unwrap();
    let old_roots = std::env::var("AXON_MCP_EMBED_ALLOWED_ROOTS").ok();
    let old_max_bytes = std::env::var("AXON_MCP_EMBED_MAX_LOCAL_BYTES").ok();
    unsafe {
        match roots {
            Some(value) => std::env::set_var("AXON_MCP_EMBED_ALLOWED_ROOTS", value),
            None => std::env::remove_var("AXON_MCP_EMBED_ALLOWED_ROOTS"),
        }
        match max_bytes {
            Some(value) => std::env::set_var("AXON_MCP_EMBED_MAX_LOCAL_BYTES", value),
            None => std::env::remove_var("AXON_MCP_EMBED_MAX_LOCAL_BYTES"),
        }
    }
    let result = f();
    unsafe {
        match old_roots {
            Some(value) => std::env::set_var("AXON_MCP_EMBED_ALLOWED_ROOTS", value),
            None => std::env::remove_var("AXON_MCP_EMBED_ALLOWED_ROOTS"),
        }
        match old_max_bytes {
            Some(value) => std::env::set_var("AXON_MCP_EMBED_MAX_LOCAL_BYTES", value),
            None => std::env::remove_var("AXON_MCP_EMBED_MAX_LOCAL_BYTES"),
        }
    }
    result
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
    with_embed_env(None, None, || {
        assert_eq!(
            validate_mcp_embed_input("https://example.com/docs").unwrap(),
            "https://example.com/docs"
        );
        assert_eq!(
            validate_mcp_embed_input("plain text to embed").unwrap(),
            "plain text to embed"
        );
    });
}

#[test]
fn mcp_embed_rejects_existing_local_path_without_roots() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("doc.md");
    std::fs::write(&file, "hello").unwrap();

    with_embed_env(None, None, || {
        assert!(validate_mcp_embed_input(&file.to_string_lossy()).is_err());
    });
}

#[test]
fn mcp_embed_allows_file_under_explicit_root() {
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("doc.md");
    std::fs::write(&file, "hello").unwrap();

    let roots = temp.path().to_string_lossy();
    let resolved = with_embed_env(Some(&roots), None, || {
        validate_mcp_embed_input(&file.to_string_lossy()).unwrap()
    });

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
    let roots = temp.path().to_string_lossy();
    with_embed_env(Some(&roots), Some("4"), || {
        assert!(validate_mcp_embed_input(&dotfile.to_string_lossy()).is_err());
        assert!(validate_mcp_embed_input(&secret.to_string_lossy()).is_err());
        assert!(validate_mcp_embed_input(&large.to_string_lossy()).is_err());
    });
}

#[test]
fn mcp_embed_rejects_nested_secret_names() {
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let secret = nested.join("secret-token.txt");
    std::fs::write(&secret, "secret").unwrap();

    let roots = temp.path().to_string_lossy();
    with_embed_env(Some(&roots), None, || {
        assert!(validate_mcp_embed_input(&temp.path().to_string_lossy()).is_err());
    });
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

    let roots = allowed.path().to_string_lossy();
    with_embed_env(Some(&roots), None, || {
        assert!(validate_mcp_embed_input(&link.to_string_lossy()).is_err());
    });
}
