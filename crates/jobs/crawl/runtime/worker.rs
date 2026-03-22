use crate::crates::core::config::Config;
use std::error::Error;

mod amqp_consumer;
mod cancel_poll;
mod embed;
mod job_context;
mod loops;
mod postprocess;
mod process;
mod result_builder;

pub(super) use amqp_consumer::reclaim_stale_running_jobs;

pub(super) async fn run_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    loops::run_worker(cfg).await
}
