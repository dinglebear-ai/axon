use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::paths::{axon_data_base_dir, ensure_private_dir};
use crate::core::ui::{accent, muted, primary};
use crate::services::query as query_svc;
use crate::services::types::{AskExplainCandidate, AskExplainFilterDecisionKind};
use chrono::Utc;
use serde_json::json;
use std::error::Error;
use std::io::{self, Write};
use std::path::PathBuf;
use uuid::Uuid;

pub async fn run_train(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let query = resolve_input_text(cfg).ok_or("train requires a query")?;
    let top_k = cfg.search_limit.clamp(2, 50);

    let mut explain_cfg = cfg.clone();
    explain_cfg.ask_explain = true;
    explain_cfg.ask_diagnostics = true;

    let result = query_svc::ask(&explain_cfg, &query, None).await?;
    let explain = result
        .explain
        .as_ref()
        .ok_or("train expected ask explain trace")?;
    let candidates = kept_candidates(&explain.candidates, top_k);
    if candidates.is_empty() {
        return Err("train found no kept candidates to vote on".into());
    }

    if cfg.json_output && cfg.train_best_rank.is_none() {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "query": query,
                "collection": cfg.collection,
                "candidates": candidate_json(&candidates),
                "message": "rerun with --best <rank> to record a vote in --json mode"
            }))?
        );
        return Ok(());
    }

    if !cfg.json_output {
        print_training_choices(&query, &candidates);
    }

    let selected_rank = match cfg.train_best_rank {
        Some(rank) => Some(validate_rank(rank, candidates.len())?),
        None => prompt_for_vote(candidates.len())?,
    };
    let Some(selected_rank) = selected_rank else {
        if !cfg.json_output {
            println!("{}", muted("Skipped; no preference recorded."));
        }
        return Ok(());
    };

    let event = preference_event(cfg, &query, selected_rank, &candidates);
    let path = append_preference_event(&event)?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&event)?);
    } else {
        let selected = candidates[selected_rank - 1];
        println!(
            "{} {}",
            primary("Recorded preference:"),
            accent(&selected.url)
        );
        println!("  {} {}", muted("Path:"), path.display());
    }

    Ok(())
}

fn kept_candidates(candidates: &[AskExplainCandidate], limit: usize) -> Vec<&AskExplainCandidate> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate
                .filter_decisions
                .iter()
                .any(|decision| decision.kind == AskExplainFilterDecisionKind::Kept)
        })
        .take(limit)
        .collect()
}

fn print_training_choices(query: &str, candidates: &[&AskExplainCandidate]) {
    println!("{}", primary("Training Vote"));
    println!("  {} {}", primary("Query:"), query);
    println!("  {} {}", muted("Candidates:"), candidates.len());
    println!();
    for (idx, candidate) in candidates.iter().enumerate() {
        println!(
            "{}. rerank={:.3} retrieval={:.3} {}",
            idx + 1,
            candidate.rerank_score,
            candidate.retrieval_score,
            accent(&candidate.url)
        );
        println!("   {}", candidate.snippet);
    }
    println!();
    println!("{}", muted("Enter the best rank, or 's' to skip."));
}

fn prompt_for_vote(max_rank: usize) -> Result<Option<usize>, Box<dyn Error>> {
    print!("best> ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    parse_vote(&input, max_rank)
}

fn parse_vote(input: &str, max_rank: usize) -> Result<Option<usize>, Box<dyn Error>> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("s") || trimmed.eq_ignore_ascii_case("skip") {
        return Ok(None);
    }
    let rank = trimmed
        .parse::<usize>()
        .map_err(|_| format!("vote must be a rank from 1 to {max_rank}, or 's' to skip"))?;
    Ok(Some(validate_rank(rank, max_rank)?))
}

fn validate_rank(rank: usize, max_rank: usize) -> Result<usize, Box<dyn Error>> {
    if (1..=max_rank).contains(&rank) {
        Ok(rank)
    } else {
        Err(format!("vote rank {rank} is outside 1..={max_rank}").into())
    }
}

fn preference_event(
    cfg: &Config,
    query: &str,
    selected_rank: usize,
    candidates: &[&AskExplainCandidate],
) -> serde_json::Value {
    let selected = candidates[selected_rank - 1];
    json!({
        "schema": "axon.training.preference.v1",
        "event_id": Uuid::new_v4(),
        "created_at": Utc::now(),
        "query": query,
        "collection": cfg.collection,
        "selected_rank": selected_rank,
        "selected_url": selected.url,
        "selected_chunk_index": selected.chunk_index,
        "notes": cfg.train_notes.clone(),
        "candidates": candidate_json(candidates),
    })
}

fn candidate_json(candidates: &[&AskExplainCandidate]) -> Vec<serde_json::Value> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            json!({
                "rank": idx + 1,
                "id": candidate.id,
                "url": candidate.url,
                "chunk_index": candidate.chunk_index,
                "retrieval_score": candidate.retrieval_score,
                "rerank_score": candidate.rerank_score,
                "score_kind": candidate.score_kind,
                "score_components": candidate.score_components,
                "snippet": candidate.snippet,
            })
        })
        .collect()
}

fn append_preference_event(event: &serde_json::Value) -> Result<PathBuf, Box<dyn Error>> {
    let training_dir = axon_data_base_dir().join("training");
    ensure_private_dir(&training_dir)?;
    let path = training_dir.join("preferences.jsonl");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    writeln!(file, "{}", serde_json::to_string(event)?)?;
    Ok(path)
}

#[cfg(test)]
#[path = "train_tests.rs"]
mod tests;
