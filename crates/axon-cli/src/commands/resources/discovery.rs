use super::{flag_value, positional, print_value};
use axon_core::config::Config;
use axon_services::context::ServiceContext;
use axon_services::service_traits::{CollectionService, CollectionServiceImpl};
use std::error::Error;
use std::sync::Arc;

pub(super) async fn run_collections(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let service = CollectionServiceImpl::new(Arc::new(context.clone()));
    let collections = service.list().await?;
    match cfg.positional.first().map(String::as_str) {
        Some("list") => print_value(serde_json::json!({ "collections": collections })),
        Some("get") => {
            let name = positional(cfg, 1, "collection")?;
            let collection = collections
                .into_iter()
                .find(|item| item.collection == name)
                .ok_or_else(|| format!("collection {name} not found"))?;
            print_value(collection)
        }
        Some(other) => Err(format!("unknown collections subcommand: {other}").into()),
        None => Err("collections requires list|get".into()),
    }
}

pub(super) async fn run_providers(
    cfg: &Config,
    context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let doctor = axon_services::system::doctor(context)
        .await
        .map_err(|error| -> Box<dyn Error> { error.to_string().into() })?;
    let providers = provider_summaries(&doctor.payload);
    match cfg.positional.first().map(String::as_str) {
        Some("list") => {
            let kind = flag_value(cfg, "--kind");
            let status = flag_value(cfg, "--status");
            let filtered = providers
                .into_iter()
                .filter(|provider| {
                    kind.as_deref().is_none_or(|kind| provider["id"] == kind)
                        && status.as_deref().is_none_or(|status| match status {
                            "healthy" | "ok" => provider["ok"] == true,
                            "unhealthy" | "error" => provider["ok"] == false,
                            _ => false,
                        })
                })
                .collect::<Vec<_>>();
            print_value(serde_json::json!({ "providers": filtered }))
        }
        Some("get") => {
            let id = positional(cfg, 1, "provider")?;
            let provider = providers
                .into_iter()
                .find(|provider| provider["id"] == id)
                .ok_or_else(|| format!("provider {id} not found"))?;
            print_value(provider)
        }
        Some(other) => Err(format!("unknown providers subcommand: {other}").into()),
        None => Err("providers requires list|get".into()),
    }
}

pub(super) fn run_capabilities(_cfg: &Config) -> Result<(), Box<dyn Error>> {
    print_value(axon_services::types::ServerInfo::rest_capabilities())
}

fn provider_summaries(payload: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(services) = payload
        .get("services")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let mut providers = services
        .iter()
        .map(|(id, detail)| {
            serde_json::json!({
                "id": id,
                "ok": detail.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
                "detail": detail,
            })
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    providers
}
