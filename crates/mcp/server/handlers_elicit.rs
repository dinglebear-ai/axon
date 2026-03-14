//! MCP elicitation demo handler.
//!
//! Demonstrates the server→client elicitation round-trip using rmcp's typed
//! `Peer::elicit::<T>()` API. When Claude Code calls `action: "elicit_demo"`,
//! this handler suspends tool execution, presents a two-field form to the user,
//! and returns the submitted values (or a status if the user declines/cancels).

use rmcp::{Peer, RoleServer, schemars, service::ElicitationError};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::crates::mcp::schema::{AxonToolResponse, ElicitDemoRequest};
use crate::crates::mcp::server::common::internal_error;
use rmcp::ErrorData;

/// The form fields that Claude Code will present to the user.
///
/// Field names become labels in the elicitation dialog. Doc-comments become
/// the description shown beneath each field.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct ElicitDemoForm {
    /// Your name
    name: String,
    /// Favorite color
    color: String,
}

// Mark as safe for typed elicitation — confirms the schema generates an object
// type (not a primitive), which is required by the MCP elicitation spec.
rmcp::elicit_safe!(ElicitDemoForm);

pub(crate) async fn handle_elicit_demo(
    peer: &Peer<RoleServer>,
    req: ElicitDemoRequest,
) -> Result<AxonToolResponse, ErrorData> {
    let message = req
        .message
        .unwrap_or_else(|| "Please fill in the form to continue.".to_string());

    match peer.elicit::<ElicitDemoForm>(&message).await {
        Ok(Some(form)) => Ok(AxonToolResponse::ok(
            "elicit_demo",
            "",
            json!({
                "action": "accept",
                "name": form.name,
                "color": form.color,
                "message": format!("Hi {}! Your favorite color is {}.", form.name, form.color)
            }),
        )),

        Ok(None) => Ok(AxonToolResponse::ok(
            "elicit_demo",
            "",
            json!({
                "action": "accept_empty",
                "message": "User accepted but provided no content."
            }),
        )),

        Err(ElicitationError::UserDeclined) => Ok(AxonToolResponse::ok(
            "elicit_demo",
            "",
            json!({
                "action": "decline",
                "message": "User explicitly declined to fill in the form."
            }),
        )),

        Err(ElicitationError::UserCancelled) => Ok(AxonToolResponse::ok(
            "elicit_demo",
            "",
            json!({
                "action": "cancel",
                "message": "User dismissed the form without responding."
            }),
        )),

        Err(ElicitationError::CapabilityNotSupported) => Ok(AxonToolResponse::ok(
            "elicit_demo",
            "",
            json!({
                "action": "capability_not_supported",
                "message": "Client does not support elicitation. Claude Code 2.1.76+ required."
            }),
        )),

        Err(e) => {
            tracing::warn!(error = %e, "elicitation failed");
            Err(internal_error(format!("elicitation error: {e}")))
        }
    }
}
