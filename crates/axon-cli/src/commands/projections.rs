use std::error::Error;
use std::fs;
use std::io::Write;

use axon_api::source::*;
use axon_core::config::{CommandKind, Config};
use axon_services::context::ServiceContext;
use axon_services::projections::{
    SourceAccessPolicy, execute_code_search_projection_batch, execute_source_projection_batch,
    preflight_code_search_batch, preflight_source_batch,
};

pub async fn run_projection(cfg: &Config, ctx: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match cfg.command {
        CommandKind::CodeSearch => run_code_search(cfg, ctx).await,
        CommandKind::Scrape | CommandKind::Crawl | CommandKind::Embed | CommandKind::Ingest => {
            run_source_projection(cfg, ctx).await
        }
        _ => Err("run_projection called for a non-projection command".into()),
    }
}

async fn run_source_projection(cfg: &Config, ctx: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let (operation, requests) = match cfg.command {
        CommandKind::Scrape => (
            ProjectionOperation::Scrape,
            scrape_requests_from_config(cfg)?,
        ),
        CommandKind::Crawl => (
            ProjectionOperation::Crawl,
            project_crawl(&load_source_request::<CrawlRequest>(cfg)?)?,
        ),
        CommandKind::Embed => (
            ProjectionOperation::Embed,
            project_embed(&load_source_request::<EmbedRequest>(cfg)?)?,
        ),
        CommandKind::Ingest => (
            ProjectionOperation::Ingest,
            project_ingest(&load_source_request::<IngestRequest>(cfg)?)?,
        ),
        _ => unreachable!(),
    };
    validate_output_policy(cfg, requests.len())?;
    let prepared = preflight_source_batch(
        operation,
        requests,
        None,
        &cfg.projection_batch,
        &SourceAccessPolicy::default(),
    )?;
    let result = execute_source_projection_batch(ctx, operation, prepared, None).await?;
    print_batch(cfg, &result)
}

fn scrape_requests_from_config(cfg: &Config) -> Result<Vec<SourceRequest>, Box<dyn Error>> {
    reject_mixed_inputs(cfg)?;
    let mut options = ScrapeOptions::default();
    options.collection = Some(cfg.collection.clone());
    options.execution.mode = ExecutionMode::Foreground;
    options.execution.detached = false;
    if cfg.scrape_inline {
        options.output.response_mode = ResponseMode::Inline;
    }
    if cfg.output_path.is_some() {
        options.output.artifact_mode = ArtifactMode::Always;
    }
    let request = if cfg.projection_request_file.is_some() || !cfg.projection_items.is_empty() {
        let mut request = load_source_request::<ScrapeRequest>(cfg)?;
        request.options = options;
        request
    } else {
        ScrapeRequest {
            inputs: cfg
                .positional
                .iter()
                .cloned()
                .map(|input| SourceProjectionInput {
                    input,
                    idempotency_key: None,
                })
                .collect(),
            options,
        }
    };
    let mut requests = project_scrape(&request)?;
    for request in &mut requests {
        request.embed = cfg.embed;
    }
    Ok(requests)
}

async fn run_code_search(cfg: &Config, ctx: &ServiceContext) -> Result<(), Box<dyn Error>> {
    let request = load_code_search_request(cfg)?;
    let plans = project_code_search(&request)?;
    validate_output_policy(cfg, plans.len())?;
    let prepared = preflight_code_search_batch(plans, &cfg.projection_batch)?;
    let result =
        execute_code_search_projection_batch(ctx, prepared, axon_api::CodeSearchCaller::Cli, None)
            .await?;
    print_batch(cfg, &result)
}

fn reject_mixed_inputs(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let kinds = [
        !cfg.positional.is_empty(),
        !cfg.projection_items.is_empty(),
        cfg.projection_request_file.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if kinds != 1 {
        return Err("provide exactly one of positional inputs, --item, or --request-file".into());
    }
    Ok(())
}

fn load_source_request<R>(cfg: &Config) -> Result<R, Box<dyn Error>>
where
    R: serde::de::DeserializeOwned + ProjectionRequestOptions,
    DefaultOptions<R>: Default,
{
    reject_mixed_inputs(cfg)?;
    if let Some(path) = &cfg.projection_request_file {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    let inputs = if cfg.projection_items.is_empty() {
        cfg.positional
            .iter()
            .cloned()
            .map(|input| SourceProjectionInput {
                input,
                idempotency_key: None,
            })
            .collect()
    } else {
        cfg.projection_items
            .iter()
            .map(|item| serde_json::from_str(item))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(R::new(inputs, <DefaultOptions<R>>::default()))
}

trait ProjectionRequestOptions {
    type Options: Default;
    fn new(inputs: Vec<SourceProjectionInput>, options: Self::Options) -> Self;
}
type DefaultOptions<R> = <R as ProjectionRequestOptions>::Options;

macro_rules! source_request_loader {
    ($request:ty, $options:ty) => {
        impl ProjectionRequestOptions for $request {
            type Options = $options;
            fn new(inputs: Vec<SourceProjectionInput>, options: Self::Options) -> Self {
                Self { inputs, options }
            }
        }
    };
}
source_request_loader!(ScrapeRequest, ScrapeOptions);
source_request_loader!(CrawlRequest, CrawlOptions);
source_request_loader!(EmbedRequest, EmbedOptions);
source_request_loader!(IngestRequest, IngestOptions);

fn load_code_search_request(cfg: &Config) -> Result<CodeSearchRequest, Box<dyn Error>> {
    reject_mixed_inputs(cfg)?;
    if let Some(path) = &cfg.projection_request_file {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    let inputs = if cfg.projection_items.is_empty() {
        cfg.positional
            .iter()
            .cloned()
            .map(|input| QueryProjectionInput { input })
            .collect()
    } else {
        cfg.projection_items
            .iter()
            .map(|item| serde_json::from_str(item))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(CodeSearchRequest {
        inputs,
        options: CodeSearchProjectionOptions {
            collection: Some(cfg.collection.clone()),
            limit: cfg.search_limit,
            ..CodeSearchProjectionOptions::default()
        },
    })
}

fn print_batch<T: serde::Serialize>(
    cfg: &Config,
    result: &BatchResult<T>,
) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_vec_pretty(result)?;
    if let Some(path) = &cfg.output_path {
        write_atomic_no_clobber(path, &json)?;
    } else if let Some(directory) = &cfg.projection_output_dir {
        write_batch_directory(
            directory,
            cfg.projection_output_template.as_deref(),
            &result.items,
        )?;
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(())
}

fn validate_output_policy(cfg: &Config, item_count: usize) -> Result<(), Box<dyn Error>> {
    if item_count > 1 && cfg.output_path.is_some() {
        return Err(
            "--output is only valid for a one-item projection batch; use --output-dir".into(),
        );
    }
    if let Some(template) = &cfg.projection_output_template
        && !valid_output_template(template)
    {
        return Err("--output-template accepts only literal text plus {index} or {input_hash}, must include one placeholder, and cannot contain path separators".into());
    }
    Ok(())
}

fn valid_output_template(template: &str) -> bool {
    if template.contains('/') || template.contains('\\') || template == "." || template == ".." {
        return false;
    }
    let stripped = template.replace("{index}", "").replace("{input_hash}", "");
    (template.contains("{index}") || template.contains("{input_hash}"))
        && !stripped.contains('{')
        && !stripped.contains('}')
}

fn write_batch_directory<T: serde::Serialize>(
    directory: &std::path::Path,
    template: Option<&str>,
    items: &[BatchItem<T>],
) -> Result<(), Box<dyn Error>> {
    let directory = fs::canonicalize(directory)?;
    if !directory.is_dir() {
        return Err("--output-dir must name an existing directory".into());
    }
    let template = template.unwrap_or("{index}.json");
    if !valid_output_template(template) {
        return Err("invalid projection output template".into());
    }
    for item in items {
        let input_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(item.input.as_deref().unwrap_or("redacted").as_bytes());
            hasher.update(item.index.to_le_bytes());
            hex::encode(hasher.finalize())
        };
        let filename = template
            .replace("{index}", &item.index.to_string())
            .replace("{input_hash}", &input_hash);
        if filename.contains('/') || filename.contains('\\') || filename == "." || filename == ".."
        {
            return Err("projection output template produced an unsafe filename".into());
        }
        write_atomic_no_clobber(&directory.join(filename), &serde_json::to_vec_pretty(item)?)?;
    }
    Ok(())
}

fn write_atomic_no_clobber(path: &std::path::Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let parent = fs::canonicalize(parent)?;
    let name = path
        .file_name()
        .ok_or("--output must name a file")?
        .to_string_lossy();
    if name == "." || name == ".." {
        return Err("--output must name a file".into());
    }
    let target = parent.join(name.as_ref());
    if fs::symlink_metadata(&target).is_ok() {
        return Err(format!("output already exists: {}", target.display()).into());
    }
    let temporary = parent.join(format!(".axon-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::hard_link(&temporary, &target)?;
        fs::remove_file(&temporary)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "projections_tests.rs"]
mod tests;
