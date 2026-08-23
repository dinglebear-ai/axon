//! Durable provider scheduler composition for the target-local source runtime.

use std::path::{Path, PathBuf};

use axon_adapters::providers::chrome_render::CHROME_RENDER_PROVIDER_ID;
use axon_adapters::providers::http_fetch::HTTP_FETCH_PROVIDER_ID;
use axon_api::source::{ProviderId, ProviderKind};
use axon_core::config::Config;
use axon_jobs::scheduler::{
    ProviderCapacityDomain, ProviderScheduler, SchedulerConfig, SqliteWriteGate,
};
use sqlx::SqlitePool;

/// Durable vector-scheduler capacity. `[providers.vector]` has no equivalent
/// config-driven capacity/reserve knobs yet, so these remain explicit defaults
/// for the shared SQLite vector lane.
const VECTOR_RESERVATION_CAPACITY: u32 = 2;
const VECTOR_RESERVATION_INTERACTIVE_RESERVE: u32 = 1;

pub(super) struct RuntimeSchedulers {
    pub(super) embedding: ProviderScheduler,
    pub(super) vector: ProviderScheduler,
    pub(super) fetch: ProviderScheduler,
    pub(super) render: ProviderScheduler,
    pub(super) parse: ProviderScheduler,
    pub(super) graph: ProviderScheduler,
    pub(super) artifact: ProviderScheduler,
}

pub(super) async fn build_runtime_schedulers(
    cfg: &Config,
    pool: &SqlitePool,
    embedding_provider_id: &ProviderId,
    vector_provider_id: &ProviderId,
    write_gate: SqliteWriteGate,
) -> Result<RuntimeSchedulers, Box<dyn std::error::Error + Send + Sync>> {
    let authority_id = scheduler_authority_id(&cfg.sqlite_path);
    let embedding = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: embedding_provider_id.0.clone(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(
            cfg.embed_tei_max_concurrent as u32,
            cfg.embed_tei_interactive_reserved_requests as u32,
        ),
        write_gate.clone(),
    )?;
    let vector = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Vector,
            instance_id: vector_provider_id.0.clone(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(
            VECTOR_RESERVATION_CAPACITY,
            VECTOR_RESERVATION_INTERACTIVE_RESERVE,
        ),
        write_gate.clone(),
    )?;
    let fetch = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Fetch,
            instance_id: HTTP_FETCH_PROVIDER_ID.to_string(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(cfg.fetch_provider_concurrency as u32, 1),
        write_gate.clone(),
    )?;
    let render = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Render,
            instance_id: CHROME_RENDER_PROVIDER_ID.to_string(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(cfg.render_provider_concurrency as u32, 1),
        write_gate.clone(),
    )?;
    let parse = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Parser,
            instance_id: "source-parse".to_string(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(cfg.embed_prep_concurrency.max(1) as u32, 1),
        write_gate.clone(),
    )?;
    let graph = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Graph,
            instance_id: "sqlite-graph".to_string(),
            authority_id: authority_id.clone(),
        },
        scheduler_config(source_db_stage_capacity(pool) as u32, 1),
        write_gate.clone(),
    )?;
    let artifact = ProviderScheduler::new_with_write_gate(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Artifact,
            instance_id: "file-artifact-store".to_string(),
            authority_id,
        },
        scheduler_config(cfg.batch_concurrency.max(1) as u32, 1),
        write_gate,
    )?;

    for scheduler in [
        &embedding, &vector, &fetch, &render, &parse, &graph, &artifact,
    ] {
        scheduler.reconcile().await?;
    }

    Ok(RuntimeSchedulers {
        embedding,
        vector,
        fetch,
        render,
        parse,
        graph,
        artifact,
    })
}

pub(super) fn source_db_stage_capacity(pool: &SqlitePool) -> usize {
    // Preserve one pool connection for job heartbeats, provider scheduler
    // control, and other liveness work. A single-connection test pool cannot
    // reserve a spare lane, so it retains one data-plane permit.
    usize::try_from(
        pool.options()
            .get_max_connections()
            .saturating_sub(1)
            .max(1),
    )
    .unwrap_or(1)
}

pub(super) fn scheduler_authority_id(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let stable = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    format!("sqlite:{}", stable.display())
}

fn scheduler_config(capacity: u32, interactive_reserve: u32) -> SchedulerConfig {
    let capacity = capacity.max(1);
    SchedulerConfig {
        capacity,
        interactive_reserve: interactive_reserve.min(capacity),
        max_entries: capacity.saturating_mul(256).max(256),
        max_units: capacity.saturating_mul(256).max(256),
    }
}
