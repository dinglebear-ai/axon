//! Dedupe near-identical chunks within a Qdrant collection.

use crate::core::config::Config;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::DedupeResult;
use crate::vector::ops::qdrant::dedupe_payload;
use std::error::Error;
use tokio::sync::mpsc;

#[must_use = "dedupe returns a Result that should be handled"]
pub async fn dedupe(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DedupeResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "starting dedupe".to_string(),
        },
    )
    .await;
    // Run dedupe and immediately convert the Result to a plain String outcome so
    // that `Box<dyn Error>` (!Send) is fully dropped before the next `.await`.
    enum DedupeOutcome {
        Success {
            duplicate_groups: usize,
            deleted: usize,
        },
        Failure(String),
    }
    let outcome = match dedupe_payload(cfg).await {
        Ok(v) => DedupeOutcome::Success {
            duplicate_groups: v
                .get("duplicate_groups")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
            deleted: v
                .get("deleted")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize,
        },
        Err(e) => DedupeOutcome::Failure(format!("dedupe failed: {e}")),
    };
    match outcome {
        DedupeOutcome::Success {
            duplicate_groups,
            deleted,
        } => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "completed dedupe: {duplicate_groups} groups, {deleted} deleted"
                    ),
                },
            )
            .await;
            Ok(DedupeResult {
                completed: true,
                duplicate_groups,
                deleted,
            })
        }
        DedupeOutcome::Failure(msg) => {
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            Err(msg.into())
        }
    }
}
