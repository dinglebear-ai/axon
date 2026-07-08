use super::AxonMcpServer;
use super::common::{internal_error, invalid_params};
use crate::schema::{AxonToolResponse, MemoryRequest, MemorySubaction};
use axon_services::memory as memory_svc;
use axon_services::types::ClientActionError;
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(super) async fn handle_memory(
        &self,
        req: MemoryRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let subaction = memory_subaction_label(req.subaction.unwrap_or(MemorySubaction::Remember));
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| internal_error(format!("initialize memory context: {e}")))?;
        let data = memory_svc::dispatch(&ctx, req)
            .await
            .map_err(map_memory_error)?;
        Ok(AxonToolResponse::ok("memory", subaction, data))
    }
}

fn memory_subaction_label(subaction: MemorySubaction) -> &'static str {
    match subaction {
        MemorySubaction::Remember => "remember",
        MemorySubaction::List => "list",
        MemorySubaction::Search => "search",
        MemorySubaction::Show => "show",
        MemorySubaction::Link => "link",
        MemorySubaction::Supersede => "supersede",
        MemorySubaction::Context => "context",
        MemorySubaction::Reinforce => "reinforce",
        MemorySubaction::Contradict => "contradict",
        MemorySubaction::Pin => "pin",
        MemorySubaction::Archive => "archive",
        MemorySubaction::Forget => "forget",
        MemorySubaction::Review => "review",
        MemorySubaction::Compact => "compact",
    }
}

fn map_memory_error(err: ClientActionError) -> ErrorData {
    let message = match err.hint {
        Some(hint) => format!("{}: {hint}", err.message),
        None => err.message,
    };
    if err.retryable || err.kind == "internal" {
        internal_error(message)
    } else {
        invalid_params(message)
    }
}
