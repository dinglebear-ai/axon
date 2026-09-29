//! Shared call pipeline for aggregate and leaf routes. Authorization and
//! caller-local execution context are identical after canonical resolution.
use super::*;
use lab_auth::AuthContext;

pub(super) async fn call(
    server: &AxonMcpServer,
    mut request: CallToolRequestParams,
    context: RequestContext<RoleServer>,
) -> Result<CallToolResponse, ErrorData> {
    rehydrate_request_meta(&mut request, &context.meta);
    let canonical = projection::normalize_projected_tool_call(server, &mut request)?;
    let (action, subaction) = identity(&request, canonical);
    tracing::info!(presented_tool = %request.name, canonical_action = %action,
        canonical_subaction = %subaction, "resolved MCP operation");
    if is_task_augmented(&request) {
        if !canonical {
            return Err(invalid_params(format!(
                "tool {} does not support task execution",
                request.name
            )));
        }
        // Refuse before creating a durable job. A task-capable client must
        // still opt in; capability support alone never changes call semantics.
        if !context
            .client_capabilities()
            .is_some_and(|caps| caps.supports_tasks())
        {
            return Err(invalid_params(
                "task-augmented tools/call requires the client to declare the io.modelcontextprotocol/tasks extension capability",
            ));
        }
        return tasks::enqueue_task(server, request, context)
            .await
            .map(Into::into);
    }
    let auth = server_authz::require_auth_context(&server.auth_policy, &context)?;
    server_authz::enforce_call_tool_scope(auth, request.name.as_ref(), &action, &subaction)?;
    let execution = ExecutionContext::new(&action, auth);
    dispatch_scoped(server, request, context, execution).await
}

fn identity(request: &CallToolRequestParams, canonical: bool) -> (String, String) {
    if !canonical {
        return (String::new(), String::new());
    }
    let field = |key| {
        request
            .arguments
            .as_ref()
            .and_then(|args| args.get(key))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    (field("action"), field("subaction"))
}

struct ExecutionContext {
    prune: axon_services::prune::PruneAuthz,
    reset: axon_services::reset::ResetAuthz,
    memory: axon_services::memory::MemoryAuthz,
    snapshot: Option<axon_api::source::AuthSnapshot>,
    codex: common::CodexCaller,
}

impl ExecutionContext {
    fn new(action: &str, auth: Option<&AuthContext>) -> Self {
        // No AuthContext exists only under the explicit loopback trust policy.
        // Mounted callers inherit admin solely from their resolved scopes.
        let admin = auth.is_none_or(|caller| {
            axon_authz::scope_satisfies(&caller.scopes, axon_authz::AXON_ADMIN_SCOPE)
        });
        Self {
            prune: axon_services::prune::PruneAuthz {
                is_admin: action == "prune" && admin,
            },
            reset: axon_services::reset::ResetAuthz {
                is_admin: action == "reset" && admin,
            },
            memory: axon_services::memory::MemoryAuthz {
                is_admin: action == "memory" && admin,
            },
            snapshot: caller_snapshot(auth),
            codex: auth.map_or_else(
                || common::CodexCaller {
                    actor: "trusted-loopback".to_owned(),
                    scopes: "local-trusted".to_owned(),
                },
                |caller| common::CodexCaller {
                    actor: caller.sub.clone(),
                    scopes: caller.scopes.join(" "),
                },
            ),
        }
    }
}

fn caller_snapshot(auth: Option<&AuthContext>) -> Option<axon_api::source::AuthSnapshot> {
    auth.map(|caller| {
        let mut context = axon_api::source::CallerContext {
            caller_id: Some(caller.sub.clone()),
            transport: axon_api::source::TransportKind::Mcp,
            trusted_local: false,
            scopes: caller.scopes.clone(),
            visibility_ceiling: axon_api::source::Visibility::Public,
            auth_mode: if caller.sub == "static-bearer" {
                axon_api::source::AuthMode::StaticToken
            } else {
                axon_api::source::AuthMode::Oauth
            },
            token_id: None,
            display_name: None,
        };
        let ceiling = axon_authz::VisibilityPolicy::new().ceiling_for(&context);
        context.visibility_ceiling = ceiling;
        axon_api::source::AuthSnapshot::from_caller(&context, ceiling, "runtime")
    })
}

async fn dispatch_scoped(
    server: &AxonMcpServer,
    request: CallToolRequestParams,
    context: RequestContext<RoleServer>,
    execution: ExecutionContext,
) -> Result<CallToolResponse, ErrorData> {
    let call = rmcp::handler::server::tool::ToolCallContext::new(server, request, context);
    common::CURRENT_PRUNE_AUTHZ
        .scope(
            execution.prune,
            common::CURRENT_RESET_AUTHZ.scope(
                execution.reset,
                common::CURRENT_MEMORY_AUTHZ.scope(
                    execution.memory,
                    common::CURRENT_CALLER_AUTH_SNAPSHOT.scope(
                        execution.snapshot,
                        common::CURRENT_CODEX_CALLER
                            .scope(execution.codex, server.projected_router.call(call)),
                    ),
                ),
            ),
        )
        .await
}

#[cfg(test)]
#[path = "execution_context_tests.rs"]
mod tests;
