mod artifact;
mod artifact_index;
mod cross_check;
mod families;
pub mod registry;
mod removed;
mod report;
mod schema_json;
mod source_input;
mod validate;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use families::{all_families, generator_for};
use report::FamilyReport;
use serde::Serialize;

#[derive(Debug, Args)]
pub struct SchemasArgs {
    #[command(subcommand)]
    command: SchemaCommand,
}

#[derive(Debug, Subcommand)]
enum SchemaCommand {
    /// Generate every schema family.
    Generate(SchemaGenerateArgs),
    /// Generate/check only the API DTO schema family.
    Api(SchemaGenerateArgs),
    /// Generate/check only the CLI schema family.
    Cli(SchemaGenerateArgs),
    /// Generate/check only the OpenAPI schema family.
    Openapi(SchemaGenerateArgs),
    /// Generate/check only the MCP schema family.
    Mcp(SchemaGenerateArgs),
    /// Generate/check only the config schema family.
    Config(SchemaGenerateArgs),
    /// Generate/check only the event schema family.
    Events(SchemaGenerateArgs),
    /// Generate/check only the error schema family.
    Errors(SchemaGenerateArgs),
    /// Generate/check only the database schema family.
    Database(SchemaGenerateArgs),
    /// Generate/check only the graph schema family.
    Graph(SchemaGenerateArgs),
    /// Generate/check only the vector-payload schema family.
    VectorPayload(SchemaGenerateArgs),
    /// Generate/check only the provider schema family.
    Providers(SchemaGenerateArgs),
    /// Generate/check only the source adapter capability schema family.
    Adapters(SchemaGenerateArgs),
}

#[derive(Debug, Args, Clone, Default)]
pub struct SchemaGenerateArgs {
    /// Fail if generated output differs from tracked files.
    #[arg(long)]
    pub check: bool,
    /// Print generated artifacts to stdout instead of writing.
    #[arg(long)]
    pub print: bool,
    /// Emit machine-readable check report.
    #[arg(long)]
    pub json: bool,
    /// Restrict aggregate generate/check to one family.
    #[arg(long, value_enum)]
    pub family: Option<SchemaFamily>,
    /// Regenerate fixture snapshots. Forbidden in CI.
    #[arg(long)]
    pub update_fixtures: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaExitCode {
    Success = 0,
    ValidationOrDriftFailure = 1,
    BadInvocation = 2,
    SourceInputOrArtifactManifestFailure = 3,
    InternalGeneratorError = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SchemaFamily {
    Api,
    Cli,
    Openapi,
    Mcp,
    Config,
    Events,
    Errors,
    Database,
    Graph,
    VectorPayload,
    Providers,
    Adapters,
}

impl Serialize for SchemaFamily {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl SchemaFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Cli => "cli",
            Self::Openapi => "openapi",
            Self::Mcp => "mcp",
            Self::Config => "config",
            Self::Events => "events",
            Self::Errors => "errors",
            Self::Database => "database",
            Self::Graph => "graph",
            Self::VectorPayload => "vector-payload",
            Self::Providers => "providers",
            Self::Adapters => "adapters",
        }
    }
}

pub fn run(root: &Path, args: SchemasArgs) -> Result<()> {
    match args.command {
        SchemaCommand::Generate(args) => run_families(root, selected_families(args.family), &args),
        SchemaCommand::Api(args) => run_single_family(root, SchemaFamily::Api, &args),
        SchemaCommand::Cli(args) => run_single_family(root, SchemaFamily::Cli, &args),
        SchemaCommand::Openapi(args) => run_single_family(root, SchemaFamily::Openapi, &args),
        SchemaCommand::Mcp(args) => run_single_family(root, SchemaFamily::Mcp, &args),
        SchemaCommand::Config(args) => run_single_family(root, SchemaFamily::Config, &args),
        SchemaCommand::Events(args) => run_single_family(root, SchemaFamily::Events, &args),
        SchemaCommand::Errors(args) => run_single_family(root, SchemaFamily::Errors, &args),
        SchemaCommand::Database(args) => run_single_family(root, SchemaFamily::Database, &args),
        SchemaCommand::Graph(args) => run_single_family(root, SchemaFamily::Graph, &args),
        SchemaCommand::VectorPayload(args) => {
            run_single_family(root, SchemaFamily::VectorPayload, &args)
        }
        SchemaCommand::Providers(args) => run_single_family(root, SchemaFamily::Providers, &args),
        SchemaCommand::Adapters(args) => run_single_family(root, SchemaFamily::Adapters, &args),
    }
}

fn selected_families(family: Option<SchemaFamily>) -> Vec<SchemaFamily> {
    family.map_or_else(all_families, |family| vec![family])
}

fn run_single_family(root: &Path, family: SchemaFamily, args: &SchemaGenerateArgs) -> Result<()> {
    if args.family.is_some() {
        bail!("--family is only valid with aggregate `schemas generate`");
    }
    run_families(root, vec![family], args)
}

fn run_families(root: &Path, families: Vec<SchemaFamily>, args: &SchemaGenerateArgs) -> Result<()> {
    if args.update_fixtures && std::env::var_os("CI").is_some() {
        bail!("--update-fixtures is forbidden in CI");
    }
    if args.print && args.json {
        bail!("--print and --json are mutually exclusive because both write stdout");
    }

    let mut reports = Vec::new();
    for family in families {
        let artifacts = generator_for(family).generate(root)?;
        let artifact_index = artifact_index::ArtifactIndex::from_generated(family, &artifacts)?;
        let validation_mode = if args.update_fixtures {
            validate::ValidationMode::UpdateFixtures
        } else {
            validate::ValidationMode::Check
        };
        let validation_report =
            validate::validate_family(root, family, &artifact_index, validation_mode)?;
        let mut structural_drift = Vec::new();
        let removed_surface_report = removed::removed_surface_absence_report(&artifacts);
        let structural_warnings =
            match removed::assert_removed_surface_absent(&removed_surface_report) {
                Ok(()) => Vec::new(),
                Err(err) => err.to_string().lines().map(str::to_owned).collect(),
            };
        if let Err(err) = registry::check_enum_projection_drift(&artifacts) {
            structural_drift.push(err.to_string());
        }
        if let Err(err) = cross_check::check_dangling_refs(&artifact_index) {
            structural_drift.push(err.to_string());
        }
        if args.print {
            print_artifacts(&artifacts)?;
            let mut report = FamilyReport::from_drift(family, artifacts.len(), structural_drift)
                .with_validation_counts(&validation_report);
            report.warnings.extend(structural_warnings);
            reports.push(report);
            continue;
        }
        if args.check {
            let mut drift = collect_drift(root, &artifacts)?;
            drift.extend(structural_drift);
            let mut report = FamilyReport::from_drift(family, artifacts.len(), drift)
                .with_validation_counts(&validation_report);
            report.warnings.extend(structural_warnings);
            reports.push(report);
        } else if structural_drift.is_empty() {
            write_artifacts(root, &artifacts)?;
            let mut report = FamilyReport::ok(family, artifacts.len())
                .with_validation_counts(&validation_report);
            report.warnings.extend(structural_warnings);
            reports.push(report);
        } else {
            let mut report = FamilyReport::from_drift(family, artifacts.len(), structural_drift)
                .with_validation_counts(&validation_report);
            report.warnings.extend(structural_warnings);
            reports.push(report);
        }
    }

    let drift = reports
        .iter()
        .flat_map(|report| report.drift.iter().cloned())
        .collect::<Vec<_>>();
    if args.json {
        print_report(&reports)?;
    }
    if !drift.is_empty() {
        bail!("schema artifacts are stale:\n{}", drift.join("\n"));
    }
    Ok(())
}

fn print_artifacts(artifacts: &[artifact::SchemaArtifact]) -> Result<()> {
    for artifact in artifacts {
        println!("--- {}", artifact.path.display());
        print!("{}", artifact.content);
        if !artifact.content.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn print_report(reports: &[FamilyReport]) -> Result<()> {
    let mut content = serde_json::to_string_pretty(reports)?;
    content.push('\n');
    print!("{content}");
    Ok(())
}

fn collect_drift(root: &Path, artifacts: &[artifact::SchemaArtifact]) -> Result<Vec<String>> {
    let mut drift = Vec::new();
    for artifact in artifacts {
        let path = root.join(&artifact.path);
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == artifact.content => {}
            Ok(_) => drift.push(format!(
                "{} differs; run `cargo xtask schemas generate`",
                artifact.path.display()
            )),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => drift.push(format!(
                "{} is missing; run `cargo xtask schemas generate`",
                artifact.path.display()
            )),
            Err(err) => return Err(err.into()),
        }
    }
    Ok(drift)
}

fn write_artifacts(root: &Path, artifacts: &[artifact::SchemaArtifact]) -> Result<()> {
    for artifact in artifacts {
        let path = root.join(&artifact.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &artifact.content)?;
    }
    Ok(())
}

fn rel(path: impl Into<PathBuf>) -> PathBuf {
    path.into()
}

#[cfg(test)]
#[path = "schemas/tests.rs"]
mod tests;
