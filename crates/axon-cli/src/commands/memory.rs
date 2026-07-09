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
    crate::json::print_json_gated(&value)?;
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
        "reinforce" => parse_reinforce(args),
        "contradict" => parse_contradict(args),
        "pin" => parse_pin(args),
        "archive" => parse_id_and_reason(args, MemorySubaction::Archive, "archive"),
        "forget" => parse_id_and_reason(args, MemorySubaction::Forget, "forget"),
        "review" => parse_review(args),
        "compact" => parse_compact(args),
        other => Err(format!("unknown memory subaction: {other}").into()),
    }
}

fn parse_reinforce(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let id = args
        .get(1)
        .ok_or("memory reinforce requires an id")?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Reinforce),
        id: Some(id),
        ..MemoryRequest::default()
    };
    let mut i = 2;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--amount" => req.amount = Some(value.parse()?),
            "--reason" => req.reason = Some(value),
            other => return Err(format!("unknown memory reinforce option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_contradict(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let source_id = args
        .get(1)
        .ok_or("memory contradict requires a memory id")?
        .clone();
    let target_id = args
        .get(2)
        .ok_or("memory contradict requires a conflicting memory id")?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Contradict),
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
            "--reason" => req.reason = Some(value),
            other => return Err(format!("unknown memory contradict option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_pin(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let id = args.get(1).ok_or("memory pin requires an id")?.clone();
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Pin),
        id: Some(id),
        pinned: Some(true),
        ..MemoryRequest::default()
    };
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--unpin" => {
                req.pinned = Some(false);
                i += 1;
            }
            "--reason" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| format!("{} requires a value", args[i]))?
                    .clone();
                req.reason = Some(value);
                i += 2;
            }
            other => return Err(format!("unknown memory pin option: {other}").into()),
        }
    }
    Ok(req)
}

fn parse_id_and_reason(
    args: &[String],
    subaction: MemorySubaction,
    verb: &str,
) -> Result<MemoryRequest, Box<dyn Error>> {
    let id = args
        .get(1)
        .ok_or_else(|| format!("memory {verb} requires an id"))?
        .clone();
    let mut req = MemoryRequest {
        subaction: Some(subaction),
        id: Some(id),
        ..MemoryRequest::default()
    };
    let mut i = 2;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--reason" => req.reason = Some(value),
            other => return Err(format!("unknown memory {verb} option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_review(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Review),
        ..MemoryRequest::default()
    };
    let mut i = 1;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| format!("{} requires a value", args[i]))?
            .clone();
        match args[i].as_str() {
            "--type" => req.memory_type = Some(parse_memory_type(&value)?),
            "--limit" => req.limit = Some(value.parse()?),
            "--reason" => req.reason = Some(value),
            other => return Err(format!("unknown memory review option: {other}").into()),
        }
        i += 2;
    }
    Ok(req)
}

fn parse_compact(args: &[String]) -> Result<MemoryRequest, Box<dyn Error>> {
    let mut memory_ids = Vec::new();
    let mut i = 1;
    while i < args.len() && !args[i].starts_with("--") {
        memory_ids.push(args[i].clone());
        i += 1;
    }
    if memory_ids.len() < 2 {
        return Err("memory compact requires at least 2 source memory ids".into());
    }
    let mut req = MemoryRequest {
        subaction: Some(MemorySubaction::Compact),
        memory_ids: Some(memory_ids),
        ..MemoryRequest::default()
    };
    while i < args.len() {
        match args[i].as_str() {
            "--archive-sources" => {
                req.archive_sources = Some(true);
                i += 1;
            }
            other => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| format!("{other} requires a value"))?
                    .clone();
                match other {
                    "--strategy" => req.strategy = Some(value),
                    "--title" => req.title = Some(value),
                    "--type" => req.memory_type = Some(parse_memory_type(&value)?),
                    "--project" => req.project = Some(value),
                    "--repo" => req.repo = Some(value),
                    "--file" => req.file = Some(value),
                    other => return Err(format!("unknown memory compact option: {other}").into()),
                }
                i += 2;
            }
        }
    }
    Ok(req)
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
#[path = "memory_tests.rs"]
mod tests;
