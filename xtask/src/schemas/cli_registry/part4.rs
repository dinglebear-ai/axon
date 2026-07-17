//! Resource command groups (flattened `ResourceCliCommand` variants):
//! artifacts, uploads, collections, graph, providers, capabilities, chat.

use super::{CliRegistryCommand, c};

pub(super) fn commands() -> Vec<CliRegistryCommand> {
    let mut commands = commands_artifacts_uploads();
    commands.extend(commands_collections_graph());
    commands.extend(commands_providers_capabilities_chat());
    commands
}

fn commands_artifacts_uploads() -> Vec<CliRegistryCommand> {
    vec![
        // artifacts
        c(
            &["artifacts", "list"],
            "List artifacts by kind, source, or job",
            Some("ArtifactListRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["artifacts", "get"],
            "Show one artifact record by opaque artifact id",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["artifacts", "content"],
            "Read or download artifact content by opaque artifact id",
            None,
            false,
            false,
            "read",
        ),
        // uploads
        c(
            &["uploads", "list"],
            "List staged uploads",
            Some("UploadListRequest"),
            false,
            false,
            "read",
        ),
        c(
            &["uploads", "get"],
            "Show one staged upload",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["uploads", "create"],
            "Stage a local file as a durable upload",
            Some("UploadCreateRequest"),
            true,
            false,
            "write",
        ),
        c(
            &["uploads", "complete"],
            "Finalize a staged upload into a durable source reference",
            Some("UploadCompleteRequest"),
            true,
            false,
            "write",
        ),
        c(
            &["uploads", "abort"],
            "Abort and discard a staged upload",
            Some("UploadAbortRequest"),
            true,
            false,
            "write",
        ),
    ]
}

fn commands_collections_graph() -> Vec<CliRegistryCommand> {
    vec![
        // collections
        c(
            &["collections", "list"],
            "List configured vector collections",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["collections", "get"],
            "Show one vector collection with optional schema and indexes",
            None,
            false,
            false,
            "read",
        ),
        // graph
        c(
            &["graph", "kinds"],
            "List SourceGraph node and edge kinds",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["graph", "resolve"],
            "Resolve an identifier to SourceGraph nodes",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["graph", "query"],
            "Query SourceGraph nodes",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["graph", "node"],
            "Show one SourceGraph node with optional edges and evidence",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["graph", "edge"],
            "Show one SourceGraph edge with optional evidence",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["graph", "source"],
            "Walk the SourceGraph neighborhood of a source",
            None,
            false,
            false,
            "read",
        ),
    ]
}

fn commands_providers_capabilities_chat() -> Vec<CliRegistryCommand> {
    vec![
        // providers
        c(
            &["providers", "list"],
            "List providers by kind or status",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["providers", "get"],
            "Show one provider with optional health and limits",
            None,
            false,
            false,
            "read",
        ),
        // capabilities / chat
        c(
            &["capabilities"],
            "Print machine-readable runtime capabilities",
            None,
            false,
            false,
            "read",
        ),
        c(
            &["chat"],
            "Send a direct prompt to the configured LLM",
            None,
            false,
            false,
            "read",
        ),
    ]
}
