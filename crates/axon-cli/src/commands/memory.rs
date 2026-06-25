use axon_core::config::Config;
use axon_mcp::schema::{MemoryNodeType, MemoryRequest, MemorySubaction};
use axon_services::context::ServiceContext;
use axon_services::memory as memory_svc;
use std::error::Error;

pub async fn run_memory(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let req = request_from_positionals(&cfg.positional)?;
    let value = memory_svc::dispatch(service_context, req)
        .await
        .map_err(|err| format!("memory failed: {}", err.message))?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn request_from_positionals(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let subaction = args.first().map(String::as_str).unwrap_or("remember");
    match subaction {
        "remember" => parse_remember(args),
        "list" => parse_list(args),
        "search" => parse_search(args),
        "show" => {
            let id = args.get(1).ok_or("memory show requires an id")?.to_string();
            Ok(MemoryRequest {
                subaction: Some(MemorySubaction::Show),
                id: Some(id),
                ..MemoryRequest::default()
            })
        }
        "link" => parse_link(args),
        "context" => parse_context(args),
        "supersede" => {
            let source_id = args
                .get(1)
                .ok_or("memory supersede requires a replacement memory id")?
                .to_string();
            let target_id = args
                .get(2)
                .ok_or("memory supersede requires a superseded memory id")?
                .to_string();
            Ok(MemoryRequest {
                subaction: Some(MemorySubaction::Supersede),
                source_id: Some(source_id),
                target_id: Some(target_id),
                ..MemoryRequest::default()
            })
        }
        other => Err(format!("unknown memory subaction: {other}").into()),
    }
}

fn parse_list(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::List),
        ..MemoryRequest::default()
    };
    let mut i = 1;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--project" => req.project = Some(value),
            "--repo" => req.repo = Some(value),
            "--file" => req.file = Some(value),
            "--type" => req.memory_type = Some(parse_memory_type(&value)?),
            "--status" => req.status = Some(value),
            "--limit" => req.limit = Some(value.parse()?),
            other => return Err(format!("unknown memory list option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_context(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Context),
        ..MemoryRequest::default()
    };
    let mut i = 1;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--query" => req.query = Some(value),
            "--project" => req.project = Some(value),
            "--repo" => req.repo = Some(value),
            "--file" => req.file = Some(value),
            "--limit" => req.limit = Some(value.parse()?),
            "--token-budget" => req.token_budget = Some(value.parse()?),
            other => return Err(format!("unknown memory context option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_remember(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let body = args
        .get(1)
        .ok_or("memory remember requires body text")?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Remember),
        body: Some(body),
        ..MemoryRequest::default()
    };
    let mut i = 2;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--title" => req.title = Some(value),
            "--type" => req.memory_type = Some(parse_memory_type(&value)?),
            "--project" => req.project = Some(value),
            "--repo" => req.repo = Some(value),
            "--file" => req.file = Some(value),
            "--confidence" => req.confidence = Some(value.parse()?),
            other => return Err(format!("unknown memory remember option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_link(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let source_id = args
        .get(1)
        .ok_or("memory link requires a source memory id")?
        .clone();
    let target_id = args
        .get(2)
        .ok_or("memory link requires a target memory id")?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Link),
        source_id: Some(source_id),
        target_id: Some(target_id),
        ..MemoryRequest::default()
    };
    let mut i = 3;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--type" => req.edge_type = Some(parse_edge_type(&value)?),
            other => return Err(format!("unknown memory link option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_search(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let query = args
        .get(1)
        .ok_or("memory search requires query text")?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Search),
        query: Some(query),
        ..MemoryRequest::default()
    };
    let mut i = 2;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--project" => req.project = Some(value),
            "--repo" => req.repo = Some(value),
            "--file" => req.file = Some(value),
            "--limit" => req.limit = Some(value.parse()?),
            other => return Err(format!("unknown memory search option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_memory_type(value: &str) -> Result<MemoryNodeType, Box<dyn Error>> {
    match value {
        "decision" => Ok(MemoryNodeType::Decision),
        "fact" => Ok(MemoryNodeType::Fact),
        "preference" => Ok(MemoryNodeType::Preference),
        "task" => Ok(MemoryNodeType::Task),
        "bug" => Ok(MemoryNodeType::Bug),
        other => Err(format!("unknown memory type: {other}").into()),
    }
}

fn parse_edge_type(value: &str) -> Result<axon_mcp::schema::MemoryEdgeType, Box<dyn Error>> {
    match value {
        "relates_to" => Ok(axon_mcp::schema::MemoryEdgeType::RelatesTo),
        "supersedes" => Ok(axon_mcp::schema::MemoryEdgeType::Supersedes),
        other => Err(format!("unknown memory edge type: {other}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_remember_request() {
        let req = request_from_positionals(&[
            "remember".to_string(),
            "Memory content lives in Qdrant.".to_string(),
        ])
        .expect("request");

        assert!(matches!(req.subaction, Some(MemorySubaction::Remember)));
        assert_eq!(req.body.as_deref(), Some("Memory content lives in Qdrant."));
    }

    #[test]
    fn parses_link_request() {
        let req = request_from_positionals(&[
            "link".to_string(),
            "source".to_string(),
            "target".to_string(),
            "--type".to_string(),
            "supersedes".to_string(),
        ])
        .expect("request");

        assert!(matches!(req.subaction, Some(MemorySubaction::Link)));
        assert_eq!(req.source_id.as_deref(), Some("source"));
        assert_eq!(req.target_id.as_deref(), Some("target"));
        assert!(matches!(
            req.edge_type,
            Some(axon_mcp::schema::MemoryEdgeType::Supersedes)
        ));
    }

    #[test]
    fn parses_list_request() {
        let req = request_from_positionals(&[
            "list".to_string(),
            "--project".to_string(),
            "axon".to_string(),
            "--repo".to_string(),
            "jmagar/axon".to_string(),
            "--file".to_string(),
            "src/services/memory.rs".to_string(),
            "--type".to_string(),
            "decision".to_string(),
            "--status".to_string(),
            "superseded".to_string(),
            "--limit".to_string(),
            "20".to_string(),
        ])
        .expect("request");

        assert!(matches!(req.subaction, Some(MemorySubaction::List)));
        assert_eq!(req.project.as_deref(), Some("axon"));
        assert_eq!(req.repo.as_deref(), Some("jmagar/axon"));
        assert_eq!(req.file.as_deref(), Some("src/services/memory.rs"));
        assert!(matches!(req.memory_type, Some(MemoryNodeType::Decision)));
        assert_eq!(req.status.as_deref(), Some("superseded"));
        assert_eq!(req.limit, Some(20));
    }

    #[test]
    fn parses_supersede_request() {
        let req = request_from_positionals(&[
            "supersede".to_string(),
            "replacement".to_string(),
            "old".to_string(),
        ])
        .expect("request");

        assert!(matches!(req.subaction, Some(MemorySubaction::Supersede)));
        assert_eq!(req.source_id.as_deref(), Some("replacement"));
        assert_eq!(req.target_id.as_deref(), Some("old"));
    }

    #[test]
    fn parses_context_request() {
        let req = request_from_positionals(&[
            "context".to_string(),
            "--project".to_string(),
            "axon".to_string(),
            "--query".to_string(),
            "memory storage".to_string(),
            "--token-budget".to_string(),
            "2000".to_string(),
        ])
        .expect("request");

        assert!(matches!(req.subaction, Some(MemorySubaction::Context)));
        assert_eq!(req.project.as_deref(), Some("axon"));
        assert_eq!(req.query.as_deref(), Some("memory storage"));
        assert_eq!(req.token_budget, Some(2000));
    }
}
