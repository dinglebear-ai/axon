use super::{DispatchOutput, push_opt, push_usize};
use crate::config::cli::{
    ArtifactSubcommand, CollectionSubcommand, GraphSubcommand, ProviderSubcommand,
    ResourceCliCommand, UploadSubcommand,
};
use crate::config::types::CommandKind;

pub(super) fn apply_resource(out: &mut DispatchOutput, command: ResourceCliCommand) {
    match command {
        ResourceCliCommand::Artifacts(args) => apply_artifacts(out, args.action),
        ResourceCliCommand::Uploads(args) => apply_uploads(out, args.action),
        ResourceCliCommand::Collections(args) => apply_collections(out, args.action),
        ResourceCliCommand::Graph(args) => apply_graph(out, args.action),
        ResourceCliCommand::Providers(args) => apply_providers(out, args.action),
        ResourceCliCommand::Capabilities => out.command = CommandKind::Capabilities,
        ResourceCliCommand::Chat(args) => {
            out.command = CommandKind::Chat;
            out.positional = args.message;
        }
    }
}

fn apply_artifacts(out: &mut DispatchOutput, action: ArtifactSubcommand) {
    out.command = CommandKind::Artifacts;
    match action {
        ArtifactSubcommand::List {
            kind,
            source_id,
            job_id,
            limit,
            cursor,
        } => {
            out.positional.push("list".into());
            push_opt(&mut out.positional, "--kind", kind);
            push_opt(&mut out.positional, "--source-id", source_id);
            push_opt(&mut out.positional, "--job-id", job_id);
            push_usize(&mut out.positional, "--limit", limit);
            push_opt(&mut out.positional, "--cursor", cursor);
        }
        ArtifactSubcommand::Get {
            artifact_id,
            include_content_url,
        } => {
            out.positional = vec!["get".into(), artifact_id];
            if include_content_url {
                out.positional.push("--include-content-url".into());
            }
        }
        ArtifactSubcommand::Content {
            artifact_id,
            download,
            range,
            output,
        } => {
            out.positional = vec!["content".into(), artifact_id];
            if download {
                out.positional.push("--download".into());
            }
            push_opt(&mut out.positional, "--range", range);
            if let Some(path) = output {
                push_opt(
                    &mut out.positional,
                    "--output",
                    Some(path.to_string_lossy().into_owned()),
                );
            }
        }
    }
}

fn apply_uploads(out: &mut DispatchOutput, action: UploadSubcommand) {
    out.command = CommandKind::Uploads;
    match action {
        UploadSubcommand::List {
            status,
            limit,
            cursor,
        } => {
            out.positional.push("list".into());
            push_opt(&mut out.positional, "--status", status);
            push_usize(&mut out.positional, "--limit", limit);
            push_opt(&mut out.positional, "--cursor", cursor);
        }
        UploadSubcommand::Get { upload_id } => {
            out.positional = vec!["get".into(), upload_id];
        }
        UploadSubcommand::Create {
            path,
            purpose,
            source_hint,
        } => {
            out.positional = vec![
                "create".into(),
                path.to_string_lossy().into_owned(),
                "--purpose".into(),
                purpose,
            ];
            push_opt(&mut out.positional, "--source-hint", source_hint);
        }
        UploadSubcommand::Complete {
            upload_id,
            sha256,
            source_options,
        } => {
            out.positional = vec!["complete".into(), upload_id];
            push_opt(&mut out.positional, "--sha256", sha256);
            for option in source_options {
                push_opt(&mut out.positional, "--source-option", Some(option));
            }
        }
        UploadSubcommand::Abort { upload_id, reason } => {
            out.positional = vec!["abort".into(), upload_id];
            push_opt(&mut out.positional, "--reason", reason);
        }
    }
}

fn apply_collections(out: &mut DispatchOutput, action: CollectionSubcommand) {
    out.command = CommandKind::Collections;
    match action {
        CollectionSubcommand::List => out.positional.push("list".into()),
        CollectionSubcommand::Get {
            collection,
            include_schema,
            include_indexes,
        } => {
            out.positional = vec!["get".into(), collection];
            if include_schema {
                out.positional.push("--include-schema".into());
            }
            if include_indexes {
                out.positional.push("--include-indexes".into());
            }
        }
    }
}

fn apply_graph(out: &mut DispatchOutput, action: GraphSubcommand) {
    out.command = CommandKind::Graph;
    match action {
        GraphSubcommand::Kinds => out.positional.push("kinds".into()),
        GraphSubcommand::Resolve {
            identifier,
            kind,
            limit,
        } => {
            out.positional = vec!["resolve".into(), identifier];
            push_opt(&mut out.positional, "--kind", kind);
            push_usize(&mut out.positional, "--limit", limit);
        }
        GraphSubcommand::Query {
            query,
            limit,
            cursor,
        } => {
            out.positional = vec!["query".into(), query];
            push_usize(&mut out.positional, "--limit", limit);
            push_opt(&mut out.positional, "--cursor", cursor);
        }
        GraphSubcommand::Node {
            node_id,
            include_edges,
            include_evidence,
        } => {
            out.positional = vec!["node".into(), node_id];
            push_bool(&mut out.positional, "--include-edges", include_edges);
            push_bool(&mut out.positional, "--include-evidence", include_evidence);
        }
        GraphSubcommand::Edge {
            edge_id,
            include_evidence,
        } => {
            out.positional = vec!["edge".into(), edge_id];
            push_bool(&mut out.positional, "--include-evidence", include_evidence);
        }
        GraphSubcommand::Source {
            source_id,
            depth,
            edge_kind,
            limit,
        } => {
            out.positional = vec!["source".into(), source_id];
            push_usize(&mut out.positional, "--depth", depth);
            push_opt(&mut out.positional, "--edge-kind", edge_kind);
            push_usize(&mut out.positional, "--limit", limit);
        }
    }
}

fn apply_providers(out: &mut DispatchOutput, action: ProviderSubcommand) {
    out.command = CommandKind::Providers;
    match action {
        ProviderSubcommand::List { kind, status } => {
            out.positional.push("list".into());
            push_opt(&mut out.positional, "--kind", kind);
            push_opt(&mut out.positional, "--status", status);
        }
        ProviderSubcommand::Get {
            provider,
            include_health,
            include_limits,
        } => {
            out.positional = vec!["get".into(), provider];
            push_bool(&mut out.positional, "--include-health", include_health);
            push_bool(&mut out.positional, "--include-limits", include_limits);
        }
    }
}

fn push_bool(out: &mut Vec<String>, flag: &str, value: bool) {
    if value {
        out.push(flag.to_string());
    }
}
