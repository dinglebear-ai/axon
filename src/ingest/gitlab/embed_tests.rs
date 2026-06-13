use super::*;
use crate::ingest::gitlab::types::{GitLabProject, GitLabTarget};

fn test_target() -> GitLabTarget {
    GitLabTarget {
        host: "gitlab.com".to_string(),
        namespace_path: "mygroup/myproject".to_string(),
        project: "myproject".to_string(),
        web_url: "https://gitlab.com/mygroup/myproject".to_string(),
        clone_url: "https://gitlab.com/mygroup/myproject.git".to_string(),
        api_base: "https://gitlab.com/api/v4".to_string(),
        encoded_project_path: "mygroup%2Fmyproject".to_string(),
    }
}

fn test_project() -> GitLabProject {
    GitLabProject {
        path_with_namespace: "mygroup/myproject".to_string(),
        name: "myproject".to_string(),
        description: None,
        default_branch: Some("main".to_string()),
        web_url: "https://gitlab.com/mygroup/myproject".to_string(),
        visibility: Some("public".to_string()),
        star_count: None,
        forks_count: None,
        open_issues_count: None,
        issues_enabled: None,
        merge_requests_enabled: None,
        wiki_enabled: None,
        last_activity_at: None,
    }
}

#[test]
fn gitlab_file_chunk_payload_sets_code_and_symbol_fields() {
    use crate::vector::ops::input::code::{ChunkSource, CodeChunk, Symbol, SymbolKind};
    let target = test_target();
    let project = test_project();
    let chunk = CodeChunk {
        text: "fn x() {}".into(),
        byte_start: 0,
        byte_end: 9,
        start_line: 10,
        end_line: 12,
        declaration_start_line: 10,
        declaration_end_line: 12,
        symbol: Some(Symbol {
            kind: SymbolKind::Function,
            name: Some("x".into()),
        }),
        source: ChunkSource::TreeSitter,
    };
    let payload = gitlab_file_chunk_payload(
        &target,
        &project,
        "src/lib.rs",
        "main",
        &chunk,
        "tree_sitter",
        "ok",
    );
    assert_eq!(payload["git_content_kind"], "file");
    assert_eq!(payload["code_file_path"], "src/lib.rs");
    assert_eq!(payload["code_line_start"], 10);
    assert_eq!(payload["code_line_end"], 12);
    assert_eq!(payload["code_chunking_method"], "tree_sitter");
    assert_eq!(payload["symbol_name"], "x");
    assert_eq!(payload["symbol_kind"], "function");
    assert_eq!(payload["symbol_extraction_status"], "ok");
}
