use super::*;
use crate::services::context::ServiceContext;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;

type CommandFn = for<'a> fn(
    &'a Config,
    &'a ServiceContext,
) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error>>> + 'a>>;

#[allow(dead_code)]
fn _assert_command_signatures(
    _crawl: CommandFn,
    _embed: CommandFn,
    _extract: CommandFn,
    _ingest: CommandFn,
) {
}

#[test]
fn commands_accept_service_context() {
    _assert_command_signatures(run_crawl, run_embed, run_extract, run_ingest);
}
